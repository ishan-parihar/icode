# Context & Memory Upgrade Plan

## Making icode a Perfect Coding Terminal

> **Status**: Proposed · **Scope**: Phase 6 of ROADMAP.md · **Target**: Semantic memory, pluggable context engine, cross-session recall

---

## 0. Why This Matters

The existing ROADMAP.md (Phases 1-5) makes icode **clawable** — machine-orchestratable, event-native, recoverable. But it does not make icode **intelligent** across sessions.

Today, every icode session starts cold. The model has no memory of:
- Architectural decisions made last week
- Debugging sessions that revealed root causes
- User preferences (test style, naming conventions, module boundaries)
- Codebase facts that were discovered through exploration
- Past failures and their resolutions

OpenClaw solves this with a multi-layered memory system: vector-backed semantic search, markdown-based long-term memory, hybrid retrieval (vector + BM25), temporal decay, and automatic memory flush before compaction destroys context.

**This plan closes that gap.** It transforms icode from a session-isolated coding tool into one that *remembers* — making every session smarter than the last.

---

## 1. Architecture Vision

### 1.1 Target State

```
┌─────────────────────────────────────────────────────────────┐
│                      icode Runtime                          │
│                                                             │
│  ┌──────────────┐   ┌───────────────┐   ┌────────────────┐ │
│  │  Context     │   │   Memory      │   │  Compaction    │ │
│  │  Engine      │   │   Subsystem   │   │  Engine        │ │
│  │  (pluggable) │   │   (vector +   │   │  (adaptive)    │ │
│  │              │   │    keyword)   │   │                │ │
│  └──────┬───────┘   └───────┬───────┘   └───────┬────────┘ │
│         │                   │                   │           │
│         ▼                   ▼                   ▼           │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              Session Manager                         │  │
│  │         (JSONL + SQLite + Vector Index)              │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌──────────────┐   ┌───────────────┐   ┌────────────────┐ │
│  │  Tool:       │   │  Tool:        │   │  Tool:         │ │
│  │  memory_     │   │  memory_      │   │  /context      │ │
│  │  recall      │   │  store        │   │  report        │ │
│  └──────────────┘   └───────────────┘   └────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 Design Principles

1. **Local-first, zero external dependencies** — no required API keys for core memory. Embeddings use local models by default, cloud providers are optional accelerants.
2. **Rust-native** — no FFI to Python/Node. All vector search, tokenization, and indexing runs in-process.
3. **Session-transparent** — memory operations happen automatically. The agent can also use memory tools explicitly when needed.
4. **Backward-compatible** — existing sessions, JSONL files, and SQLite schemas continue working unchanged.
5. **Pluggable** — context engine is a trait; compaction strategy is swappable; embedding provider is configurable.

---

## 2. Gap Analysis Summary

| Area | OpenClaw Has | icode Has | Priority |
|------|-------------|-----------|----------|
| Vector memory store (LanceDB/sqlite-vec) | ✅ | ❌ | **P0** |
| Embedding pipeline (multi-provider) | ✅ | ❌ | **P0** |
| Semantic search over memories | ✅ | ❌ | **P0** |
| MEMORY.md / daily notes | ✅ | ❌ | **P1** |
| Memory flush before compaction | ✅ | ❌ | **P1** |
| Cross-session memory retrieval | ✅ | ❌ | **P0** |
| Pluggable context engine | ✅ | ❌ | **P1** |
| Adaptive compaction (tool-call pairs, parallel) | ✅ | ⚠️ Basic | **P2** |
| Real token estimation (not chars/4) | ✅ | ❌ | **P2** |
| Context pruning (trim old tool results) | ✅ | ❌ | **P2** |
| Hybrid search (vector + BM25) | ✅ | ❌ | **P2** |
| Temporal decay + MMR re-ranking | ✅ | ❌ | **P3** |
| Dreaming (background consolidation) | ✅ | ❌ | **P3** |

---

## 3. Implementation Phases

### Phase 6.0: Memory Subsystem Foundation

**Goal**: Vector-backed memory store with semantic search, embedding pipeline, and memory tools.

#### 6.0.1 Crate: `memory`

Create a new workspace crate `rust/crates/memory/` with the following structure:

```
rust/crates/memory/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API exports
│   ├── config.rs           # MemoryConfig (backend, embedding, search params)
│   ├── store/
│   │   ├── mod.rs          # MemoryStore trait
│   │   ├── sqlite.rs       # SQLite-backed storage (metadata + FTS5)
│   │   └── vector.rs       # Vector index (usearch or lancedb)
│   ├── embedding/
│   │   ├── mod.rs          # EmbeddingProvider trait
│   │   ├── local.rs        # Local embedding (ort/onnxruntime + BGE model)
│   │   ├── openai.rs       # OpenAI text-embedding-3-small
│   │   └── remote.rs       # Generic HTTP embedding endpoint
│   ├── search/
│   │   ├── mod.rs          # SearchResult, SearchQuery
│   │   ├── vector.rs       # Cosine similarity search
│   │   ├── keyword.rs      # FTS5/BM25 keyword search
│   │   └── hybrid.rs       # Merge vector + keyword with weights
│   ├── memory.rs           # MemoryEntry, MemoryManager
│   ├── chunker.rs          # Text chunking (token-aware, with overlap)
│   ├── indexer.rs          # File watcher + batch indexing pipeline
│   └── tools.rs            # memory_recall, memory_store tool specs
```

**Key dependencies** (to be verified via context7):
- `rusqlite` + `sqlite-vec` (or `usearch` for HNSW vector search)
- `ort` (ONNX Runtime) for local embedding inference
- `tantivy` for full-text search (alternative to SQLite FTS5)
- `reqwest` for remote embedding providers
- `tokio` for async indexing

#### 6.0.2 MemoryStore Trait

```rust
pub trait MemoryStore: Send + Sync {
    /// Store a memory entry with its embedding vector
    async fn store(&self, entry: MemoryEntry) -> Result<()>;

