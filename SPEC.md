# .larp format spec v0.3

## File layout

```
header:  "LARP" (4 bytes) | version u16 LE | reserved u16
record*: payload_len uvarint | crc32 u32 LE | id uvarint | flags u8 | payload
```

- v3 (current): frames use variable-length integers (LEB128; signed values
  zigzag-encoded). Flags bit 0 means the payload is zstd-compressed
  (level 3); when compression would not shrink a payload it is stored raw
  instead. The CRC covers the id bytes, the flags byte, and the stored
  payload bytes. The record id lives in the frame, so an append can find
  the next id by scanning frame headers without decompressing anything.
- v2: `payload_len u32 LE | crc32 u32 LE | payload`, payload
  zstd-compressed. CRC over the stored (compressed) bytes.
- v1: same framing as v2, payload stored raw.

## Record payload (v3; after decompression when compressed)

```
type_tag    u8     (0 fact, 1 preference, 2 episode, 3 correction)
timestamp   svarint (unix seconds)
salience    f32 LE (0.0–1.0; non-finite values are rejected on write)
ttl_secs    svarint (0 = never expires)
supersedes  uvarint (corrections only, else 0)
text_len    uvarint
text        UTF-8 bytes
tag_count   uvarint
tags        tag_len uvarint + UTF-8 bytes, repeated
meta_count  uvarint
meta        key_len uvarint + key + val_len uvarint + value, repeated
```

v1/v2 payloads (legacy) use the fixed 41-byte header instead:

```
id          u64 LE
type_tag    u8
timestamp   i64 LE
salience    f32 LE
ttl_secs    i64 LE
supersedes  u64 LE
text_len    u32 LE
text        UTF-8 bytes
```

v1/v2 records have no tags or metadata; they read as empty.

## Rules

- Records are append-only. Never rewrite a record in place.
- A correction record supersedes the record named by `supersedes`. Only
  corrections supersede; the field is inert on other types.
- Liveness is evaluated by parity along each supersession chain: a
  correction supersedes its target, but if that correction is itself
  superseded (for example, forgotten), the target revives. So forgetting
  a correction undoes the forget. Expired corrections stop superseding.
- A record with `ttl_secs > 0` is expired once `now > timestamp + ttl_secs`.
- A trailing partial frame, CRC mismatch, unknown flag bit, or undecodable
  payload means a torn write: readers stop there and ignore the rest.
  Earlier records are never affected. **Appends truncate the file back
  to the last good frame before writing**, so a crash mid-write can never
  make later appends silently unreachable or reuse ids.
- Whole-file rewrites (compact, import, migration) write a temp file in
  the same directory, fsync it, then rename it over the original, so a
  crash mid-rewrite leaves the old file intact. Appends fsync before
  returning.
- All mutations take an advisory exclusive lock on `<file>.lock`, so two
  writers cannot interleave or lose each other's records. The lock is
  advisory: it coordinates larp writers, it cannot stop other programs
  editing the file by hand.
- Readers accept v1, v2, and v3. Writers always emit v3. Appending to a
  v1/v2 file migrates the whole file to v3 (a torn tail, if any, is
  dropped — it was already unrecoverable). Old binaries cannot read v3.
- A single record decompressing beyond 16 MiB is rejected (zip-bomb guard)
  and treated as a torn tail.
- Recall ranking: BM25 (k1 = 1.2, b = 0.75) over whole-word matches,
  times salience, times a decay that applies to episodes only (halving
  every 90 days). Facts and preferences do not fade. Empty query ranks by
  salience (episodes decayed).
- Token estimate: `chars/4 + 1` per record.
- The store entry points (CLI append, MCP remember) refuse near-duplicates
  of live records of the same type (identical normalized text, or word-set
  Jaccard >= 0.9) and report the existing id instead of storing twice.
- See docs/BENCHMARKS.md for measured size/latency numbers and the honest
  limitations of per-record compression.
