# Architecture

A map of the codebase for contributors. Line counts are approximate
(v1.4.0).

## Layout

```
larp-files/
├── src/
│   ├── lib.rs        # ~2,000 lines — core: frames, records, BM25, RRF, recall
│   ├── main.rs       # ~650 lines — CLI (append/search/recall/index/mcp/…)
│   ├── mcp.rs        # ~550 lines — MCP server over stdio (5 tools)
│   ├── sidecar.rs    # ~560 lines — .vec embedding sidecar format
│   └── embed.rs      # ~450 lines — ONNX embedder (MiniLM) + model download
├── tests/
│   ├── integration.rs  # ~1,400 lines — CLI + MCP end-to-end tests
│   └── fuzz_lite.rs    # ~90 lines — corrupt-input property tests
├── examples/
│   ├── bench.rs            # 10k-record benchmark harness
│   ├── semantic_bench.rs   # fixed 9-query semantic benchmark
│   ├── quickstart.rs       # the README quickstart, as code
│   └── larp_mcp_client.py  # reference MCP client (stdio JSON-RPC)
├── skills/larp-memory/SKILL.md  # agent skill (Agent Skills standard)
├── completions/          # bash / zsh / fish tab completion
├── docs/                 # all documentation (see docs/README.md)
└── .github/workflows/    # tests, benchmarks, coverage, security, release
```

## The core (`src/lib.rs`)

Everything the format promises lives here:

- **Framing** — `encode_frame_v3` / `scan_v3_headers`: `[len][crc32][id]
  [flags][payload]` frames, append-only, CRC32 over id + flags + payload.
  A torn tail is detected by CRC and truncated by `repair`, never trusted.
- **Records** — type (fact / preference / episode / correction), zigzag
  timestamp, f32 salience, TTL, `supersedes` id, text, tags, meta.
  Corrections supersede; history is never rewritten.
- **Ranking** — BM25 (k1=1.2, b=0.75, whole-word) × salience ×
  `0.5^(age_days/90)` episode-only decay. `recall` fills a token budget
  with the top-ranked live records, never exceeding it.
- **Hybrid recall** — `rrf_fuse` merges the BM25 leg and the vector leg
  (weights 0.50/0.35/0.15, k=60; the graph leg is reserved). The fused
  score gets the same salience × decay multiplier as BM25.
- **Safety rails** — 16 MiB per-record decompression cap, `MAX_RECORD_BYTES`
  enforced on encode, saturating TTL arithmetic, `next_record_id` errors
  instead of wrapping, atomic rewrites via private temp file + fsync +
  rename + dir fsync, advisory file lock (`fs2`) on every mutation.

## The vector sidecar (`src/sidecar.rs`, `src/embed.rs`)

Semantic recall is an **optional cache, not part of the format** —
`SPEC.md` stays v0.3 and a `.larp` file is fully usable without it.

- `sidecar.rs` — the `.vec` file: `LARV` magic, version, embedding dim,
  model id, source mtime/len/count, then `(id, chunk, f32[dim])` entries
  sorted by (id, chunk). No CRC: it is rebuildable, so `build_index`
  rebuilds on corruption instead of erroring. Indexing snapshots records
  under the `.larp` lock, embeds unlocked, re-stats before an atomic
  write, and retries on drift (≤2).
- `embed.rs` — `OnnxEmbedder`: loads Xenova `all-MiniLM-L6-v2`
  (`tokenizer.json` + `model.onnx`, ~90 MB, lazily downloaded to the
  model cache with a one-time notice), mean-pool + L2-normalize,
  ≤256-token chunks with the best chunk per record. Any failure —
  offline, corrupt model, missing sidecar — falls back to BM25 with a
  stderr notice; the command never fails because of embeddings.

## The CLI (`src/main.rs`)

Thin argument parsing over the library. Global `--file <path>` selects
the memory file (default `memory.larp`). Commands: `append`, `list`,
`dump`, `search`, `recall`, `compact`, `stats`, `import`/`export`,
`import-md`, `init`, `verify`, `repair`, `index`, `mcp`. `search` and
`recall` take `--semantic`; `index` takes `--force`.

## The MCP server (`src/mcp.rs`)

Stdio JSON-RPC 2.0, protocol `2024-11-05`, five tools: `remember`,
`recall` (optional `semantic` boolean, BM25 fallback noted in output),
`forget`, `list`, `stats`. Memory text is treated as **data, never as
instructions** — tool output escapes anything that could forge output
lines. `remember`/`forget` invalidate the cached vector sidecar.

## Testing

- `cargo test --locked --all-targets` — 66 tests (20 lib + 2 fuzz +
  44 integration), 0 ignored. CI additionally fails the run if any
  test is `#[ignore]`d.
- Integration tests drive the real CLI binary and the real MCP server
  over stdio; the semantic tests use the real model (cached in CI).
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
  and `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` all run in CI.
- Benchmarks: `cargo run --release --example bench` (10k records) and
  `cargo run --release --example semantic_bench` (fixed 9-query set).
  `docs/benchmark-baseline.txt` is the regression gate.

## CI / release

- `tests.yml` — matrix (ubuntu/macOS/Windows × stable/beta) + release-mode
  tests + examples/CLI/MCP smoke + lint (clippy, fmt, rustdoc, audit).
- `benchmarks.yml` — weekly bench + regression gate. `coverage.yml` —
  tarpaulin (informational, no threshold yet). `security.yml` — cargo
  audit on manifest changes + weekly.
- `release.yml` — on tag `v*`: per-OS builds, SHA256SUMS, provenance
  attestations, GitHub release. Windows links ONNX Runtime dynamically
  (NuGet 1.31.0) and ships the DLLs next to `larp.exe`.
