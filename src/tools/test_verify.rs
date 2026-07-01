use super::{Tool, ToolError};
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

pub struct TestVerifyTool;

#[async_trait::async_trait]
impl Tool for TestVerifyTool {
    fn name(&self) -> &str {
        "test_verify"
    }

    fn description(&self) -> &str {
        "Auto-detect the test framework in the workspace and run tests.\
         Use this after every write_file to verify correctness.\
         Pass changed_files to run targeted tests."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "changed_files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of files that were modified (used to target tests)"
                },
                "framework": {
                    "type": "string",
                    "enum": ["auto", "rust", "node", "python", "go"],
                    "description": "Test framework. Use 'auto' to auto-detect (default)."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default 300 for tests)"
                }
            },
            "required": []
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        workspace: &Path,
    ) -> Result<String, ToolError> {
        let changed_files: Vec<String> = args["changed_files"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let framework = args["framework"].as_str().unwrap_or("auto").to_string();
        let _timeout_secs = args["timeout_secs"].as_u64().unwrap_or(300);

        let ws = workspace.to_path_buf();

        tokio::task::spawn_blocking(move || {
            let command = detect_and_build_command(&ws, &framework, &changed_files)
                .map_err(ToolError::Message)?;

            let child = Command::new("sh")
                .arg("-c")
                .arg(&command)
                .current_dir(&ws)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| ToolError::Message(format!("Test command failed to spawn: {e}")))?;

            let output = match child.wait_with_output() {
                Ok(o) => o,
                Err(e) => return Err(ToolError::Message(format!("Test execution failed: {e}"))),
            };

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut result = format!("[Test command] {command}\n\n");

            if !stdout.is_empty() {
                result.push_str(&stdout);
            }
            if !stderr.is_empty() {
                result.push_str(&format!("\n[stderr]\n{}", stderr));
            }
            result.push_str(&format!(
                "\n\nExit code: {}",
                output.status.code().unwrap_or(-1)
            ));

            Ok(result)
        })
        .await
        .map_err(|e| ToolError::Message(format!("Task panicked: {e}")))?
    }
}

fn detect_and_build_command(
    workspace: &Path,
    framework: &str,
    changed_files: &[String],
) -> Result<String, String> {
    let effective_fw = if framework == "auto" {
        detect_framework(workspace)?
    } else {
        framework.to_string()
    };

    match effective_fw.as_str() {
        "rust" => build_rust_command(changed_files),
        "node" => Ok("npm test".to_string()),
        "python" => {
            if changed_files.is_empty() {
                Ok("python -m pytest".to_string())
            } else {
                let files: Vec<&str> = changed_files.iter().map(|s| s.as_str()).collect();
                Ok(format!("python -m pytest {}", files.join(" ")))
            }
        }
        "go" => {
            if changed_files.is_empty() {
                Ok("go test ./...".to_string())
            } else {
                let dirs: Vec<String> = changed_files
                    .iter()
                    .filter_map(|f| {
                        std::path::Path::new(f)
                            .parent()
                            .map(|p| p.display().to_string())
                    })
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                Ok(format!("go test {}", dirs.join(" ")))
            }
        }
        other => Err(format!("Unknown or unsupported framework: {other}")),
    }
}

fn detect_framework(workspace: &Path) -> Result<String, String> {
    if workspace.join("Cargo.toml").exists() {
        return Ok("rust".to_string());
    }
    if workspace.join("package.json").exists() {
        return Ok("node".to_string());
    }
    if workspace.join("pyproject.toml").exists()
        || workspace.join("setup.py").exists()
        || workspace.join("setup.cfg").exists()
    {
        return Ok("python".to_string());
    }
    if workspace.join("go.mod").exists() {
        return Ok("go".to_string());
    }
    Err("Could not auto-detect test framework. Pass --framework explicitly.".to_string())
}

fn build_rust_command(changed_files: &[String]) -> Result<String, String> {
    if changed_files.is_empty() {
        return Ok("cargo test".to_string());
    }

    let filters: Vec<String> = changed_files
        .iter()
        .filter_map(|f| derive_rust_test_filter(f))
        .collect();

    if filters.is_empty() {
        return Ok("cargo test".to_string());
    }

    let filter_str = filters.join(" -- ");
    Ok(format!("cargo test -- {filter_str}"))
}

fn derive_rust_test_filter(file_path: &str) -> Option<String> {
    let path = std::path::Path::new(file_path);

    let stem = path.file_stem()?.to_str()?;

    if stem == "mod" {
        if let Some(parent) = path.parent() {
            if let Some(dir_name) = parent.file_name() {
                return Some(dir_name.to_str()?.to_string());
            }
        }
    }

    Some(stem.to_string())
}
