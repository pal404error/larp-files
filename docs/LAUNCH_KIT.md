# Launch Kit & Discovery Playbook for `.larp`

This kit provides ready-to-use launch drafts, community submission pull requests, and repository metadata. **All items here are drafts intended for human review before publishing.**

---

## 1. GitHub Repository Metadata & SEO

Update the GitHub repository settings (About section) to capture high-intent agent developer searches:

### Description
> Single-file memory format for AI agents. Crash-safe, append-only, token-budgeted recall, and native Model Context Protocol (MCP) server in a single Rust binary.

### Recommended GitHub Topics
```text
agent-memory, mcp-server, model-context-protocol, ai-agents, context-engineering, local-first, rust-cli, token-optimization, memory-management, zstd
```

---

## 2. Launch Copy Drafts

### A. Hacker News — Show HN

**Title:**
> Show HN: .larp – Single-file agent memory format (Rust, MCP, 99% fewer tokens)

**Body Text:**
```text
Hey HN,

When building long-running AI coding agents, you quickly run into the memory dilemma:
1. Flat Markdown files: Every session, your agent re-reads the entire file. At 10,000 memories, you dump ~287,000 tokens into the prompt before the user even types a word.
2. Vector Databases: Require background daemons, Docker containers, external embedding models, and float vector indexing for what is often just 50–200 key facts.

I built .larp (https://github.com/pal404error/larp-files) as the local-first middle ground. It's a single-file format with a standalone Rust binary (`larp-memory` crate) and native Model Context Protocol (MCP) server.

How it works:
- Append-only binary framing: Header with magic bytes + CRC32-checked records with per-record zstd compression (v2).
- Non-destructive updates: Corrections supersede prior records. The old records stay in the file for full auditability until you explicitly run `larp compact`.
- Token-budgeted recall: Returns only the highest-scoring memories that fit strictly within your token budget (Score = keyword_match * salience * 0.5^(age_days/90)). In benchmarks on 10k records, a 2,000-token budget returned the 181 most relevant memories in 1,991 tokens (99.3% reduction in context window waste).
- Native MCP: Speaks JSON-RPC 2.0 over stdio with `remember`, `recall`, and `forget` tools for Claude Code, Cursor, VS Code, and Codex.

An honest note on tradeoffs:
Per-record zstd framing adds ~41 bytes of metadata overhead per record. On very short records (60–100 chars), .larp is only ~9% smaller on disk than raw Markdown (1.04 MB vs 1.15 MB). The real gain is token cost and prompt latency, not disk bytes. We're exploring header-trained shared zstd dictionaries for the next version.

Zero external databases, zero background daemons, zero network telemetry. Everything runs locally in sub-millisecond time.

Repo & Spec: https://github.com/pal404error/larp-files
Format specification: https://github.com/pal404error/larp-files/blob/main/docs/FORMAT.md

I'd love feedback on the format layout and recall scoring heuristics!
```

---

### B. Reddit — r/rust

**Title:**
> .larp 1.1.0 – A single-file agent memory format in Rust (MCP, CRC-checked zstd frames, std-only CLI)

**Body Text:**
```text
Hi r/rust!

I wanted to share .larp (`larp-memory` on crates.io, CLI `larp`), an open-source single-file format designed to give AI agents persistent memory without needing an external vector database or dumping giant markdown files into context windows.

Repo: https://github.com/pal404error/larp-files

Technical highlights:
- Pure Rust, std-only CLI arg parsing, zero unsafe code blocks.
- Format v2: 12-byte header (`LARP` magic + v2 flag), followed by append-only records: 8-byte ID, 8-byte timestamp, 8-byte TTL, 8-byte supersedes ID, 1-byte type tag, 4-byte salience float, 4-byte compressed payload length, 4-byte CRC32, followed by the zstd compressed payload.
- Crash resilience: If a write is interrupted mid-record, the reader ignores the torn tail. Fuzz tests (`tests/fuzz_lite.rs`) test the reader against 500+ random bit-flips and pathological payloads to ensure it never panics.
- Zip-bomb defense: Enforces a strict 16 MiB per-record decompression limit.
- Native stdio MCP server: Implements JSON-RPC 2.0 directly over stdin/stdout for AI tools.

Question for the community:
In our 10,000-record benchmark, per-record zstd framing meant .larp was only 9% smaller than raw markdown on disk for small (~80 char) records due to zstd frame headers and lack of cross-record dictionary sharing. 
Would you recommend introducing a header-trained shared zstd dictionary, or chunking records into multi-record compressed blocks? Curious how others have balanced single-record random seeks vs compression ratios.
```

