```
__   ______                   __
|  \ /      \                 |  \
 \$$|  $$$$$$\  ______    ____| $$  ______
| $$| $$   \$$ /      \  /      $$ /      \
| $$| $$      |  $$$$$$\|  $$$$$$$|  $$$$$$\
| $$| $$   __ | $$  | $$| $$  | $$| $$    $$
| $$| $$__/  \| $$__/ $$| $$__| $$| $$$$$$$$
| $$ \$$    $$ \$$    $$ \$$    $$ \$$     \
 \$$  \$$$$$$   \$$$$$$   \$$$$$$$  \$$$$$$$
```

**Rust-native AI coding harness** — a memory-safe, high-performance reimplementation of an agent coding runtime with tool orchestration, mock LLM testing, and plugin/MCP lifecycle management.

[![Rust](https://img.shields.io/badge/Rust-2021-ed8b00.svg)](https://www.rust-lang.org)
[![Crates](https://img.shields.io/badge/crates-9-blue.svg)](./rust/Cargo.toml)
[![LOC](https://img.shields.io/badge/Rust_LOC-48,599-brightgreen.svg)](./PARITY.md)
[![Tests](https://img.shields.io/badge/Test_LOC-2,568-yellow.svg)](./PARITY.md)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

## Origin

icode is derived from the leaked Claude Code codebase, initially forked from [claw-code](https://github.com/ultraworkers/claw-code) and subsequently reworked from the [opencode](https://github.com/anomalyco/opencode) source. What started as a TypeScript harness has been re-engineered as a **memory-safe, zero-copy, fully Rust-native** AI coding runtime — preserving the original intent while delivering strict compile-time guarantees, deterministic mock testing, and a production-grade TUI.

## What It Is

icode is a Rust port of an AI coding agent harness — the kind of system that sits between an LLM and your codebase, managing tool calls, permissions, sessions, and execution state. While the original projects in this space are TypeScript or Python, icode targets **memory safety, zero-copy performance, and strict compile-time guarantees** through Rust.

The codebase implements a **9-crate workspace** with 48,599 lines of Rust, covering bash execution, file operations, task management, team/cron registries, MCP server lifecycle, LSP client dispatch, and permission enforcement — all with a mock Anthropic API for deterministic integration testing.

## Architecture

```
icode/rust/
├── crates/
│   ├── icode-cli/               # Main CLI entrypoint + TUI
│   ├── runtime/                 # Core runtime: bash, file_ops, sandbox, validation
│   │   ├── bash.rs              # Bash execution with timeout/background/sandbox
│   │   ├── bash_validation.rs   # 1004 LOC: readOnly, destructive, sed, path validation
│   │   ├── file_ops.rs          # 744 LOC: read/write/edit with binary detection, size limits
│   │   ├── sandbox.rs           # 385 LOC: unshare capability probing, container detection
│   │   ├── task_registry.rs     # 335 LOC: in-memory task lifecycle
│   │   ├── team_cron_registry.rs# 363 LOC: team + cron registries
│   │   ├── mcp_tool_bridge.rs   # 406 LOC: MCP server lifecycle bridge
│   │   ├── lsp_client.rs        # 438 LOC: LSP diagnostics, hover, definition, completion
│   │   ├── permission_enforcer.rs# 340 LOC: tool gating, workspace boundaries, bash enforcement
│   │   └── permissions.rs       # Permission policy definitions
│   ├── tools/                   # Tool surface: 40 exposed tool specs
│   ├── commands/                # Slash command handling (/plan, /plugin, etc.)
│   ├── plugins/                 # Plugin install/enable/disable/uninstall
│   ├── mock-anthropic-service/  # Deterministic mock Anthropic API
│   ├── config/                  # Config loading with user > project > local precedence
│   └── ...                      # Additional utility crates
└── mock_parity_scenarios.json   # Canonical test scenario definitions
```

## Feature Matrix

### Tool Surface — 40 Exposed Tools

| Category | Tools | Status |
|---|---|---|
| **Core Execution** | `bash`, `read_file`, `write_file`, `edit_file`, `glob_search`, `grep_search` | ✅ Implemented |
| **Task Management** | `TaskCreate`, `TaskGet`, `TaskList`, `TaskStop`, `TaskUpdate`, `TaskOutput` | ✅ Registry-backed |
| **Team & Cron** | `TeamCreate`, `TeamDelete`, `CronCreate`, `CronDelete`, `CronList` | ✅ Registry-backed |
| **MCP Lifecycle** | `ListMcpResources`, `ReadMcpResource`, `McpAuth`, `MCP` | ✅ Registry-backed |
| **LSP Client** | `symbols`, `references`, `diagnostics`, `definition`, `hover` | ✅ Registry-backed |
| **Product Tools** | `WebFetch`, `WebSearch`, `TodoWrite`, `Skill`, `Agent`, `ToolSearch`, `NotebookEdit`, `Sleep`, `SendUserMessage`, `Config`, `EnterPlanMode`, `ExitPlanMode`, `StructuredOutput`, `REPL`, `PowerShell` | ✅ Implemented |

### Security & Permissions

- **Permission modes** — read-only vs workspace-write enforced across all tool paths
- **Workspace boundary checks** — symlink escape prevention, canonical path validation
- **Bash validation** — 6 validation submodules: readOnly, destructive commands, sed, path, mode, command semantics
- **File safety** — binary file detection, read/write size limits (`MAX_READ_SIZE`, `MAX_WRITE_SIZE`), NUL-byte detection
- **Sandbox detection** — probes `unshare` capability and container signals instead of binary presence

### Mock Parity Harness

A deterministic testing framework for validating harness behavior against an Anthropic-compatible mock API:

- **10 scripted scenarios**: streaming text, file roundtrip, grep chunk assembly, write allow/deny, multi-tool turns, bash stdout, permission prompts (approve/deny), plugin execution
- **19 captured `/v1/messages` requests** validated for behavioral correctness
- **Clean-environment harness** — each scenario runs in an isolated state, no cross-test pollution
- **Behavioral diff runner** — automated parity checking between expected and actual behavior

### 9 Merged Feature Lanes

| # | Lane | LOC | Description |
|---|---|---|---|
| 1 | Bash validation | +1,004 | readOnly, destructive command, sed, path, mode, semantics validation |
| 2 | CI sandbox fix | 385 | Probe `unshare` capability instead of binary existence |
| 3 | File-tool edge cases | +744 | Binary detection, size limits, workspace boundaries, symlink escape |
| 4 | TaskRegistry | +335 | In-memory task lifecycle (create/get/list/stop/update/output) |
| 5 | Task wiring | +79 | Wire TaskRegistry into all 6 task tool dispatch paths |
| 6 | Team + Cron | +441 | TeamRegistry + CronRegistry with tool dispatch wiring |
| 7 | MCP lifecycle | +491 | McpToolRegistry: server connection, resources, auth, tool dispatch |
| 8 | LSP client | +461 | LspRegistry: diagnostics, hover, definition, references, completion, symbols |
| 9 | Permission enforcement | +357 | PermissionEnforcer: tool gating, file write boundaries, bash read-only |

## Tech Stack

| Layer | Technology |
|---|---|
| **Language** | Rust 2021 edition |
| **Workspace** | 9 crates, resolver 2 |
| **Serialization** | serde + serde_json |
| **Linting** | clippy pedantic (warn), unsafe_code (forbid) |
| **Testing** | `cargo test --workspace` with mock service |
| **CI** | GitHub Actions (fmt check, clippy, test) |

## Quick Start

```bash
# Enter Rust workspace
cd rust/

# Format all crates
cargo fmt --all

# Run clippy (strict: pedantic warnings, no unsafe code)
cargo clippy --workspace --all-targets -- -D warnings

# Run full test suite
cargo test --workspace

# Run mock parity harness specifically
cargo test -p icode-cli mock_parity_harness
```

### Running the Parity Diff

```bash
# Compare current behavior against canonical scenario expectations
python3 rust/scripts/run_mock_parity_diff.py
```

## Project Structure

```
icode/
├── rust/                    # Rust workspace (9 crates)
│   ├── crates/              # Individual crate implementations
│   ├── scripts/             # Parity diff and validation scripts
│   └── mock_parity_scenarios.json
├── src/                     # Python porting workspace (legacy)
├── tests/                   # Python verification tests (legacy)
├── PARITY.md                # Current parity status and lane details
├── ROADMAP.md               # Development roadmap — 5 phases
└── README.md                # This file
```

## Roadmap

The project follows a 5-phase development plan toward a fully "clawable" (machine-orchestratable) coding harness:

| Phase | Focus | Key Deliverables |
|---|---|---|
| **1** | Reliable Worker Boot | Ready handshake, trust prompt resolver, session control API |
| **2** | Event-Native Integration | Canonical lane event schema, failure taxonomy, summary compression |
| **3** | Branch/Test Awareness | Stale-branch detection, recovery recipes, green-level contracts |
| **4** | Claws-First Execution | Typed task packets, policy engine, machine-readable lane board |
| **5** | Plugin/MCP Maturity | Lifecycle contracts, end-to-end MCP parity, degraded-mode reporting |

See [ROADMAP.md](./ROADMAP.md) for the full plan with acceptance criteria.

## Current Status

- **514 commits** across active development
- **48,599 Rust LOC** across 9 crates
- **2,568 test LOC** — no `#[ignore]` tests
- **10 mock parity scenarios** — all passing
- **40 tool specs** exposed on the tool surface
- **9 feature lanes** merged onto main

## License

MIT
