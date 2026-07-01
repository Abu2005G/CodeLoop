use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

mod agent;
mod config;
mod llm;
mod tools;

use agent::Agent;
use config::Config;
use llm::deepseek::DeepSeekClient;

#[derive(Parser, Debug)]
#[command(
    name = "codeloop",
    version,
    about = "A loop-based coding agent powered by DeepSeek"
)]
struct Cli {
    /// The coding task to perform
    #[arg(required_unless_present = "task_file")]
    task: Option<String>,

    /// Read task from a file
    #[arg(long)]
    task_file: Option<String>,

    /// DeepSeek model to use
    #[arg(long, default_value = "deepseek-chat")]
    model: String,

    /// Maximum agent loop iterations
    #[arg(long, default_value = "20")]
    max_iter: u32,

    /// Workspace directory (default: current directory)
    #[arg(long)]
    workspace: Option<PathBuf>,

    /// DeepSeek API key (or set DEEPSEEK_API_KEY env var)
    #[arg(long)]
    api_key: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("codeloop=info".parse()?))
        .init();

    let cli = Cli::parse();

    let task = match (cli.task, cli.task_file) {
        (Some(t), _) => t,
        (_, Some(f)) => std::fs::read_to_string(&f)?,
        _ => anyhow::bail!("Provide a task or --task-file"),
    };

    let config = Config::new(
        cli.api_key,
        Some(cli.model),
        Some(cli.max_iter),
        cli.workspace,
    )?;

    let client = Box::new(DeepSeekClient::new(
        config.api_key.clone(),
        config.model.clone(),
    ));
    let tools = tools::default_tools();

    let mut agent = Agent::new(client, tools, &config);
    let result = agent.run(&task).await?;

    println!("\n{result}");
    Ok(())
}
