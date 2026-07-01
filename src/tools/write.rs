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
        "Write or overwrite a file with the given content."
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

            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| ToolError::Message(format!("Cannot create dir: {e}")))?;
            }
            fs::write(&full_path, content)
                .map_err(|e| ToolError::Message(format!("Cannot write {path}: {e}")))?;

            Ok(format!("Wrote {} bytes to {path}", content.len()))
        })
        .await
        .map_err(|e| ToolError::Message(format!("Task panicked: {e}")))?
    }
}
