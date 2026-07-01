use super::{should_skip, Tool, ToolError};
use regex::Regex;
use serde_json::json;
use std::path::Path;

pub struct SearchCodeTool;

#[async_trait::async_trait]
impl Tool for SearchCodeTool {
    fn name(&self) -> &str {
        "search_code"
    }

    fn description(&self) -> &str {
        "Search for a regex pattern in files. Supports file glob filter."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search for" },
                "glob_filter": { "type": "string", "description": "Optional file glob filter (e.g. '*.rs')" },
                "max_results": { "type": "integer", "description": "Max matches to return (default 20)" }
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
            let glob_filter = args["glob_filter"].as_str().unwrap_or("**/*");
            let max = args["max_results"].as_u64().unwrap_or(20) as usize;

            let re = Regex::new(pattern)
                .map_err(|e| ToolError::Message(format!("Invalid regex: {e}")))?;

            let pattern_path = ws.join(glob_filter);
            let entries = ::glob::glob(&pattern_path.to_string_lossy())
                .map_err(|e| ToolError::Message(format!("Invalid glob: {e}")))?;

            let mut results = Vec::new();

            for entry in entries.flatten() {
                if !entry.is_file() {
                    continue;
                }
                if should_skip(&entry) {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&entry) {
                    for (i, line) in content.lines().enumerate() {
                        if re.is_match(line) {
                            let rel = entry.strip_prefix(&ws).unwrap_or(&entry);
                            results.push(format!("{}:{}: {}", rel.display(), i + 1, line));
                            if results.len() >= max {
                                break;
                            }
                        }
                    }
                }
                if results.len() >= max {
                    break;
                }
            }

            if results.is_empty() {
                Ok("No matches found.".into())
            } else {
                Ok(results.join("\n"))
            }
        })
        .await
        .map_err(|e| ToolError::Message(format!("Task panicked: {e}")))?
    }
}
