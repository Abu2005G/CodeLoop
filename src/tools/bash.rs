use super::{Tool, ToolError};
use serde_json::json;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

pub struct BashExecTool;

#[async_trait::async_trait]
impl Tool for BashExecTool {
    fn name(&self) -> &str {
        "bash_exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command with an enforced timeout. Returns stdout and stderr."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "timeout_secs": { "type": "integer", "description": "Timeout in seconds (default 120)" }
            },
            "required": ["command"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        workspace: &Path,
    ) -> Result<String, ToolError> {
        let cmd = args["command"]
            .as_str()
            .ok_or_else(|| ToolError::Message("Missing command".into()))?;
        let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(120);

        let child = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(workspace)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::Message(format!("Command failed to spawn: {e}")))?;

        match timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if stdout.is_empty() && stderr.is_empty() {
                    Ok(format!("Exit code: {}", output.status.code().unwrap_or(-1)))
                } else {
                    let mut result = String::new();
                    if !stdout.is_empty() {
                        result.push_str(&stdout);
                    }
                    if !stderr.is_empty() {
                        result.push_str(&format!("\n[stderr]\n{}", stderr));
                    }
                    if output.status.code() != Some(0) {
                        result.push_str(&format!(
                            "\nExit code: {}",
                            output.status.code().unwrap_or(-1)
                        ));
                    }
                    Ok(result)
                }
            }
            Ok(Err(e)) => Err(ToolError::Message(format!("Execution failed: {e}"))),
            Err(_) => Err(ToolError::Message(format!(
                "Command timed out after {timeout_secs} seconds"
            ))),
        }
    }
}