    /// Search memories by semantic relevance
    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchResult>>;

    /// Get a specific memory by ID
    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>>;

    /// Delete a memory by ID
    async fn delete(&self, id: &str) -> Result<()>;

    /// Rebuild the index (after bulk changes)
    async fn rebuild_index(&self) -> Result<()>;

    /// Get index statistics
    fn stats(&self) -> IndexStats;
}
```

#### 6.0.3 EmbeddingProvider Trait

```rust
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for a batch of texts
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Dimensionality of the embedding model
    fn dimensions(&self) -> usize;

    /// Provider identifier (e.g., "openai", "local-bge", "remote")
    fn id(&self) -> &str;
}
```

#### 6.0.4 MemoryEntry

```rust
pub struct MemoryEntry {
    pub id: String,              // UUID
    pub source: MemorySource,    // MemoryFile | SessionTranscript | Manual
    pub content: String,         // The memory text
    pub chunk_index: usize,      // Which chunk of the source
    pub total_chunks: usize,     // Total chunks for this source
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub recall_count: u32,       // How many times this was recalled
    pub last_recalled_at: Option<DateTime<Utc>>,
    pub metadata: BTreeMap<String, String>, // source_file, session_id, etc.
}

pub enum MemorySource {
    MemoryFile { path: PathBuf },     // MEMORY.md or daily note
    SessionTranscript { session_id: String },
    Manual { category: String },      // user-stored fact
}
```

#### 6.0.5 Memory Tools

Two new tools exposed to the agent:

```
memory_recall(query: String, limit: Option<usize>) -> Vec<MemoryResult>
  - Semantic search over stored memories
  - Returns content + relevance score + source metadata
  - Injected into context before model turn

