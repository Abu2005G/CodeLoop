use super::{Tool, ToolError};
use serde_json::json;
use std::fs;
use std::path::Path;

pub struct ReadFileTool;

#[async_trait::async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a file from the filesystem. Use offset and limit for long files."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" },
                "offset": { "type": "integer", "description": "Line number to start from (1-indexed)" },
                "limit": { "type": "integer", "description": "Number of lines to read" }
            },
            "required": ["path"]
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

            let full_path = if std::path::Path::new(path).is_absolute() {
                std::path::PathBuf::from(path)
            } else {
                ws.join(path)
            };

            let offset = args["offset"].as_u64().unwrap_or(1).max(1);
            let limit = args["limit"].as_u64();

            let content = fs::read_to_string(&full_path)
                .map_err(|e| ToolError::Message(format!("Cannot read {path}: {e}")))?;

            let lines: Vec<&str> = content.lines().collect();
            let start = (offset - 1) as usize;
            let end = limit.map_or(lines.len(), |l| (start + l as usize).min(lines.len()));

            let output: Vec<String> = lines[start..end]
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:>6}: {}", start + i + 1, line))
                .collect();

            Ok(output.join("\n"))
        })
        .await
        .map_err(|e| ToolError::Message(format!("Task panicked: {e}")))?
    }
}
