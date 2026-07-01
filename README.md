# codeloop

A loop-based coding agent powered by DeepSeek. Give it a task and it reads your codebase, writes code, runs commands, and iterates until the job is done — all autonomously.

```
┌──────────┐     ┌──────────────┐     ┌──────────┐
│  LLM     │────▶│  Tool Calls  │────▶│  Local   │
│ (DeepSeek)│◀────│  (results)   │◀────│  System  │
└──────────┘     └──────────────┘     └──────────┘
       ▲                                    │
       └──────── agentic loop ──────────────┘
```

## Prerequisites

- **Rust** 1.75+ ([install via rustup](https://rustup.rs))
- **DeepSeek API key** ([platform.deepseek.com](https://platform.deepseek.com))

## Installation

```bash
# Clone and build
git clone <repo-url> codeloop
cd codeloop
cargo build --release

# The binary is at target/release/codeloop
# Optionally, add to PATH:
cp target/release/codeloop /usr/local/bin/
```

## Quickstart

```bash
# Set your API key (or pass --api-key)
export DEEPSEEK_API_KEY="sk-xxxxxxxxxxxxxxxx"

# Run a task
codeloop "Add a health-check endpoint to the Express server"

# Target a specific workspace
codeloop --workspace ~/projects/myapp "Refactor the auth module"

# Read a task from a file
codeloop --task-file instructions.md
```

## CLI Reference

```
codeloop [OPTIONS] [TASK]

Arguments:
  [TASK]                    The coding task to perform

Options:
  --task-file <FILE>        Read task from a file
  --model <MODEL>           DeepSeek model [default: deepseek-chat]
  --max-iter <N>            Max agent loop iterations [default: 20]
  --workspace <DIR>         Workspace directory [default: current dir]
  --api-key <KEY>           API key (or set DEEPSEEK_API_KEY env var)
  -h, --help                Print help
  -V, --version             Print version
```

## How It Works

codeloop operates in a **plan → act → observe → replan** loop:

1. **Prompt** — Your task + a system prompt describing available tools are sent to DeepSeek.
2. **Think** — The LLM reasons about the task and decides what action to take.
3. **Tool Call** — The LLM requests one or more tool executions (e.g. `read_file`, `search_code`).
4. **Execute** — codeloop runs the tools on your local machine and collects results.
5. **Feedback** — Results are fed back into the conversation, letting the LLM decide the next step.
6. **Repeat** — Steps 2-5 loop until the LLM signals completion (text-only response, no more tool calls) or `--max-iter` is reached.

Every interaction is stateless across runs — each `codeloop` invocation starts fresh.

## Built-in Tools

| Tool | Description |
|------|-------------|
| `read_file` | Reads a file with optional line offset/limit. Returns line-numbered output. |
| `write_file` | Writes or overwrites a file. Creates parent directories as needed. |
| `bash_exec` | Runs a shell command in the workspace directory. Enforces a configurable timeout (default 120s). Timed-out processes are killed. |
| `search_code` | Searches files for a regex pattern with an optional glob filter. Skips `target/`, `.git/`, `node_modules/`, and `.cargo/`. |
| `list_files` | Lists files matching a glob pattern. Also skips common ignore directories. |

## Architecture

```
src/
├── main.rs           # CLI entry point (clap)
├── config.rs         # API key, model, workspace resolution
├── llm/
│   ├── mod.rs        # LlmClient trait + Message/ToolCall types
│   └── deepseek.rs   # DeepSeek API client (OpenAI-compatible /v1/chat/completions)
├── tools/
│   ├── mod.rs        # Tool trait, ToolRegistry, default_tools(), skip-dir filter
│   ├── read.rs       # read_file tool
│   ├── write.rs      # write_file tool
│   ├── bash.rs       # bash_exec tool (tokio::process, timeout)
│   ├── grep.rs       # search_code tool (regex + glob)
│   └── glob.rs       # list_files tool (glob pattern matching)
└── agent/
    └── mod.rs        # Agent struct, system prompt builder, main orchestration loop
```

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| `spawn_blocking` for file I/O | Prevents synchronous FS operations from blocking the tokio async runtime |
| `canonicalize()`'d workspace | All tools operate on absolute paths — no confusion with relative references |
| `kill_on_drop(true)` for subprocesses | Timed-out commands are guaranteed to terminate, preventing zombie processes |
| Shared skip-dir filter | `target/`, `.git/`, `node_modules/`, `.cargo/` are excluded globally to avoid scanning huge dependency trees |
| OpenAI-compatible API | DeepSeek's API mirrors the OpenAI chat completions format, making the client straightforward and swappable |

## Examples

### Fix a bug
```bash
codeloop "The login handler in src/auth.rs doesn't validate email format. Fix it."
```

### Add a feature
```bash
codeloop "Add pagination to the GET /api/users endpoint. Use query params 'page' and 'limit'."
```

### Refactor code
```bash
codeloop --workspace ~/projects/backend "Extract database logic from route handlers into a service layer."
```

### Write tests
```bash
codeloop "Write unit tests for the calculateTotal function in src/utils.ts. Cover edge cases."
```

### Multi-step task from file
```bash
cat > task.md << 'EOF'
1. Create a new Rust module src/pricing.rs
2. Implement a function `fn apply_discount(price: f64, pct: f64) -> f64`
3. Add unit tests
4. Register the module in src/main.rs
EOF

codeloop --task-file task.md
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | Your DeepSeek API key (required) |
| `RUST_LOG` | Logging level (e.g. `RUST_LOG=codeloop=debug`) |

## Limitations

- No streaming — each LLM call is a blocking request/response cycle.
- No conversation persistence across runs.
- No multi-step planning phase — the agent reacts iteratively rather than planning ahead.
- No custom tool registration — the five built-in tools are fixed.
- Workspace must exist before running (canonicalization fails on nonexistent paths).

## License

MIT