memory_store(content: String, category: Option<String>) -> MemoryId
  - Explicitly store a fact/decision
  - Called by the agent when it learns something important
```

#### 6.0.6 Acceptance Criteria

- [ ] `memory` crate compiles and passes unit tests
- [ ] `MemoryStore` trait implemented with SQLite + vector backend
- [ ] At least one `EmbeddingProvider` works (local BGE or OpenAI)
- [ ] `memory_recall` tool returns semantically relevant results
- [ ] `memory_store` tool persists entries with embeddings
- [ ] Memory survives process restart (persistent storage verified)
- [ ] Index rebuild works after bulk changes
- [ ] No new unsafe code (workspace `forbid` maintained)

---

### Phase 6.1: Memory Files + Auto-Capture

**Goal**: Markdown-based long-term memory (MEMORY.md, daily notes) with automatic capture.

#### 6.1.1 Memory File Structure

```
.icode/memory/
├── MEMORY.md              # Long-term facts, preferences, decisions
└── daily/
    ├── 2026-04-06.md      # Today's session observations
    └── 2026-04-05.md      # Yesterday's session observations
```

**MEMORY.md format:**
```markdown
# Memory

## Preferences
- User prefers TypeScript over JavaScript for new modules
- Test naming convention: `{function_name}_handles_{scenario}`

## Architectural Decisions
- [2026-04-01] Chose SQLite over JSONL for session metadata due to query performance
- [2026-04-03] MCP servers run in degraded mode on partial startup, not fail-fast

## Key Facts
- The `runtime` crate is the central hub; all other crates depend on it directly or transitively
- Permission modes: ReadOnly, WorkspaceWrite, DangerFullAccess
```

**Daily note format:**
```markdown
# 2026-04-06

## Session: session-1743940800000-0
- Discovered: `chars/4+1` token estimation is ~15% inaccurate for Rust code
- Decision: Will replace with tiktoken-based estimation in Phase 6.3
- Debugged: MCP handshake timeout was caused by missing stdin flush
```

#### 6.1.2 Auto-Capture Hook

After each agent turn, a post-turn hook analyzes the conversation for:
- **Decisions** ("I'll use X because Y")
- **Discoveries** ("The issue is that Z...")
- **Preferences** ("I prefer...")
- **Key facts** (module relationships, API contracts, error patterns)
- **Debugging insights** (root causes, workarounds)

Captured items are appended to the daily note. The agent can also be prompted to write to MEMORY.md for long-term facts.

#### 6.1.3 Auto-Load

At session start:
1. Load `MEMORY.md` (full content, budget: 4,000 chars)
2. Load today's daily note (if exists)
3. Load yesterday's daily note (if exists)
4. Run `memory_recall` against recent session context for additional relevant memories

#### 6.1.4 Acceptance Criteria

- [ ] MEMORY.md created on first session with content
- [ ] Daily notes auto-created per session date
- [ ] Auto-capture hook runs after each turn without blocking
- [ ] Memory files loaded at session start within 100ms
- [ ] Agent can read/write memory files via tool interface
- [ ] Memory file budget enforced (no unbounded growth in context)

---

### Phase 6.2: Memory Flush Before Compaction

**Goal**: Prevent context loss during compaction by saving critical facts before summarization.

#### 6.2.1 The Problem

Current icode compaction replaces older messages with a summary. Any facts, decisions, or discoveries that weren't explicitly saved are lost forever.

#### 6.2.2 The Solution

Before running `compact_session()`:

1. **Memory flush turn** — Run a silent model turn with instructions:
   > "Review the conversation history. Identify any important facts, decisions, discoveries, or preferences that are not yet saved to memory. Store each one using memory_store."

2. **Index updated memories** — The indexer picks up the new memory entries and generates embeddings.

3. **Proceed with compaction** — Now safe to summarize, because critical context is persisted.

#### 6.2.3 Implementation

```rust
// In conversation.rs, within maybe_auto_compact():
async fn maybe_auto_compact(&mut self) -> Result<()> {
    if !should_compact(&self.session, &self.config) {
        return Ok(());
    }

    // NEW: Memory flush before compaction
    self.memory_flush().await?;

    // Existing compaction
    let result = compact_session(&self.session, &self.config.compaction).await?;
    self.session.record_compaction(result.summary, result.removed_count);
    Ok(())
}

