# Changelog

All notable changes to `.larp` (`larp-memory`) will be documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), adhering to [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

### Planned
- **Shared zstd dictionary training:** Train a dictionary across sample memory corpora to significantly increase compression on short string payloads.
- **Block-compressed segments:** Optional segment chunking to exploit cross-record redundancy while maintaining fast single-record seek.
- **Direct import converters:** Turn existing `CLAUDE.md`, `.cursorrules`, and markdown memory dumps directly into indexed `.larp` files in one command.

---

## [1.1.0] — 2026-10-27
> **"From standalone CLI to native agent standard."**

### Added
- **Agent Skill:** Official [`skills/larp-memory/SKILL.md`](skills/larp-memory/SKILL.md) compatible with the Agent Skills standard (Claude Code, Antigravity, Cursor, Codex).
- **Memory Inspection:** New `larp stats` command displaying total/live/superseded counts, live record breakdown by type, estimated token footprint, and timestamp spans.
- **MCP Ecosystem Hub:** Ready-to-use client configurations for Claude Code, Cursor, VS Code, Codex, and OpenCode in `docs/MCP_CLIENTS.md`.
- **Reference MCP Client:** Dependency-free Python MCP client demonstration in `examples/larp_mcp_client.py`.
- **One-Liner Installers:** Automated `install.sh` (Linux/macOS) and `install.ps1` (Windows PowerShell) fetching prebuilt release binaries.
- **Documentation Hub & FAQ:** Added docs index and FAQ addressing vector DB differences, offline usage, and Windows tips.

### Changed
- Package renamed from `larp` to `larp-memory` on crates.io to avoid registry collisions (binary name remains `larp`).

---

## [1.0.0] — 2026-10-27
> **"First stable format freeze with CRC-checked zstd frames and stdio MCP."**

### Added
- **Format v2 Stabilization:** Header magic `LARP` + format version 2 frozen.
- **Compression & Safety:** Per-record zstd payload compression (level 3) with a strict 16 MiB per-record decompression limit (zip-bomb guard).
- **Deterministic Fuzz Testing:** Added `tests/fuzz_lite.rs` testing reader behavior against 500+ random byte mutations and pathological inputs (zero panics guarantee).
- **Crash Recovery:** Readers gracefully handle torn-tail appends and corrupt records.
- **CI / CD Matrix:** Multi-platform testing across Ubuntu, macOS, and Windows.

---

## [0.3.0] — 2026-10-26
> **"JSONL interoperability and first stdio MCP server."**

### Added
- **Lossless Interchange:** `larp import` and `larp export` with collision-safe ID remapping.
- **MCP Server:** Native stdio JSON-RPC 2.0 server (`larp mcp`) exposing `remember`, `recall`, and `forget`.
- **CLI Flags:** Global `--file <path>` flag and `--version` support.

---

## [0.2.0] — 2026-10-25
> **"Zstandard compression upgrade."**

### Added
- Implemented format v2 zstd compression.
- Automatic v1-to-v2 migration on subsequent appends.
- 10,000-record benchmark suite in `examples/bench.rs`.

---

## [0.1.0] — 2026-10-24
> **"Initial prototype of single-file agent memory."**

### Added
- Core binary file format: header, CRC32 checks, append-only records, and superseded status flags.
- Recency-decayed token-budgeted recall ranking (`keyword_match * salience * 0.5^(age_days/90)`).
- Basic CLI: `append`, `list`, `dump`, `search`, `recall`, `compact`.
