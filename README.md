# icode 🛠️

**The Hardened Runtime for AI Coding Agents.**

[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![TUI](https://img.shields.io/badge/UI-Terminal-blue.svg)](https://github.com/ratatui-org/ratatui)
[![MCP](https://img.shields.io/badge/Protocol-MCP-purple.svg)](https://modelcontextprotocol.io/)

`icode` is not just a coding assistant; it is a **high-performance, memory-safe execution environment** designed to host AI agents. Written entirely in Rust, it provides the critical infrastructure needed to move AI agents from "chat-bot wrappers" to "production-grade system operators."

While most agents live in dynamic runtimes (Node/Python), `icode` explores the frontier of **Strict Agent Orchestration**: combining zero-cost abstractions, explicit permission boundaries, and a deterministic tool-execution loop.

---

## 🚩 The Problem: The "Fragile Agent" Gap

Current AI coding assistants often suffer from three systemic failures:
1. **Non-Deterministic Tooling**: Tool execution is often a "best effort" string-matching process, leading to fragile edits and silent failures.
2. **Security Blindspots**: Giving an LLM `bash` access is a security nightmare. Most agents lack a granular, policy-driven permission layer that can actually be audited.
3. **State Decay**: As sessions grow, the "context window" becomes a mess. Managing session snapshots, reverts, and compaction in a way that doesn't confuse the model is a massive engineering challenge.

## 💡 The Solution: A Hardened AI Runtime

`icode` solves these problems by implementing a **layered runtime architecture**:

### 1. The Guardrail Layer (Permission Engine)
Instead of a simple "yes/no" for tools, `icode` implements a **Policy-Driven Permission System**. It validates every tool call against workspace boundaries and specific safety rules *before* the command ever hits the shell.

### 2. The Orchestration Layer (Agent Delegation)
`icode` treats agents as modular entities. It supports **Hierarchical Delegation**: a "Master" agent can spawn specialized sub-agents (Explorer, Librarian, Fixer) for bounded tasks, each with its own isolated context and tool-set.

### 3. The State Layer (Session Management)
Using a custom SQLite-backed store, `icode` manages sessions as first-class citizens. It supports **Atomic Session Snapshots**, allowing a user to "branch" a coding session or revert the entire workspace state if an agent goes off the rails.

---

## ✨ Engineering Highlights

### 🏗 System Architecture
- **Rust-Native Core**: Built with a modular crate workspace for maximum compile-time safety and runtime performance.
- **TUI Excellence**: A high-fidelity Terminal User Interface (TUI) featuring a command palette, real-time debug panels, and a structured message stream.
- **MCP Integration**: Native support for the **Model Context Protocol (MCP)**, allowing `icode` to plug into any MCP-compliant server for external data and tools.
- **Provider Agnostic**: A unified API layer that abstracts away the differences between Anthropic, OpenAI, Gemini, and local LLM providers.

### 🛠 Technical Specifications
- **Memory Safety**: Zero-overhead abstractions for handling large file buffers and streaming LLM responses.
- **Hook-Based Extensibility**: A `hooks-engine` that allows injecting custom logic (e.g., `todo_continuation_enforcer`) into the agent's thought-action loop.
- **Parity Harness**: A dedicated testing suite that ensures `icode` tool outputs maintain strict parity with reference implementations.

---

## 🌌 Potentialities & Future Scope

`icode` is a prototype for the next generation of **AI Operating Systems**:

- **Autonomous System Operator**: Moving from "editing files" to "managing infrastructure." An agent that can monitor logs, detect crashes, and apply patches autonomously.
- **Local-First Intelligence**: Fully integrating local models (Llama/Mistral) into the hardened runtime, removing the dependency on external APIs.
- **Standardized Agent Schemas**: Developing a protocol for how agents "handoff" work to one another across different runtimes.

---

## 🚀 Quick Start

### Installation
```bash
cd rust
cargo build -p icode-cli
# Or use the install script
./rust/scripts/install.sh
```

### Basic Usage
```bash
# Start the interactive TUI
icode

# Run a one-shot command
icode "Refactor the auth logic in src/auth.rs"
```

## 🛠 Tech Stack
- **Language**: Rust (Edition 2021)
- **TUI**: Ratatui / Crossterm
- **Async**: Tokio
- **Persistence**: SQLite
- **Protocol**: MCP (Model Context Protocol)

---
Developed by [Ishan Parihar](https://github.com/ishan-parihar) as an exploration into the intersection of systems programming and agentic AI.
