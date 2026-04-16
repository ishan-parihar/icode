# icode

```text
╔══════════════════════════════════════════════╗
║     ██╗ ██████╗ ██████╗ ██████╗ ███████╗     ║
║     ╚═╝██╔════╝██╔═══██╗██╔══██╗██╔════╝     ║
║     ██╗██║     ██║   ██║██║  ██║█████╗       ║
║     ██║██║     ██║   ██║██║  ██║██╔══╝       ║
║     ██║╚██████╗╚██████╔╝██████╔╝███████╗     ║
║     ╚═╝ ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝     ║
╚══════════════════════════════════════════════╝
```

Rust-native AI coding runtime and terminal assistant.

`icode` is a modular Rust workspace that provides:
- a CLI + TUI experience for coding workflows,
- an agent runtime with structured tool execution,
- provider integration (Anthropic/OpenAI/Gemini-compatible flows),
- MCP/plugin orchestration and session management.

## Why This Project

Most coding assistants are implemented in dynamic runtimes. `icode` explores the same problem space with:
- memory safety and strict typing,
- explicit permission boundaries,
- deterministic test surfaces,
- a workspace-oriented architecture that is easier to reason about and extend.

## Quick Start

```bash
cd rust

# Build and run
cargo build -p icode-cli
cargo run -p icode-cli
```

Install globally:

```bash
./rust/scripts/install.sh
icode --help
```

## Verification

```bash
cd rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Parity harness:

```bash
cd rust
./scripts/run_mock_parity_harness.sh
python3 scripts/run_mock_parity_diff.py
```

## Workspace Layout

```text
icode/
├── rust/
│   ├── crates/
│   │   ├── icode-cli/            # Main binary (CLI + TUI)
│   │   ├── runtime/              # Core orchestration/runtime logic
│   │   ├── tools/                # Built-in tool implementations
│   │   ├── api/                  # Provider clients + streaming
│   │   ├── orchestration/        # Agent and background orchestration
│   │   └── ...                   # Supporting crates (config, plugins, telemetry, etc.)
│   ├── scripts/
│   └── Cargo.toml
├── docs/
├── ROADMAP.md
└── README.md
```

## Core Capabilities

- **Runtime safety controls**: permission modes, workspace boundaries, command validation.
- **Tooling surface**: file operations, shell execution, search/glob, web tools, todos, notebooks.
- **Session workflows**: interactive TUI, one-shot prompt mode, persisted sessions.
- **Provider flexibility**: model aliases and multi-provider routing.
- **Extensibility**: MCP lifecycle integration and plugin hooks.

## Roadmap

High-level milestones are tracked in `ROADMAP.md`, focused on:
- reliable worker/session boot,
- event-native orchestration,
- branch/test awareness and recovery,
- autonomous policy-driven execution,
- plugin/MCP lifecycle maturity.

## Portfolio Note

This repository is under active development. If you are evaluating it, check:
- architecture and crate boundaries,
- test strategy and harnesses,
- security/permission design in the runtime layer,
- CI workflows in `.github/workflows`.

## License

MIT