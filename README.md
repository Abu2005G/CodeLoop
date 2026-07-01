# codeloop

A loop-based coding agent powered by DeepSeek that follows an architect-grade protocol: **Plan → Get Approval → Execute → Verify → Reflect**. Not a blind code generator — it reads your codebase, proposes a structured plan, waits for your sign-off, and verifies every change.

```
┌──────────┐     ┌──────────────┐     ┌──────────────┐     ┌──────────┐
│  LLM     │────▶│  Tool Calls  │────▶│  Human Gate  │────▶│  Local   │
│ (DeepSeek)│◀────│  (results)   │◀────│  [y/n/edit]  │◀────│  System  │
└──────────┘     └──────────────┘     └──────────────┘     └──────────┘
       ▲                                                          │
       └─────────────── agentic loop ─────────────────────────────┘
```

## Prerequisites

- **Rust** 1.75+ ([install via rustup](https://rustup.rs))
- **DeepSeek API key** ([platform.deepseek.com](https://platform.deepseek.com))

## Installation

```bash
# Clone and build
git clone https://github.com/Abu2005G/CodeLoop.git
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

# Interactive mode (default) — you approve every action
codeloop "Add a health-check endpoint to the Express server"

# Unattended mode — skip human approval prompts
codeloop --auto "Refactor the auth module"

# Target a specific workspace
codeloop --workspace ~/projects/myapp "Fix the pagination bug"

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
  --auto                    Skip HITL prompts (auto-approve all tool calls)
  -h, --help                Print help
  -V, --version             Print version
```

## How It Works

codeloop follows a **3-phase architect protocol** with a human-in-the-loop gate:

### Phase 1: Analysis & Planning
1. The agent explores the codebase using `list_files` and `search_code`.
2. It deep-reads relevant files with `read_file`.
3. It produces a structured plan with explicit steps.
4. **It waits** — no files are written until you approve.

### Phase 2: Execution & Verification
1. Steps are executed one at a time.
2. Before each tool execution, you see a **HITL prompt**:

```
═══════════════════════════════════════════════════════════
  ITERATION 3 — AGENT PROPOSAL
═══════════════════════════════════════════════════════════
  Tool:  write_file
  Args:  {
           "path": "src/auth.rs",
           "content": "pub fn validate_email(...) { ... }"
         }
═══════════════════════════════════════════════════════════
  [y] approve  [n] reject + feedback  [q] quit
  >
```

   - `y` — approve and execute
   - `n` — reject, optionally provide feedback (sent back to the LLM to pivot)
   - `q` — quit the agent entirely
3. After every `write_file`, `test_verify` is run to confirm correctness.
4. **Reflection**: If a test fails or a tool errors, the agent is forced to analyze the root cause and propose a different approach. It will **never** repeat the same broken fix.

### Phase 3: Final Review
1. The full test suite is run via `test_verify`.
2. A summary of all changes is produced.
3. The agent stops — no lingering tool calls.

### Plan State Tracking
The agent maintains a persistent plan context across iterations:
- **Current Objective**: The task being executed
- **Last Error**: Most recent failure, injected into the system prompt for reflection
- **Reflection Flag**: When set, the system prompt includes a "REFLECTION REQUIRED" section forcing the LLM to pivot

Use `--auto` to skip the HITL prompts and run fully autonomously.

## Built-in Tools

| Tool | Description |
|------|-------------|
| `read_file` | Reads a file with optional line offset/limit. Returns line-numbered output. |
| `write_file` | Writes or overwrites a file. Creates parent directories as needed. Shows a **diff summary** with added/removed line counts and previews. |
| `bash_exec` | Runs a shell command with an enforced timeout (default 120s). Timed-out processes are killed via `kill_on_drop`. |
| `search_code` | Searches files for a regex pattern with an optional glob filter. Skips `target/`, `.git/`, `node_modules/`, `.cargo/`. |
| `list_files` | Lists files matching a glob pattern. Also skips common ignore directories. |
| `test_verify` | **Auto-detects** the test framework (Rust/Node/Python/Go) and runs targeted or full tests. Accepts `changed_files` to focus on modified modules. |

## Architecture

```
src/
├── main.rs           # CLI entry point (clap), --auto flag
├── config.rs         # API key, model, canonicalized workspace
├── llm/
│   ├── mod.rs        # LlmClient trait + Message/ToolCall types
│   └── deepseek.rs   # DeepSeek API client (OpenAI-compatible /v1/chat/completions)
├── tools/
│   ├── mod.rs        # Tool trait, ToolRegistry, default_tools(), skip-dir filter
│   ├── read.rs       # read_file tool
│   ├── write.rs      # write_file tool + diff summary
│   ├── bash.rs       # bash_exec tool (tokio::process, timeout, kill_on_drop)
│   ├── grep.rs       # search_code tool (regex + glob)
│   ├── glob.rs       # list_files tool (glob pattern matching)
│   └── test_verify.rs # Auto-detect framework, run targeted tests
└── agent/
    └── mod.rs        # Agent struct, PlanState, 3-phase architect prompt, HITL gate
```

### Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| 3-Phase architect protocol | Forces planning before action — reduces hallucination by making the LLM commit to a strategy first |
| Human-in-the-loop gate | Safety kill-switch; catch bad plans before files are overwritten; redirect the agent with feedback |
| `PlanState` + reflection | On tool failure, the system prompt changes to force root-cause analysis — prevents infinite retry loops |
| `test_verify` as a named tool | Tight verification loop; the LLM knows to call it after writes, and results feed directly into reflection |
| `write_file` diff summary | Every write reports lines added/removed with previews — you know exactly what changed |
| `spawn_blocking` for file I/O | Prevents synchronous FS operations from blocking the tokio async runtime |
| `canonicalize()`'d workspace | All tools operate on absolute paths — no confusion with relative references |
| `kill_on_drop(true)` for subprocesses | Timed-out commands are guaranteed to terminate, preventing zombie processes |
| Shared skip-dir filter | `target/`, `.git/`, `node_modules/`, `.cargo/` are excluded globally |
| OpenAI-compatible API | DeepSeek's API mirrors the OpenAI chat completions format, making the client straightforward and swappable |

## Examples

### Interactive: fix a bug (with approval)
```bash
codeloop "The login handler in src/auth.rs doesn't validate email format. Fix it."
# Agent explores → proposes plan → you approve → executes → verifies → done
```

### Unattended: add a feature
```bash
codeloop --auto "Add pagination to the GET /api/users endpoint with page and limit query params."
```

### Refactor with targeted verification
```bash
codeloop --auto --workspace ~/projects/backend \
  "Extract database logic from route handlers into a service layer. Verify with tests after each change."
```

### Reject and redirect
```bash
$ codeloop "Optimize the sorting algorithm"
# Agent proposes using bubble sort
  > n
  Feedback: Use quicksort instead — the dataset can be large.
# Agent pivots to quicksort implementation
```

### Multi-step from a file
```bash
cat > task.md << 'EOF'
1. Create a new Rust module src/pricing.rs
2. Implement fn apply_discount(price: f64, pct: f64) -> f64
3. Write unit tests
4. Verify with cargo test
5. Register the module in src/main.rs
EOF

codeloop --task-file task.md
```

## HITL Interaction Flow

```
  LLM proposes action → [HITL GATE] → User decides:
                                              ├─ y → Execute tools → Feed results to LLM → Loop
                                              ├─ n → Send rejection + feedback to LLM → LLM replans → Loop
                                              └─ q → Agent exits
```

In `--auto` mode, the gate is skipped entirely — all actions execute immediately.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `DEEPSEEK_API_KEY` | Your DeepSeek API key (required) |
| `RUST_LOG` | Logging level (e.g. `RUST_LOG=codeloop=debug`) |

## Limitations

- No streaming — each LLM call is a blocking request/response cycle.
- No conversation persistence across runs.
- `test_verify` framework auto-detection requires the workspace to contain standard config files (Cargo.toml, package.json, etc.).
- Workspace must exist before running (canonicalization fails on nonexistent paths).
- In `--auto` mode, there is no kill-switch — the agent runs until completion or `--max-iter`.

## License

MIT