async fn memory_flush(&mut self) -> Result<()> {
    if self.memory.is_none() {
        return Ok(()); // No memory subsystem configured
    }

    let memory = self.memory.as_ref().unwrap();
    let flush_prompt = "Review recent conversation. For each important fact, decision, \
                        or discovery not yet saved to memory, call memory_store with the content.";

    // Run a single-turn flush (no tool execution except memory_store)
    memory.flush_from_conversation(&self.session.messages, flush_prompt).await
}
```

#### 6.2.4 Acceptance Criteria

- [ ] Memory flush runs automatically before compaction
- [ ] Flush completes within 5 seconds (with timeout)
- [ ] If flush fails, compaction still proceeds (non-blocking)
- [ ] Flushed memories are queryable via memory_recall immediately
- [ ] No duplicate memories from repeated flushes (dedup by content hash)

---

### Phase 6.3: Pluggable Context Engine

**Goal**: Replace hardcoded context assembly with a trait-based, swappable context engine.

#### 6.3.1 ContextEngine Trait

```rust
pub trait ContextEngine: Send + Sync {
    /// Called when a new message is added to the session.
    /// Engine can store/index the message in its own data store.
    async fn ingest(&self, session_id: &str, message: &ConversationMessage) -> Result<()>;

    /// Called before each model run.
    /// Returns messages that fit within the token budget.
    async fn assemble(&self, ctx: AssembleContext) -> Result<AssembleResult>;

    /// Called when context window is full or user runs /compact.
    /// Summarizes older history to free space.
    async fn compact(&self, session_id: &str, force: bool) -> Result<CompactResult>;

    /// Called after a model turn completes.
    /// Engine can persist state, trigger background compaction, etc.
    async fn after_turn(&self, session_id: &str, turn: TurnResult) -> Result<()>;

    /// Engine metadata
    fn info(&self) -> EngineInfo;
}

pub struct AssembleContext {
    pub session_id: String,
    pub messages: Vec<ConversationMessage>,
    pub token_budget: usize,
    pub system_prompt: String,
    pub memory_results: Option<Vec<SearchResult>>, // From pre-turn memory search
}

pub struct AssembleResult {
    pub messages: Vec<ConversationMessage>,
    pub estimated_tokens: usize,
    pub system_prompt_addition: Option<String>, // Dynamic context injection
    pub compaction_needed: bool,
}

pub struct EngineInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub owns_compaction: bool, // If true, disables built-in auto-compaction
}
```

#### 6.3.2 Built-in Engines

1. **`LegacyEngine`** — Current behavior: pass-through assembly, rule-based compaction. Default for backward compatibility.

2. **`MemoryAwareEngine`** — New engine that:
   - Runs `memory_recall` before each turn
   - Injects relevant memories into `system_prompt_addition`
   - Uses real token estimation for budget decisions
   - Triggers memory flush before compaction

#### 6.3.3 Engine Registry

```rust
pub struct ContextEngineRegistry {
    engines: HashMap<String, Box<dyn ContextEngine>>,
    active_engine: String, // Engine ID of the active engine
}

impl ContextEngineRegistry {
    pub fn register(&mut self, engine: Box<dyn ContextEngine>);
    pub fn active(&self) -> &dyn ContextEngine;
    pub fn set_active(&mut self, engine_id: &str) -> Result<()>;
}
```

#### 6.3.4 Configuration

```toml
# .icode/config.toml (new section)
[context]
engine = "memory-aware"  # "legacy" | "memory-aware" | custom plugin id

