use super::{should_skip, Tool, ToolError};
use serde_json::json;
use std::path::Path;

pub struct ListFilesTool;

#[async_trait::async_trait]
impl Tool for ListFilesTool {
    fn name(&self) -> &str {
        "list_files"
    }

    fn description(&self) -> &str {
        "List files matching a glob pattern. Skips common ignore directories."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. 'src/**/*.rs')" },
                "max_results": { "type": "integer", "description": "Max files to return (default 50)" }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        workspace: &Path,
    ) -> Result<String, ToolError> {
        let ws = workspace.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let pattern = args["pattern"]
                .as_str()
                .ok_or_else(|| ToolError::Message("Missing pattern".into()))?;
            let max = args["max_results"].as_u64().unwrap_or(50) as usize;

            let pattern_path = ws.join(pattern);
            let entries = ::glob::glob(&pattern_path.to_string_lossy())
                .map_err(|e| ToolError::Message(format!("Invalid glob: {e}")))?;

            let mut results = Vec::new();
            for entry in entries.flatten() {
                if should_skip(&entry) {
                    continue;
                }
                if let Ok(rel) = entry.strip_prefix(&ws) {
                    results.push(rel.display().to_string());
                    if results.len() >= max {
                        break;
                    }
                }
            }

            if results.is_empty() {
                Ok("No files matched.".into())
            } else {
                Ok(results.join("\n"))
            }
        })
        .await
        .map_err(|e| ToolError::Message(format!("Task panicked: {e}")))?
    }
}
