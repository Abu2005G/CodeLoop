use crate::config::Config;
use crate::llm::{LlmClient, Message};
use crate::tools::ToolRegistry;
use tracing::info;

pub struct Agent {
    client: Box<dyn LlmClient>,
    tools: ToolRegistry,
    workspace: std::path::PathBuf,
    max_iterations: u32,
    history: Vec<Message>,
}

impl Agent {
    pub fn new(client: Box<dyn LlmClient>, tools: ToolRegistry, config: &Config) -> Self {
        Self {
            client,
            tools,
            workspace: config.workspace.clone(),
            max_iterations: config.max_iterations,
            history: Vec::new(),
        }
    }

    fn build_system_prompt(&self) -> String {
        format!(
            "You are a coding agent that helps with software engineering tasks. \
            You work in the workspace: {}. \
            You have access to tools. Use them to read files, write code, search the codebase, \
            run shell commands, and list files. \
            Think step by step. When you have completed the task, provide a summary \
            of what you did as your final response without calling any more tools. \
            Be thorough but efficient.",
            self.workspace.display()
        )
    }

    pub fn system_prompt_message(&self) -> Message {
        Message::system(&self.build_system_prompt())
    }

    pub async fn run(&mut self, task: &str) -> anyhow::Result<String> {
        self.history.clear();
        self.history.push(self.system_prompt_message());
        self.history.push(Message::user(task));

        let tool_defs = self.tools.get_definitions();

        for iteration in 1..=self.max_iterations {
            info!(iteration, "Calling LLM...");

            let response = self.client.chat(&self.history, &tool_defs).await?;

            if !response.tool_calls.is_empty() {
                self.history.push(Message::assistant(
                    response.content.clone(),
                    Some(response.tool_calls.clone()),
                ));

                for tc in &response.tool_calls {
                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)?;
                    info!(
                        "Executing tool: {} with args: {}",
                        tc.function.name, tc.function.arguments
                    );
                    let result = match self.tools.find_by_name(&tc.function.name) {
                        Some(tool) => match tool.execute(args, &self.workspace).await {
                            Ok(output) => output,
                            Err(e) => format!("Error: {e}"),
                        },
                        None => format!("Unknown tool: {}", tc.function.name),
                    };
                    self.history
                        .push(Message::tool(&tc.id, &tc.function.name, &result));
                }
            } else if let Some(content) = &response.content {
                info!("LLM finished with no tool calls.");
                self.history
                    .push(Message::assistant(Some(content.clone()), None));
                return Ok(content.clone());
            } else {
                anyhow::bail!("LLM returned neither content nor tool calls");
            }
        }

        self.history.push(Message::user(
            "Summarize what you've done so far and whether the task is complete.",
        ));
        let response = self.client.chat(&self.history, &[]).await?;
        let summary = response
            .content
            .unwrap_or_else(|| "No summary available.".into());
        Ok(format!("MAX_ITERATIONS_REACHED. {summary}"))
    }
}