[context.memory]
enabled = true
auto_recall = true           # Search memory before each turn
auto_capture = true          # Capture facts after each turn
recall_limit = 6             # Max memories injected per turn
min_relevance_score = 0.35   # Minimum similarity score

[context.embedding]
provider = "local"           # "local" | "openai" | "remote"
model = "BGE-small-en-v1.5"  # Local model name
# If provider = "openai":
# api_key = "sk-..."
# model = "text-embedding-3-small"
```

#### 6.3.5 Acceptance Criteria

- [ ] `ContextEngine` trait defined and documented
- [ ] `LegacyEngine` implements trait with identical behavior to current code
- [ ] `MemoryAwareEngine` implements trait with memory integration
- [ ] Engine selectable via config file
- [ ] Switching engines does not lose session data
- [ ] `system_prompt_addition` from engine is correctly prepended to system prompt
- [ ] Engine errors logged and surfaced (no silent failures)

---

### Phase 6.4: Better Compaction

**Goal**: Replace crude compaction with adaptive, token-aware, tool-call-preserving summarization.

#### 6.4.1 Real Token Estimation

Replace `chars/4 + 1` with a proper tokenizer.

**Option A**: `tiktoken-rs` — Rust bindings for OpenAI's tiktoken. Supports cl100k_base (GPT-4), p50k_base, r50k_base.

**Option B**: Provider-specific estimation tables — Different models have different tokenization. Build a lookup:

```rust
pub fn estimate_tokens(messages: &[ConversationMessage], model: &str) -> usize {
    let tokenizer = get_tokenizer_for_model(model);
    messages.iter().map(|m| tokenizer.count(m)).sum()
}
```

#### 6.4.2 Adaptive Chunk Compaction

Current icode compaction: "Keep last N messages, summarize everything else into one `<summary>` block."

OpenClaw's approach (target):
1. **Split messages by token share** — Divide history into chunks that fit a token budget
2. **Preserve tool-call pairs** — Never split a `tool_use` from its `tool_result`
3. **Handle oversized messages** — If a single message > 50% of context, mark as "too large to summarize" and keep it
4. **Parallel split summarization** — Summarize multiple chunks in parallel, then merge summaries
5. **Progressive fallback** — If full summary fails, try partial; if partial fails, keep most recent and drop oldest

#### 6.4.3 Merge Compaction Summaries

Current icode already has `merge_compact_summaries()` which preserves previous summaries under "Previously compacted context:". Enhance this to:
- Track which topics appear in old vs new summaries
- Deduplicate overlapping content
- Maintain a timeline across compaction cycles

#### 6.4.4 Acceptance Criteria

- [ ] Token estimation accuracy within ±5% of actual provider token count
- [ ] Tool-call pairs never broken by compaction
- [ ] Oversized single messages handled gracefully (not lost, not summarized poorly)
- [ ] Compaction completes within 10 seconds for 100-message sessions
- [ ] Merged summaries remain coherent after 3+ compaction cycles
- [ ] No regression: existing compaction tests still pass

---

### Phase 6.5: Context Pruning + Advanced Search

**Goal**: Optimize context assembly and add hybrid search.

#### 6.5.1 Context Pruning

During context assembly, trim old tool results that exceed a size threshold:

```rust
fn prune_tool_results(messages: &[ConversationMessage], max_tool_result_chars: usize) -> Vec<ConversationMessage> {
    messages.iter().map(|m| {
        match m {
            Message::ToolResult { output, .. } if output.len() > max_tool_result_chars => {
                // Replace with truncated version + note
                Message::ToolResult {
                    output: format!("{}...[truncated, {} chars omitted]",
                        &output[..max_tool_result_chars],
                        output.len() - max_tool_result_chars),
                    ..m.clone()
                }
            }
            _ => m.clone(),
        }
    }).collect()
}
```

#### 6.5.2 Hybrid Search

Combine vector similarity with keyword matching (BM25):

```rust
pub fn hybrid_search(
    vector_results: Vec<SearchResult>,
    keyword_results: Vec<SearchResult>,
    weights: HybridWeights,  // default: 0.7 vector, 0.3 keyword
) -> Vec<SearchResult> {
    // Merge by ID, combine scores with weights, re-rank
    // Apply MMR for diversity if enabled
    // Apply temporal decay if enabled
}
```

#### 6.5.3 MMR Re-Ranking

Maximal Marginal Relevance — avoids returning 6 nearly-identical memories:

```rust
fn mmr_rerank(
    candidates: &[SearchResult],
    query_embedding: &[f32],
    lambda: f32,     // diversity weight (0.5 = balanced)
    k: usize,        // number of results
) -> Vec<SearchResult> {
    // Iteratively select: maximize (lambda * relevance - (1-lambda) * max_similarity_to_selected))
}
```

#### 6.5.4 Temporal Decay

Older memories score lower unless frequently recalled:

```rust
fn apply_temporal_decay(score: f32, created_at: DateTime<Utc>, recall_count: u32) -> f32 {
    let age_days = (Utc::now() - created_at).num_days() as f32;
    let half_life = 30.0; // days
    let decay = 0.5_f32.powf(age_days / half_life);
    let recall_boost = 1.0 + (recall_count as f32 * 0.1).min(0.5);
    score * decay * recall_boost
}
```

#### 6.5.5 Acceptance Criteria

- [ ] Context pruning reduces token count by 20%+ for sessions with large tool outputs
- [ ] Hybrid search outperforms vector-only on code symbol queries (exact match matters)
- [ ] MMR re-ranking returns diverse results (no 2 results with >80% content overlap)
- [ ] Temporal decay correctly down-weights stale memories (>30 days old, never recalled)
- [ ] Frequently recalled memories maintain high scores despite age

---

### Phase 6.6: Dreaming (Background Consolidation)

**Goal**: Automatic promotion of frequently-recalled short-term memories to long-term storage.

#### 6.6.1 How It Works

1. **Trigger**: Background task runs every 24 hours (or on session close)
2. **Scan**: Read all daily notes from the past 7 days
3. **Score**: For each memory item, calculate:
   - Recall frequency (how many different queries recalled it)
   - Recency (how recent)
   - Cross-query diversity (was it recalled for different topics?)
4. **Promote**: Items scoring above threshold get promoted from daily note to MEMORY.md
5. **Prune**: Daily notes older than 30 days are archived (not deleted)

#### 6.6.2 Implementation

```rust
pub async fn dreaming_cycle(memory: &MemoryManager, config: &DreamingConfig) -> Result<DreamingReport> {
    let daily_notes = memory.load_daily_notes(last_n_days: 7).await?;
    let recalls = memory.get_recall_log(last_n_days: 7).await?;

    let mut promotions = Vec::new();
    for item in daily_notes.items() {
        let score = dreaming_score(&item, &recalls, config);
        if score >= config.promotion_threshold {
            promotions.push(item);
        }
    }

    for item in &promotions {
        memory.promote_to_long_term(item).await?;
    }

    Ok(DreamingReport {
        items_scored: daily_notes.items().count(),
        items_promoted: promotions.len(),
        items_archived: memory.archive_old_daily_notes(30).await?,
    })
}
```

#### 6.6.3 Acceptance Criteria

- [ ] Dreaming runs automatically on configurable schedule
- [ ] Promoted items appear in MEMORY.md with source attribution
- [ ] Dreaming completes within 30 seconds for 7 days of daily notes
- [ ] Promoted items are queryable via memory_recall immediately
- [ ] Archive of old daily notes is accessible (not deleted)

---

## 4. Dependency Analysis

### New Crate Dependencies

| Crate | Purpose | Alternative Considered | Why Chosen |
|-------|---------|----------------------|------------|
| `usearch` or `lancedb` | HNSW vector search | `faiss`, `hnswlib` | `usearch` is pure Rust, `lancedb` has richer ecosystem |
| `ort` | ONNX Runtime for local embeddings | `candle` | `ort` has broader model support |
| `tiktoken-rs` | Accurate token counting | `cl100k` manual impl | Well-maintained, supports multiple encodings |
| `tantivy` | Full-text search (BM25) | SQLite FTS5 | Tantivy is more flexible for hybrid search |
| `tokio-cron-scheduler` | Scheduled tasks (dreaming) | `schedule` | Better tokio integration |
| `notify` | File watching (memory files) | `watchexec` | Standard Rust file watcher |

### Impact on Existing Crates

| Crate | Changes Needed |
|-------|---------------|
| `runtime` | Import `memory` crate; integrate `ContextEngine` trait; add memory flush hook; replace token estimation |
| `tools` | Add `memory_recall` and `memory_store` tool specs |
| `commands` | Add `/memory` command (status, search, index, clear) |
| `api` | Add embedding provider HTTP client |
| `icode-cli` | Add TUI memory panel, `/memory` command handling |
| `plugins` | Support context engine plugins |

---

## 5. Migration Strategy

### Zero-Breaking-Changes Approach

1. **Default to legacy behavior** — `context.engine = "legacy"` is the default. Memory subsystem is opt-in.
2. **Feature-gate the memory crate** — `memory` crate is an optional workspace member until Phase 6.0 is complete.
3. **Config-driven activation** — Users enable memory by adding `[context.memory]` section to config.
4. **Existing sessions untouched** — JSONL files and SQLite metadata schema are not modified. Memory is a parallel subsystem.

### Migration Steps

```
Before:  icode starts → loads session → assembles context → runs model
After:   icode starts → loads session → loads MEMORY.md + daily notes →
         runs memory_recall → assembles context (engine-dependent) →
         runs model → post-turn memory capture → after_turn hook
