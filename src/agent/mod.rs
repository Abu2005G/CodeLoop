use crate::config::Config;
use crate::llm::{LlmClient, Message, ToolCall};
use crate::tools::ToolRegistry;
use std::io::{self, Write};
use tracing::{info, warn};

pub struct PlanState {
    pub current_objective: String,
    pub last_error: Option<String>,
    pub reflection_needed: bool,
}

impl PlanState {
    pub fn new(objective: &str) -> Self {
        Self {
            current_objective: objective.to_string(),
            last_error: None,
            reflection_needed: false,
        }
    }
}

pub struct Agent {
    client: Box<dyn LlmClient>,
    tools: ToolRegistry,
    workspace: std::path::PathBuf,
    max_iterations: u32,
    history: Vec<Message>,
    plan_state: PlanState,
    auto_mode: bool,
}

impl Agent {
    pub fn new(
        client: Box<dyn LlmClient>,
        tools: ToolRegistry,
        config: &Config,
        auto_mode: bool,
    ) -> Self {
        Self {
            client,
            tools,
            workspace: config.workspace.clone(),
            max_iterations: config.max_iterations,
            history: Vec::new(),
            plan_state: PlanState::new(""),
            auto_mode,
        }
    }

    fn build_system_prompt(&self) -> String {
        let reflection = if self.plan_state.reflection_needed {
            format!(
                "\n\nREFLECTION REQUIRED — Last error: {}\n\
                 Analyze the error above. Identify the root cause. \
                 Propose a different approach. Do NOT repeat the same failing action.",
                self.plan_state.last_error.as_deref().unwrap_or("Unknown")
            )
        } else {
            String::new()
        };

        format!(
            "You are an expert software architect. Follow this protocol strictly:\n\
             \n\
             PHASE 1: ANALYSIS & PLANNING\n\
             1. Explore the codebase: use list_files and search_code to understand structure.\n\
             2. Read relevant files to understand the current implementation.\n\
             3. Produce a structured plan.\n\
             \n\
             PHASE 2: EXECUTION & VERIFICATION\n\
             1. Execute one step at a time using the appropriate tools.\n\
             2. After EVERY write_file, run test_verify to ensure correctness.\n\
             3. On failure: analyze the error, identify the root cause, propose a fix.\n\
                NEVER attempt the same failing approach twice.\n\
             4. Do NOT use bash_exec to run tests — always use test_verify.\n\
             \n\
             PHASE 3: FINAL REVIEW\n\
             1. Run the full test suite with test_verify.\n\
             2. Summarize all changes made.\n\
             3. Do NOT call any more tools after the summary.\n\
             \n\
             Current objective: {}\n\
             Workspace: {}{}",
            self.plan_state.current_objective,
            self.workspace.display(),
            reflection,
        )
    }

    fn system_prompt_message(&self) -> Message {
        Message::system(&self.build_system_prompt())
    }

    fn format_tool_calls(tool_calls: &[ToolCall]) -> String {
        tool_calls
            .iter()
            .map(|tc| {
                let pretty_args: String =
                    match serde_json::from_str::<serde_json::Value>(&tc.function.arguments) {
                        Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_default(),
                        Err(_) => tc.function.arguments.clone(),
                    };
                format!("  Tool: {}\n  Args: {}", tc.function.name, pretty_args)
            })
            .collect::<Vec<_>>()
            .join("\n---\n")
    }

    fn hitl_gate(&self, tool_calls: &[ToolCall], iteration: u32) -> io::Result<HitlDecision> {
        println!();
        println!("═══════════════════════════════════════════════════════════");
        println!("  ITERATION {} — AGENT PROPOSAL", iteration);
        println!("═══════════════════════════════════════════════════════════");
        println!("{}", Self::format_tool_calls(tool_calls));
        println!("═══════════════════════════════════════════════════════════");

        loop {
            print!("  [y] approve  [n] reject + feedback  [q] quit\n  > ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim().to_lowercase().as_str() {
                "y" | "yes" => return Ok(HitlDecision::Approve),
                "n" | "no" => {
                    print!("  Feedback (or press Enter to skip): ");
                    io::stdout().flush()?;
                    let mut feedback = String::new();
                    io::stdin().read_line(&mut feedback)?;
                    let fb = feedback.trim().to_string();
                    return Ok(HitlDecision::Reject(if fb.is_empty() {
                        None
                    } else {
                        Some(fb)
                    }));
                }
                "q" | "quit" => return Ok(HitlDecision::Quit),
                other => {
                    println!("  Unknown: '{}'. Enter y, n, or q.", other);
                }
            }
        }
    }

    pub async fn run(&mut self, task: &str) -> anyhow::Result<String> {
        self.plan_state = PlanState::new(task);
        self.history.clear();
        self.history.push(self.system_prompt_message());
        self.history.push(Message::user(&format!(
            "Task: {}\n\nBegin with Phase 1 — explore the codebase and produce a plan.",
            task
        )));

        let tool_defs = self.tools.get_definitions();

        for iteration in 1..=self.max_iterations {
            info!(iteration, "Calling LLM...");

            self.history[0] = self.system_prompt_message();

            let response = self.client.chat(&self.history, &tool_defs).await?;

            if !response.tool_calls.is_empty() {
                self.history.push(Message::assistant(
                    response.content.clone(),
                    Some(response.tool_calls.clone()),
                ));

                if !self.auto_mode {
                    match self.hitl_gate(&response.tool_calls, iteration)? {
                        HitlDecision::Approve => { /* proceed */ }
                        HitlDecision::Reject(feedback) => {
                            let msg = if let Some(fb) = feedback {
                                format!("User rejected the proposed action. Feedback: {fb}\nPlease propose a different approach.")
                            } else {
                                "User rejected the proposed action. Please propose a different approach."
                                    .to_string()
                            };
                            self.history.push(Message::user(&msg));
                            self.plan_state.reflection_needed = true;
                            continue;
                        }
                        HitlDecision::Quit => {
                            return Ok("Agent terminated by user.".to_string());
                        }
                    }
                }

                for tc in &response.tool_calls {
                    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)?;
                    info!(
                        "Executing tool: {} with args: {}",
                        tc.function.name, tc.function.arguments
                    );
                    let result = match self.tools.find_by_name(&tc.function.name) {
                        Some(tool) => match tool.execute(args, &self.workspace).await {
                            Ok(output) => {
                                self.plan_state.reflection_needed = false;
                                self.plan_state.last_error = None;
                                output
                            }
                            Err(e) => {
                                let err_msg = format!("Error: {e}");
                                self.plan_state.last_error = Some(err_msg.clone());
                                self.plan_state.reflection_needed = true;
                                warn!("Tool error: {e}");
                                err_msg
                            }
                        },
                        None => {
                            let err_msg = format!("Unknown tool: {}", tc.function.name);
                            self.plan_state.last_error = Some(err_msg.clone());
                            self.plan_state.reflection_needed = true;
                            err_msg
                        }
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
            "Maximum iterations reached. Provide a summary of what was accomplished and what remains.",
        ));
        let response = self.client.chat(&self.history, &[]).await?;
        let summary = response
            .content
            .unwrap_or_else(|| "No summary available.".to_string());
        Ok(format!("MAX_ITERATIONS_REACHED. {summary}"))
    }
}

enum HitlDecision {
    Approve,
    Reject(Option<String>),
    Quit,
}