---

### C. Product Hunt

- **Name:** `.larp`
- **Tagline:** Agent memory in a single file — small on disk, cheap to recall
- **Description:**
  `.larp` replaces token-wasting Markdown logs and complex vector databases with a single, crash-safe memory file for AI coding agents. Includes a standalone Rust CLI, drop-in Model Context Protocol (MCP) server, and an agent skill for Claude Code, Cursor, and Codex.
- **First Maker Comment:**
  *"We built .larp because flat memory files exhaust agent context windows, and vector databases are overkill for local developer tools. .larp serves the best memories within a strict token budget—cutting context consumption by 99% in sub-millisecond time with zero external servers."*

---

## 3. Curated Directory & Awesome-List Pull Requests

Here are 5 pre-drafted submissions for top directories:

### 1. `punkpeye/awesome-mcp-servers`
- **Category:** Tools / Knowledge & Memory
- **Entry Markdown:**
  ```markdown
  - [.larp](https://github.com/pal404error/larp-files) - Single-file, crash-safe agent memory format and MCP server with token-budgeted recall, zstd compression, and auditable corrections.
  ```
- **PR Title:** `Add .larp memory server to Knowledge & Memory`

---

### 2. `modelcontextprotocol/servers`
- **Location:** Community Servers / Storage & Memory
- **Entry Markdown:**
  ```markdown
  - **[.larp Memory](https://github.com/pal404error/larp-files)**: Single-file, local-first agent memory with token-budgeted recall, versioned corrections, and zero external database dependencies.
  ```
- **PR Title:** `feat: add .larp single-file memory server`

---

### 3. `e2b-dev/awesome-ai-agents`
- **Category:** Memory & Context Management
- **Entry Markdown:**
  ```markdown
  - [.larp](https://github.com/pal404error/larp-files) - Local-first agent memory format in Rust. Uses CRC-checked append-only records and token-budget recall to cut context consumption by 99.3%.
  ```
- **PR Title:** `Add .larp to Memory & Context Management`

---

### 4. `rust-unofficial/awesome-rust`
- **Category:** Applications / Command-line utilities (or Development tools)
- **Entry Markdown:**
  ```markdown
  - [pal404error/larp-files](https://github.com/pal404error/larp-files) - Single-file memory format and CLI for AI agents with zstd compression and Model Context Protocol support.
  ```
- **PR Title:** `Add larp-memory to CLI applications`

---

### 5. `VoltAgent/awesome-agent-skills`
- **Category:** Skills / Context & Memory
- **Entry Markdown:**
  ```markdown
  - [larp-memory](https://github.com/pal404error/larp-files/tree/main/skills/larp-memory) - Agent skill teaching models when to remember, recall within token limits, and supersede outdated facts using .larp files.
  ```
- **PR Title:** `Add larp-memory skill`

---

## 4. High-Impact Roadmap (Effort vs. Star Velocity)

Three future feature releases that would each warrant a standalone announcement:

| Feature | Effort | Star Velocity / Impact | Why Developers Will Care |
|---|:---:|:---:|---|
| **1. Shared Zstd Dictionary Training** | Medium | **High** | Solves the per-record overhead; enables 60%+ disk size reduction over Markdown for short memories. Major technical Show HN update. |
| **2. One-Command Importers (`larp ingest`)** | Low | **Very High** | Ingest `CLAUDE.md`, `.cursorrules`, ChatGPT memory exports, or JSON logs directly into `.larp` with automatic type inference and salience scoring. Instant migration path. |
| **3. Interactive Terminal Memory Inspector (`larp tui`)** | Medium | **High** | Ratatui-based TUI to view, search, filter, and inspect memory timelines and audit superseded chains visually in the terminal. Incredible for demos and Twitter/X GIFs. |