```

---

## 6. Testing Strategy

### Unit Tests
- `MemoryStore` CRUD operations with mock embeddings
- `EmbeddingProvider` with local and remote mocks
- `hybrid_search` score merging and re-ranking
- `ContextEngine` trait implementations
- Token estimation accuracy against known inputs
- Compaction with tool-call pair preservation

### Integration Tests
- Full memory lifecycle: store → index → search → recall
- Memory flush before compaction (end-to-end)
- Engine switching without data loss
- Session restart with memory reload

### Parity Tests
- Compare icode memory search results against openclaw for the same query set
- Measure token estimation accuracy vs actual Anthropic API token counts
- Verify compaction summary quality (LLM-as-judge evaluation)

---

## 7. Risk Assessment

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| Local embedding model too slow on cold start | Medium | Medium | Lazy-load model; cache in memory; fallback to keyword-only search |
| Vector index grows unbounded | Medium | Low | Configurable max index size; LRU eviction for low-recall memories |
| Memory flush adds latency to compaction | Low | Medium | Timeout at 5s; non-blocking; skip if memory subsystem not ready |
| `usearch`/`lancedb` has build issues on some platforms | High | Low | Fallback to pure-Rust HNSW implementation; test on CI targets |
| Breaking change to existing session format | Critical | Low | Zero-migration approach; parallel subsystem; no schema changes |
| Embedding API key exposure | High | Low | Key stored in config (existing pattern); no default cloud provider |

---

## 8. Effort Estimate

| Phase | Effort | Dependencies | Can Parallelize With |
|-------|--------|-------------|---------------------|
| 6.0: Memory Subsystem | 3-4 sessions | None | 6.3 (trait design only) |
| 6.1: Memory Files | 1-2 sessions | 6.0 (store) | 6.2 (flush design) |
| 6.2: Memory Flush | 1 session | 6.0, 6.1 | 6.4 (compaction design) |
| 6.3: Context Engine | 2 sessions | 6.0 (memory integration) | 6.4 (compaction impl) |
| 6.4: Better Compaction | 2 sessions | 6.3 (engine trait) | 6.5 (pruning design) |
| 6.5: Context Pruning | 1-2 sessions | 6.3 | 6.6 (dreaming design) |
| 6.6: Dreaming | 1 session | 6.0, 6.1 | None |

**Total**: 11-14 sessions · **Recommended order**: 6.0 → 6.1 → 6.2 → 6.3 → 6.4 → 6.5 → 6.6

---

## 9. Success Metrics

After full implementation:

1. **Memory recall accuracy**: >80% of memory_recall results are relevant to the query (measured by manual audit of 100 queries)
2. **Token estimation accuracy**: Within ±5% of actual provider token count (measured across 50 sessions)
3. **Context loss prevention**: 0% of critical decisions lost during compaction (measured by memory flush coverage)
4. **Compaction quality**: Summaries remain coherent after 5+ compaction cycles (LLM-as-judge score > 7/10)
5. **Memory overhead**: <100ms added to session start for memory load + recall (measured p95)
6. **Index size**: <50MB for 30 days of active usage (measured on typical Rust project)

---

## 10. Appendix: OpenClaw Reference

### Key Files to Study

| OpenClaw File | What to Learn |
|---------------|--------------|
| `extensions/memory-core/src/memory/manager.ts` | SQLite + sqlite-vec memory index manager (974 lines) |
| `extensions/memory-core/src/memory/hybrid.ts` | Hybrid search merging (vector + BM25 + MMR) |
| `extensions/memory-core/src/memory/embeddings.ts` | Multi-provider embedding with auto-select + fallback |
| `extensions/memory-core/src/memory/temporal-decay.ts` | Recency-aware scoring |
| `extensions/memory-core/src/dreaming.ts` | Background consolidation logic |
| `extensions/memory-core/src/tools.ts` | memory_search + memory_get tool implementations |
| `src/context-engine/types.ts` | ContextEngine interface definition |
| `src/context-engine/legacy.ts` | Built-in legacy engine implementation |
| `src/agents/compaction.ts` | Adaptive chunk compaction algorithms |
| `src/auto-reply/reply/memory-flush.ts` | Pre-compaction memory flush |
| `src/agents/memory-search.ts` | Full memory search config resolution (412 lines) |
| `extensions/memory-lancedb/index.ts` | LanceDB plugin (auto-recall + auto-capture) |
| `src/agents/pi-embedded-runner/session-truncation.ts` | Post-compaction file truncation |
| `src/agents/session-transcript-repair.ts` | Tool-call pair repair after truncation |

### OpenClaw Config Knobs to Consider

| Knob | Default | Purpose |
|------|---------|---------|
| `memorySearch.maxResults` | 6 | Max memories returned per search |
| `memorySearch.minScore` | 0.35 | Minimum relevance score |
| `memorySearch.hybridWeights.vector` | 0.7 | Vector search weight in hybrid |
| `memorySearch.hybridWeights.text` | 0.3 | Keyword search weight in hybrid |
| `memorySearch.chunkSize` | 400 tokens | Chunk size for indexing |
| `memorySearch.chunkOverlap` | 80 tokens | Overlap between chunks |
| `memorySearch.temporalDecay.halfLife` | 30 days | Memory decay rate |
| `memorySearch.mmr.lambda` | 0.5 | Diversity vs relevance balance |
| `bootstrapMaxChars` | 20,000 | Max size of a single bootstrap file |
| `bootstrapTotalMaxChars` | 150,000 | Total size of all bootstrap files |
| `compaction.chunkRatio` | 0.4 | Fraction of context to summarize per pass |
| `compaction.safetyMargin` | 1.2 | Multiplier for token estimation inaccuracy |
| `compaction.summarizationOverheadTokens` | 4,096 | Budget reserved for summary text |
