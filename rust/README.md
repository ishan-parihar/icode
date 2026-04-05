# icode — Rust CLI

A high-performance terminal AI assistant with a native ratatui TUI, multi-provider model support, and a clean-room Rust implementation.

## Quick Start

### Install

```bash
./rust/scripts/install.sh
```

Or build manually:

```bash
cd rust/
cargo build --release
./target/release/icode
```

### Use

```bash
# Interactive TUI
icode

# One-shot prompt
icode prompt "explain this codebase"

# With specific model
icode --model sonnet prompt "fix the bug in main.rs"

# With API key
ANTHROPIC_API_KEY="sk-ant-..." icode
```

## TUI

The native terminal UI is built with ratatui and inspired by OpenCode's design language:

- **Dark theme** — high-contrast terminal palette with semantic colors
- **Two-column layout** — message area (left) + collapsible sidebar (right)
- **Agent-colored borders** — visual distinction between user, assistant, and tool messages
- **Streaming display** — real-time token rendering as responses arrive
- **Tool event panels** — collapsible tool call/argument/result display
- **Status bar** — model, token count, cost, and session state
- **Model picker** (`Ctrl+M`) — searchable overlay with Favorites, Recent, and All sections; capability badges; provider-colored names
- **Keyboard navigation** — `↑/↓` scroll, `Tab` completion, `/` search in picker, `Ctrl+F` toggle favorites

### Model Picker

Open with `Ctrl+M` in the TUI:

- **Search** — type to filter across all providers
- **Sections** — Favorites → Recent → All models
- **Badges** — context window, tools, reasoning, images, price/MTok
- **Keyboard** — `Enter` select, `Esc` close, `Ctrl+F` favorite, `↑/↓` navigate

## Configuration

Set your API credentials:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export GEMINI_API_KEY="..."
```

Or authenticate via OAuth:

```bash
icode login
```

Model state (current selection, recent, favorites) persists in `~/.icode/model_state.json`.

## Multi-Provider Models

Models from Anthropic, OpenAI, and Gemini with alias resolution and capability tracking:

| Alias | Resolves To | Provider |
|-------|------------|----------|
| `opus` | `claude-opus-4-6` | Anthropic |
| `sonnet` | `claude-sonnet-4-6` | Anthropic |
| `haiku` | `claude-haiku-4-5-20251213` | Anthropic |
| `gpt-4o` | `gpt-4o` | OpenAI |
| `gemini` | `gemini-2.5-pro` | Gemini |

Each model carries `ModelCapabilities`: context window, max output, tools/reasoning/images flags, and per-provider pricing.

## CLI Flags

```
icode [OPTIONS] [COMMAND]

Options:
  --model MODEL                    Set the model (alias or full name)
  --dangerously-skip-permissions   Skip all permission checks
  --permission-mode MODE           read-only, workspace-write, or danger-full-access
  --allowedTools TOOLS             Restrict enabled tools
  --output-format FORMAT           text or json
  --version, -V                    Print version info

Commands:
  prompt <text>      One-shot prompt (non-interactive)
  login              Authenticate via OAuth
  logout             Clear stored credentials
  init               Initialize project config
  doctor             Check environment health
  self-update        Update to latest version
```

## Slash Commands (REPL)

| Command | Description |
|---------|-------------|
| `/help` | Show help |
| `/status` | Session status (model, tokens, cost) |
| `/cost` | Cost breakdown |
| `/compact` | Compact conversation history |
| `/clear` | Clear conversation |
| `/model [name]` | Show or switch model |
| `/permissions` | Show or switch permission mode |
| `/config [section]` | Config (env, hooks, model) |
| `/memory` | Project memory contents |
| `/diff` | Git diff |
| `/export [path]` | Export conversation |
| `/session [id]` | Resume previous session |
| `/version` | Version info |

## Features

| Feature | Status |
|---------|--------|
| Native ratatui TUI | ✅ |
| Multi-provider (Anthropic, OpenAI, Gemini) | ✅ |
| Model picker with persistence | ✅ |
| Model capabilities + pricing | ✅ |
| Streaming token display | ✅ |
| Tool system (bash, read, write, edit, grep, glob) | ✅ |
| Web tools (search, fetch) | ✅ |
| Sub-agent orchestration | ✅ |
| Todo tracking | ✅ |
| Notebook editing | ✅ |
| Project memory (CLAUDE.md) | ✅ |
| Config file hierarchy | ✅ |
| Permission system | ✅ |
| MCP server lifecycle | ✅ |
| Session persistence + resume | ✅ |
| Extended thinking | ✅ |
| Cost tracking | ✅ |
| Git integration | ✅ |
| Markdown terminal rendering | ✅ |
| Model aliases | ✅ |
| Slash commands | ✅ |
| Hooks (PreToolUse/PostToolUse) | 🔧 Config only |
| OAuth login/logout | ✅ |
| Mock parity harness | ✅ |

## Workspace Layout

```
rust/
├── Cargo.toml              # Workspace root
├── Cargo.lock
├── scripts/
│   ├── install.sh          # Global install script
│   └── run_mock_parity_harness.sh
└── crates/
    ├── api/                # HTTP client, SSE, auth, multi-provider routing
    ├── commands/           # Slash command registry
    ├── compat-harness/     # TS manifest extraction
    ├── icode-cli/          # Main binary: TUI, REPL, one-shot, model picker
    ├── mock-anthropic-service/ # Deterministic mock for testing
    ├── runtime/            # Agentic loop, config, permissions, MCP, prompts
    └── tools/              # Built-in tool implementations
```

### Crate Responsibilities

- **api** — HTTP client, SSE streaming, request/response types, multi-provider dispatch (Anthropic, OpenAI, Gemini), OAuth + API key auth
- **commands** — Slash command definitions and help text
- **compat-harness** — Extracts tool/prompt manifests from upstream source
- **icode-cli** — ratatui TUI, model picker, REPL (rustyline), one-shot prompt, streaming display, tool call rendering, CLI arg parsing
- **mock-anthropic-service** — Deterministic `/v1/messages` mock for parity tests
- **runtime** — `ConversationRuntime` agentic loop, `ConfigLoader` hierarchy, `Session` persistence, permission policy, MCP client, system prompt assembly, usage tracking
- **tools** — Tool execution: Bash, ReadFile, WriteFile, EditFile, GlobSearch, GrepSearch, WebSearch, WebFetch, Agent, TodoWrite, NotebookEdit, Skill, ToolSearch

## Mock Parity Harness

Deterministic end-to-end testing against a local mock API service:

```bash
cd rust/
./scripts/run_mock_parity_harness.sh
```

Scenarios: `streaming_text`, `read_file_roundtrip`, `grep_chunk_assembly`, `write_file_allowed`, `write_file_denied`, `multi_tool_turn_roundtrip`, `bash_stdout_roundtrip`, `bash_permission_prompt_approved`, `bash_permission_prompt_denied`, `plugin_tool_roundtrip`.

## Development

```bash
# Build
cargo build -p icode-cli

# Run TUI
cargo run -p icode-cli

# Run tests
cargo test -p icode-cli

# Lint
cargo clippy -p icode-cli -- -D warnings

# Format
cargo fmt -p icode-cli
```

## Stats

- **~22K lines** of Rust
- **7 crates** in workspace
- **Binary name:** `icode`
- **Config directory:** `~/.icode/`
- **Default model:** `claude-opus-4-6`
- **Default permissions:** `danger-full-access`

## License

See repository root.
