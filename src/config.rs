use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub max_iterations: u32,
    pub workspace: PathBuf,
}

impl Config {
    pub fn new(
        api_key: Option<String>,
        model: Option<String>,
        max_iterations: Option<u32>,
        workspace: Option<PathBuf>,
    ) -> anyhow::Result<Self> {
        let api_key = api_key
            .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("DEEPSEEK_API_KEY not set. Use --api-key or set the env var.")
            })?;

        Ok(Self {
            api_key,
            model: model.unwrap_or_else(|| "deepseek-chat".into()),
            max_iterations: max_iterations.unwrap_or(20),
            workspace: {
                let raw = workspace.unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                });
                raw.canonicalize().map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to canonicalize workspace path '{}': {}",
                        raw.display(),
                        e
                    )
                })?
            },
        })
    }
}
