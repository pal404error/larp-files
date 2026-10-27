# .larp — agent memory in a single file

Markdown memory files waste tokens. Vector DBs need a server. `.larp` is the
file format in between: small on disk, cheap to load, searchable, versioned.

```bash
larp append --type fact --salience 0.9 "the user prefers rust"
larp append --type preference "likes short answers"
larp recall --max-tokens 2000 rust
```

## Why

| | Markdown memory file | Vector database | .larp |
|---|---|---|---|
| Setup | None | Server + embeddings | None |
| Recall cost | Full file, every session | Query cost | Token budget you set |
| Portable | Yes | No | Yes — one file |
| Versioned/correctable | No | Varies | Yes — corrections supersede |
| Crash-safe | No | Yes | Yes — CRC-checked frames |

## Install

```bash
# From source
git clone https://github.com/pal404error/larp-files.git
cd larp-files
cargo install --path .

# Once published to crates.io
cargo install larp
```

## Quick start

```bash
larp append --type fact --salience 0.9 "the user prefers rust"
larp append --type preference "likes short answers"
larp search rust
larp recall --max-tokens 2000 rust   # best memories fitting 2000 tokens
larp dump                            # render as Markdown
larp compact                         # drop expired/superseded records
larp export > memory.jsonl           # JSONL interchange
larp import backup.jsonl             # restore / merge
```

Use `--file <path>` to pick the memory file (default: `memory.larp`).

## Use it as your agent's memory (MCP)

`larp mcp` speaks JSON-RPC over stdio and exposes three tools: `remember`,
`recall`, and `forget` (which supersedes a record — history is preserved,
never deleted). Add it to any MCP-compatible client:

```json
{"mcpServers": {"larp": {"command": "larp", "args": ["--file", "/path/to/memory.larp", "mcp"]}}}
```

## How recall works

Score = keyword match x salience x recency decay (`0.5^(age_days/90)`).
`recall` returns the best records that fit your token budget — never a
blind full-file dump.

## Benchmarks (10,000 records)

| Metric | Value |
|---|---|
| .larp vs Markdown size | 91.0% |
| .larp vs JSONL size | 86.2% |
| Recall at 2000 tokens | 181 records in 1,991 tokens (vs ~287,000 for the full file — 99.3% fewer) |
| Search / recall latency | 0.85 ms / 0.70 ms |

Honest note: per-record zstd framing limits size wins on short records —
the real saving is tokens, not bytes. Full numbers, method, and next
optimizations: [docs/BENCHMARKS.md](docs/BENCHMARKS.md).

## Format

- Header (`LARP` magic + version) + CRC-checked append-only records.
- v2 payloads are zstd-compressed; v1 files are read and auto-migrated.
- Corrections supersede; expiry via TTL; 16 MiB decompression cap.
- Full spec: [docs/FORMAT.md](docs/FORMAT.md). Changelog:
  [CHANGELOG.md](CHANGELOG.md).
