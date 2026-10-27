# .larp format spec v0.2

## File layout

```
header:  "LARP" (4 bytes) | version u16 LE | reserved u16
record*: payload_len u32 LE | crc32 u32 LE | payload
```

- v1: payload stored raw.
- v2 (current): payload zstd-compressed (level 3). CRC is computed over the
  stored (compressed) bytes.

## Record payload (after decompression for v2)

```
id          u64 LE
type_tag    u8     (0 fact, 1 preference, 2 episode, 3 correction)
timestamp   i64 LE (unix seconds)
salience    f32 LE (0.0–1.0)
ttl_secs    i64 LE (0 = never expires)
supersedes  u64 LE (corrections only, else 0)
text_len    u32 LE
text        UTF-8 bytes
```

## Rules

- Records are append-only. Never rewrite a record in place.
- A correction record supersedes the record named by `supersedes`. Readers
  exclude superseded records from live results; history is preserved.
- A record with `ttl_secs > 0` is expired once `now > timestamp + ttl_secs`.
- A trailing partial frame, CRC mismatch, or undecodable payload means a
  torn write: readers stop there and ignore the rest. Earlier records are
  never affected.
- Readers accept v1 and v2. Writers always emit v2. Appending to a v1 file
  migrates the whole file to v2 (a torn tail, if any, is dropped — it was
  already unrecoverable).
- A single record decompressing beyond 16 MiB is rejected (zip-bomb guard)
  and treated as a torn tail.
- Recall ranking: `score = keyword_match_ratio × salience × 0.5^(age_days/90)`.
- Token estimate: `chars/4 + 1` per record.
- v0.2 search scans records (no footer index yet). A footer index is planned
  with a later compaction upgrade; the record format does not change when it
  lands.
- Known limitation: single appends re-read the file to assign the next id
  (O(n)). A bulk-write path exists for imports; append caching is future work.
- See docs/BENCHMARKS.md for measured size/latency numbers and the honest
  limitations of per-record compression.
