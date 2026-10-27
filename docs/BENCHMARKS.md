# Benchmarks — Phase 3

Method: `cargo run --release --example bench` — 10,000 generated memories
(facts, preferences, episodes; short texts, heavy template repetition).
Machine: this dev VM. Re-run to reproduce.

## Results

| Metric | Value |
|---|---|
| Records | 10,000 |
| .larp (v3, varint + zstd/raw) size | 881,091 bytes |
| Markdown dump size | 1,147,515 bytes |
| JSONL size | 1,211,242 bytes |
| .larp vs Markdown | **76.8%** |
| .larp vs JSONL | **72.7%** |
| Bulk write (10k records) | 99 ms |
| Search (avg, 10k records) | 5.85 ms |
| Recall (avg, 10k records) | 5.20 ms |
| Single append on 10k file | 4 ms |
| Recall at 2000-token budget | 181 records, 1991 tokens used |

![File size chart](benchmark-chart.svg)

## Regression gate

The Benchmarks workflow fails on regressions vs
[benchmark-baseline.txt](benchmark-baseline.txt), checked by
`.github/scripts/check-bench.py`: sizes and token counts +10%,
percentage points +2.0, latencies 3x (shared runners are noisy), and
the recall count must not drop. To update the baseline after an
intentional change, re-run `cargo run --release --example bench`,
replace the values in the baseline file, and say why in the commit
message.

## Honest reading

**The size target was missed, then narrowed.** The plan aimed for ≥50%
smaller than Markdown. Format v2 measured only 9% smaller: records here
are short (~60–100 chars), so per-record framing (41 bytes of metadata +
zstd frame overhead per record) dominated, and per-record compression
cannot exploit repetition *across* records — which is where most of the
redundancy lives. Format v3 (variable-length header fields, store the
payload raw when compression would not shrink it) closed part of the gap:
**23% smaller than Markdown** (76.8%). The remaining gap is still mostly
cross-record repetition, which only block compression would capture.

**Search got slower and better at the same time.** v2 search was a
substring scan (0.90 ms); v3 ranks with BM25 over whole-word matches
(5.85 ms at 10k records). That is still single-digit milliseconds per
call — fine for one recall per agent turn — and it buys rare-term
weighting and whole-word matching that the substring scan got wrong.
The latency baseline was regenerated for this intentional change, and
the gate still guards against future 3x regressions.

**The token story is the real win.** The full 10,000-record Markdown file is
~287,000 tokens (bytes/4 estimate). Token-budget recall serves the 181
best memories in 1,991 tokens — **99.3% fewer tokens** than a full-file dump.
That, not raw bytes, is what saves agent cost.

## Next optimizations (not yet implemented)

1. **Shared zstd dictionary** stored in the header, trained on record text —
   should compress short records far better than per-record frames alone.
   UPDATE: a static shared dictionary was measured and REJECTED — it cut
   bench-like text to 59.5% of plain size but gave 100.7% (no gain) on
   held-out vocabulary, i.e. it would flatter the benchmark without
   helping real diverse memories. A per-file dictionary trained on that
   file's own records remains a possible future direction; it needs a
   format version bump and a cold-start story for small files.
2. **Block-compressed segments** (compress batches of records together) to
   exploit cross-record repetition.
3. **Faster appends**: DONE in v3 — appends scan frame headers only
   (CRC-checked, no decompression) to find the next id: 4 ms at 10k
   records, down from 65 ms. The MCP server additionally caches records
   between calls.
4. **Semantic recall** (local embeddings mixed with BM25): the relevance
   test suite documents the gap — a paraphrase with no shared words is
   not found. This needs a model download, so it stays future work for
   an offline-first crate.
