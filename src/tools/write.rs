use super::{Tool, ToolError};
use serde_json::json;
use std::fs;
use std::path::Path;

pub struct WriteFileTool;

#[async_trait::async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write or overwrite a file. Shows a diff summary for existing files."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to write" },
                "content": { "type": "string", "description": "Content to write to the file" }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        workspace: &Path,
    ) -> Result<String, ToolError> {
        let ws = workspace.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let path = args["path"]
                .as_str()
                .ok_or_else(|| ToolError::Message("Missing path".into()))?;
            let content = args["content"]
                .as_str()
                .ok_or_else(|| ToolError::Message("Missing content".into()))?;

            let full_path = if std::path::Path::new(path).is_absolute() {
                std::path::PathBuf::from(path)
            } else {
                ws.join(path)
            };

            let old_content = fs::read_to_string(&full_path).ok();

            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| ToolError::Message(format!("Cannot create dir: {e}")))?;
            }
            fs::write(&full_path, content)
                .map_err(|e| ToolError::Message(format!("Cannot write {path}: {e}")))?;

            let mut result = format!("Wrote {} bytes to {path}", content.len());

            if let Some(old) = old_content {
                result.push_str(&diff_summary(&old, content));
            } else {
                let lines = content.lines().count();
                result.push_str(&format!(
                    "\n[D] New file: {lines} lines, {} bytes",
                    content.len()
                ));
            }

            Ok(result)
        })
        .await
        .map_err(|e| ToolError::Message(format!("Task panicked: {e}")))?
    }
}

fn diff_summary(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    if old_lines == new_lines {
        return "\n[D] No changes detected.".to_string();
    }

    let old_set: std::collections::HashSet<&&str> = old_lines.iter().collect();
    let new_set: std::collections::HashSet<&&str> = new_lines.iter().collect();

    let added = new_lines.iter().filter(|l| !old_set.contains(l)).count();
    let removed = old_lines.iter().filter(|l| !new_set.contains(l)).count();

    let mut summary = format!(
        "\n[D] {} → {} lines (+{}, -{})",
        old_lines.len(),
        new_lines.len(),
        added,
        removed,
    );

    // Show first few added/removed lines as a preview
    if added > 0 {
        let previews: Vec<&str> = new_lines
            .iter()
            .filter(|l| !old_set.contains(*l))
            .take(3)
            .copied()
            .collect();
        if !previews.is_empty() {
            summary.push_str("\n[D] Added:");
            for p in previews {
                summary.push_str(&format!("\n    + {p}"));
            }
        }
    }

    if removed > 0 {
        let previews: Vec<&str> = old_lines
            .iter()
            .filter(|l| !new_set.contains(*l))
            .take(3)
            .copied()
            .collect();
        if !previews.is_empty() {
            summary.push_str("\n[D] Removed:");
            for p in previews {
                summary.push_str(&format!("\n    - {p}"));
            }
        }
    }

    summary
}
