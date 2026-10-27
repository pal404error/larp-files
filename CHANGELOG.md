# Changelog

## Unreleased

- Hardening: deterministic fuzz-lite parser tests (500 random mutations +
  pathological inputs, reader must never panic).
- Hardening: MCP server ignores malformed JSON lines with a clean -32700
  error instead of failing.
- CI: GitHub Actions workflow (test, clippy, fmt, cargo audit).

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
