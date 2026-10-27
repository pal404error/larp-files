# .larp format — public specification (v3)

`.larp` is a single-file format for AI agent memory: small on disk, cheap to
load, searchable, and versioned. No server, no database. Copy it, email it,
commit it to git.

## Design goals

1. **One file, no server.**
2. **Token-budget recall** — ask for memories that fit N tokens, get the best
   ones, never a blind full-file dump.
3. **Versioned and correctable** — memories are superseded, never silently
   rewritten, so history stays traceable.
4. **Crash-safe appends** — a killed writer can only tear the tail, and the
   next append truncates back to the last good frame before writing.
5. **Safe for parallel writers** — mutations take an advisory lock, so two
   agents appending at once cannot interleave or lose records.
6. **Human-debuggable** — any file renders as Markdown.

## File layout

```
header:  "LARP" (4 bytes) | version u16 LE | reserved u16
record*: payload_len uvarint | crc32 u32 LE | id uvarint | flags u8 | payload
```

- **v3** (current) frames use variable-length integers (LEB128; signed
  values zigzag-encoded). Flags bit 0 means zstd-compressed at level 3;
  when compression would not shrink a payload it is stored raw instead.
  The CRC covers the id bytes, the flags byte, and the stored payload.
  The id lives in the frame, so appends find the next id by scanning
  frame headers without decompressing anything.
- **v2** frames are `payload_len u32 LE | crc32 u32 LE | payload` with a
  zstd-compressed payload; **v1** is the same framing with a raw payload.
- Readers accept v1, v2, and v3. Writers always emit v3. Appending to a
  v1/v2 file migrates the whole file to v3.

## Record payload (v3; after decompression when compressed)

```
type_tag    u8     (0 fact, 1 preference, 2 episode, 3 correction)
timestamp   svarint (unix seconds)
salience    f32 LE (0.0–1.0)
ttl_secs    svarint (0 = never expires)
supersedes  uvarint (corrections only, else 0)
text_len    uvarint
text        UTF-8 bytes
tag_count   uvarint
tags        tag_len uvarint + UTF-8 bytes, repeated
meta_count  uvarint
meta        key_len uvarint + key + val_len uvarint + value, repeated
```

v1/v2 payloads use the legacy fixed 41-byte header (id, type, timestamp,
salience, ttl, supersedes, text length, text) and have no tags/metadata.

## Semantics

- **Append-only.** Records are never rewritten in place.
- **Corrections.** A correction record supersedes the record named by
  `supersedes` (only corrections supersede). Liveness follows parity
  along each chain: if a correction is itself superseded — for example,
  forgotten — its target revives. So forgetting a correction undoes the
  forget. Readers exclude non-live records from results; old records stay
  in the file as history.
- **Expiry.** A record with `ttl_secs > 0` is expired once
  `now > timestamp + ttl_secs`. Expired corrections stop superseding.
- **Torn writes.** A trailing partial frame, CRC mismatch, unknown flag
  bit, or undecodable payload means a torn write: readers stop there and
  ignore the rest. Earlier records are never affected. The next append
  truncates back to the last good frame first, so nothing written after
  a crash is silently lost.
- **Durability.** Whole-file rewrites (compact, import, migration) write
  a temp file, fsync it, then rename it over the original. Appends fsync
  before returning. Mutations hold an advisory exclusive lock on
  `<file>.lock`.
- **Safety cap.** A single record decompressing beyond 16 MiB is rejected
  and treated as a torn tail (zip-bomb guard).
- **Recall ranking.** BM25 (k1 = 1.2, b = 0.75) over whole-word matches —
  "rust" does not match "trust" — times salience, times a decay that
  applies to episodes only (halving every 90 days). Facts and preferences
  do not fade. Rare terms outweigh common ones.
- **Token estimate.** `chars/4 + 1` per record.
- **Duplicates.** The store entry points (CLI append, MCP remember)
  refuse near-duplicates of live records of the same type and report the
  existing id instead.
- **Tags.** Records carry project/namespace tags and extensible metadata;
  recall and search accept a tag filter.

## CLI

```
larp [--file <path>] append --type <fact|preference|episode|correction>
     [--salience <0..1>] [--ttl <secs>] [--supersedes <id>]
     [--tag <t>]... [--meta <k=v>]... <text>
larp [--file <path>] list
larp [--file <path>] dump
larp [--file <path>] search [--tag <t>] <query>
larp [--file <path>] recall [--max-tokens <n>] [--tag <t>] [query]
larp [--file <path>] compact
larp [--file <path>] import <path.jsonl>|-     # - reads stdin
larp [--file <path>] export [--live]
larp [--file <path>] mcp                       # MCP server over stdio
larp --version
```

Default file is `memory.larp` in the current directory. Recall, search,
and list lines carry score (when ranked), salience, timestamp, and tags,
so a caller can judge staleness.

## JSONL interchange

One JSON object per line:

```json
{"id": 1, "type": "fact", "timestamp": 1793106955, "salience": 0.9, "ttl_secs": 0, "supersedes": 0, "text": "the user prefers rust", "tags": ["work"]}
```

`tags` and `meta` are omitted when empty, so old interchange files still
parse. On import, ids are preserved when they do not collide with existing
records or each other; otherwise fresh ids are assigned, and `supersedes`
references inside the same batch are remapped to match.

## MCP server

`larp mcp` speaks JSON-RPC 2.0 over stdio (newline-delimited) and exposes:

| Tool | Arguments | Effect |
|---|---|---|
| `remember` | `text` (required), `type`, `salience`, `ttl_secs`, `supersedes`, `tags`, `meta` | Appends a record, returns `stored #id` — or `already stored as #id` for a near-duplicate |
| `recall` | `query`, `max_tokens` (default 2000), `tag` | Returns best memories fitting the budget, with score, salience, and timestamp per line |
| `forget` | `id` (required) | Appends a correction superseding the record; history is preserved, never deleted; forgetting a correction revives its target |
| `list` | `limit` (default 100) | Lists live memories |
| `stats` | — | File statistics: totals, live counts by type, token estimate |

The server answers protocol version `2024-11-05`, the version it
implements, and caches records between calls (reloading when the file's
mtime or size changes), so calls no longer re-read the whole file.

Add it to any MCP-compatible client:

```json
{"mcpServers": {"larp": {"command": "larp", "args": ["--file", "/path/to/memory.larp", "mcp"]}}}
```

## Versioning

- The crate follows SemVer; the on-disk format has its own version in the
  header. Format changes bump the header version and ship a reader for the
  previous versions plus an auto-migration path (as v1 -> v2 and v2 -> v3
  did). Old binaries cannot read v3 files.
- See [BENCHMARKS.md](BENCHMARKS.md) for measured size/latency numbers and
  the honest limitations of per-record compression.
