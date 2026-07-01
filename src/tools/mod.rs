use crate::llm::ToolDefinition;
use std::path::Path;

pub mod bash;
pub mod glob;
pub mod grep;
pub mod read;
pub mod write;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("{0}")]
    Message(String),
}

const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules", ".cargo"];

pub fn should_skip(path: &Path) -> bool {
    path.components()
        .any(|c| SKIP_DIRS.iter().any(|d| c.as_os_str() == *d))
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, workspace: &Path)
        -> Result<String, ToolError>;
}

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn get_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .iter()
            .map(|t| ToolDefinition::new(t.name(), t.description(), t.parameters()))
            .collect()
    }

    pub fn find_by_name(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.as_ref())
    }
}

pub fn default_tools() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(read::ReadFileTool));
    registry.register(Box::new(write::WriteFileTool));
    registry.register(Box::new(bash::BashExecTool));
    registry.register(Box::new(grep::SearchCodeTool));
    registry.register(Box::new(glob::ListFilesTool));
    registry
}
