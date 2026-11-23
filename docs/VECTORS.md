# Semantic vectors in .larp (v1.4)

The `.larp` format itself is untouched by semantic recall. All vectors live
in a rebuildable sidecar file next to the memory file, so a lost or corrupt
sidecar is never data loss — `larp index` rebuilds it from the `.larp` file.

## Sidecar file

For a memory file `memory.larp`, the sidecar is `memory.larp.vec`.

### Layout

```
header:
  magic            4 bytes  "LARV"
  version          u16 LE   (= 1)
  dim              u16 LE   (384 for all-MiniLM-L6-v2)
  model-id length  uvarint
  model-id         UTF-8 bytes, e.g. "Xenova/all-MiniLM-L6-v2"
  source mtime     u64 LE   nanoseconds since the Unix epoch
  source length    u64 LE   byte length of the .larp file
  source count     u64 LE   record count of the .larp file

entries (sorted by record id, then chunk index):
  record id        uvarint
  chunk index      uvarint
  vector           dim x f32 LE
```

There is no CRC: the sidecar is a cache, not the source of truth. A sidecar
that fails to parse, or whose header disagrees with the `.larp` file, is
treated as stale and rebuilt — never repaired.

### Staleness

A sidecar is fresh only when **all** of these match the current `.larp`
file and the runtime embedder:

- model-id and dim match the embedder
- source mtime, source byte-length, and source record count match the
  `.larp` file

Any mismatch means stale. `larp index` without `--force` does an
incremental update when the model matches: it keeps vectors for record ids
that still exist and embeds only the missing ones. A model change, a
corrupt sidecar, or `--force` triggers a full rebuild. The sidecar is
written atomically (temp file + fsync + rename), reusing the same
`write_bytes_atomic` helper as the `.larp` writer.

### Chunking

Record text is embedded in chunks of at most **256 tokens** (measured by
the model's tokenizer). Each chunk gets its own vector, keyed by
(record id, chunk index). Recall scores a record by its **best chunk**,
then dedups to one line per record. Short records — the common case —
have exactly one chunk, so the sidecar stays one vector per record in
practice.

Why chunks: the prototype truncated whole sessions to 256 tokens and the
vector leg lost to BM25 on LongMemEval-S (R@10 0.860 vs 1.000). Chunking
is the fix for that starvation.

## Model

- **Model:** Xenova `all-MiniLM-L6-v2` ONNX (~90 MB), dim 384 — the same
  model the Python prototype uses.
- **Cache location:** `$LARP_MODEL_DIR` if set, else
  `$XDG_CACHE_HOME/larp/models`, else `~/.cache/larp/models`.
- **Lazy download:** on first `--semantic` use, `larp` prints a one-time
  notice naming the model and the cache dir, then downloads `model.onnx`
  + `tokenizer.json` from Hugging Face. The download is never silent.
- **Offline fallback:** if the model is absent and the machine is offline
  (or the download fails, or the cached files are corrupt), the command
  prints a notice and falls back to BM25 for that call. It never fails
  for a missing model.

## Hybrid recall

`larp search --semantic` / `larp recall --semantic` (and the MCP `recall`
tool with `"semantic": true`) fuse three legs with reciprocal rank fusion
(RRF, k=60):

| leg    | weight |
|--------|--------|
| BM25   | 0.50   |
| vector | 0.35   |
| graph  | 0.15   |

The graph leg is empty in v1.4 (co-recall graph is a lifecycle follow-up),
so it contributes nothing; the weights are kept as the paper defaults and
**not** silently renormalized. The same salience × type-decay multiplier
that BM25 uses is applied to the fused score.

Vector-leg hygiene, from the prototype: cosine ≤ 0 rows are dropped, so
the vector leg never returns the whole store on a no-overlap query, and
an empty query (zero vector) skips the vector leg entirely.

## Benchmarks

Semantic recall is benchmarked on a fixed in-repo set
(`examples/semantic_bench.rs`): short records plus long records that
exercise chunking, with keyword, paraphrase, and trap queries that have
known answers. Results are reported as BM25 vs vector vs hybrid
(any-hit@5 / R@10) in `docs/BENCHMARKS.md` under "Semantic recall
(fixed set)".

> The fixed paper weights do not win everywhere: on the Python
> prototype's synthetic corpus vector-only beat hybrid (R@10 0.840 vs
> 0.631), while on LongMemEval-S BM25 alone beat hybrid (R@10 1.000 vs
> 0.980). The table reports what the weights do; it does not claim they
> are optimal.
