# Changelog

## Unreleased

- CI: split into a dedicated Tests workflow (test matrix across Ubuntu,
  macOS, Windows + lint job) and a Benchmarks workflow (runs the 10,000
  record benchmark on pushes to main, relevant PRs, weekly, and on demand;
  results go to the job summary and an artifact).

## 1.0.0 — 2026-10-27

First stable release. Format v2 is frozen: header + CRC-checked append-only
records, zstd payloads, corrections that supersede, expiry, token-budget
recall, JSONL interchange, and an MCP server (`remember` / `recall` /
`forget`). Readers accept v1 and auto-migrate on append.

- Hardening: deterministic fuzz-lite parser tests (500 random mutations +
  pathological inputs, reader must never panic).
- Hardening: MCP server answers malformed JSON lines with a clean -32700
  error instead of failing.
- CI: GitHub Actions workflow (test, clippy, fmt, cargo audit).
- Release workflow: prebuilt binaries on tag push (Linux, macOS, Windows).
- Docs: public format spec (docs/FORMAT.md), benchmarks
  (docs/BENCHMARKS.md), launch plan (docs/LAUNCH.md).

## 0.3.0

- JSONL interchange: `larp import` / `larp export` (ids preserved when they
  do not collide; collisions get fresh ids and `supersedes` references inside
  the same batch are remapped).
- MCP server: `larp mcp` over stdio with `remember`, `recall`, and `forget`
  (supersede, never delete) tools.
- CLI: `larp --version` / `-V`.

## 0.2.0

- Format v2: record payloads are zstd-compressed (level 3).
- Reader accepts v1 (raw) and v2; appending to a v1 file migrates it to v2.
- Bulk `write_file` path and a 10,000-record benchmark example
  (`cargo run --release --example bench`); see docs/BENCHMARKS.md.
- 16 MiB per-record decompression cap (zip-bomb guard).

## 0.1.0

- Initial release: binary format with header, CRC-checked append-only
  records, torn-tail safety, corrections that supersede, expiry,
  token-budget recall ranked by keyword match x salience x recency decay.
- CLI: `append`, `list`, `dump`, `search`, `recall`, `compact`.
