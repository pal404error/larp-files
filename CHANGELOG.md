# Changelog

All notable changes to `.larp` (`larp-memory`) will be documented in this file.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), adhering to [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

### Planned
- **Block-compressed segments:** Optional segment chunking to exploit cross-record redundancy while maintaining fast single-record seek.
- **Direct import converters:** Turn existing `CLAUDE.md`, `.cursorrules`, and markdown memory dumps directly into indexed `.larp` files in one command.
- **Semantic recall:** Local embeddings mixed with BM25. The relevance
  tests document the current gap (paraphrases with no shared words are
  not found); this needs a model download, so it stays future work for
  an offline-first crate.

---

## [1.2.0] — 2026-10-28

### Added
- **Benchmark regression gate:** the Benchmarks workflow now fails on
  regressions vs `docs/benchmark-baseline.txt` (sizes/tokens +10%,
  percentage points +2.0, latencies 3x, recall count must not drop),
  checked by `.github/scripts/check-bench.py`.
- **Shell completions** for bash, zsh, and fish in `completions/`.
- **Format v3: tags and metadata.** Records carry project/namespace
  tags (`--tag`, repeatable) and extensible key/value metadata
  (`--meta k=v`); `search` and `recall` accept a `--tag` filter, and
  the MCP `remember`/`recall` tools take `tags`/`meta`/`tag`.
- **MCP `list` and `stats` tools.** Recall lines now carry score,
  salience, and timestamp so callers can judge staleness.
- **Near-duplicate detection on store.** CLI append and MCP remember
  refuse near-duplicates of live records of the same type (identical
  normalized text, or word-set Jaccard >= 0.9) and report the existing
  id (`already stored as #N`) instead of storing twice.
- **Relevance tests** with curated queries and known right answers
  (including a deliberately ignored test documenting the semantic
  paraphrase gap).

### Fixed
- **Crash-damaged tails no longer eat later appends.** Appends truncate
  back to the last good frame before writing, so a record appended
  after a crash is reachable and ids never repeat (was: silently lost,
  same id twice).
- **NaN/infinite salience rejected on write** (CLI, MCP, library); a
  NaN record already on disk scores 0 instead of crashing recall's
  sort (was: empty-query recall panicked, and over MCP it took the
  server down every call).
- **Whole-file rewrites are atomic.** Compact, import, and migration
  write a temp file, fsync it, then rename it over the original (was:
  in-place overwrite — a crash mid-write lost everything).
- **Two writers are safe.** Mutations take an advisory exclusive lock
  on `<file>.lock` (new `fs2` dependency), so concurrent appends
  serialize instead of interleaving or losing records.
- **Appends are durable.** Appends fsync before returning (was: flush
  only, so "appended" did not mean on disk).
- **MCP corrections actually correct.** `remember` now accepts
  `supersedes` (was: offered `type: correction` with no way to name
  the target, so the old fact stayed live).
- **Forgetting a correction revives its target.** Liveness follows
  parity along supersession chains (was: a forgotten correction left
  its target dead forever).
- **MCP protocol version** is now the version the server implements
  (`2024-11-05`), not an echo of the client's version.

### Changed
- **Format v3** (readers still accept v1/v2; writers emit v3; appending
  to v1/v2 migrates): variable-length frame and record headers, record
  id moved into the frame, payload stored raw when compression would
  not shrink it. File size at 10k records: **76.8% of Markdown** (was
  91.0%), **72.7% of JSONL** (was 86.2%). Old binaries cannot read v3.
- **Recall ranking is BM25** (k1 = 1.2, b = 0.75) over whole-word
  matches — "rust" no longer matches "trust" — times salience, with
  recency decay on episodes only. Facts and preferences no longer fade
  against weak recent matches. Search at 10k records: 5.85 ms (was
  0.90 ms for a substring scan); the baseline was regenerated for
  this intentional change.
- **Appends are 16x faster** at 10k records (4 ms, was 65 ms): v3
  appends scan frame headers only, no decompression. The MCP server
  additionally caches records between calls.
- **Benchmarks refreshed** to a current run; the static shared
  dictionary idea was measured and rejected (see BENCHMARKS.md).

### Removed
- **All emojis** from README headings, the banner, and the benchmark
  chart (13 removed, verified zero remain).

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
