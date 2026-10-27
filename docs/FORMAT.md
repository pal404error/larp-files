# .larp format — public specification (v2)

`.larp` is a single-file format for AI agent memory: small on disk, cheap to
load, searchable, and versioned. No server, no database. Copy it, email it,
commit it to git.

## Design goals

1. **One file, no server.**
2. **Token-budget recall** — ask for memories that fit N tokens, get the best
   ones, never a blind full-file dump.
3. **Versioned and correctable** — memories are superseded, never silently
   rewritten, so history stays traceable.
4. **Crash-safe appends** — a killed writer can only tear the tail.
5. **Human-debuggable** — any file renders as Markdown.

## File layout

```
header:  "LARP" (4 bytes) | version u16 LE | reserved u16
record*: payload_len u32 LE | crc32 u32 LE | payload
```

- **v1** payloads are stored raw. **v2** (current) payloads are
  zstd-compressed at level 3; the CRC is computed over the stored
  (compressed) bytes.
- Readers accept v1 and v2. Writers always emit v2. Appending to a v1 file
  migrates the whole file to v2.

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

## Semantics

- **Append-only.** Records are never rewritten in place.
- **Corrections.** A correction record supersedes the record named by
  `supersedes`. Readers exclude superseded records from live results; the
  old record stays in the file as history.
- **Expiry.** A record with `ttl_secs > 0` is expired once
  `now > timestamp + ttl_secs`.
- **Torn writes.** A trailing partial frame, CRC mismatch, or undecodable
  payload means a torn write: readers stop there and ignore the rest.
  Earlier records are never affected.
- **Safety cap.** A single record decompressing beyond 16 MiB is rejected
  and treated as a torn tail (zip-bomb guard).
- **Recall ranking.** `score = keyword_match_ratio x salience x 0.5^(age_days/90)`.
- **Token estimate.** `chars/4 + 1` per record.
- **Known limitation.** Single appends re-read the file to assign the next id
  (O(n) — ~64 ms at 10,000 records). Use bulk import for large loads.

## CLI

```
larp [--file <path>] append --type <fact|preference|episode|correction>
     [--salience <0..1>] [--ttl <secs>] [--supersedes <id>] <text>
larp [--file <path>] list
larp [--file <path>] dump
larp [--file <path>] search <query>
larp [--file <path>] recall [--max-tokens <n>] [query]
larp [--file <path>] compact
larp [--file <path>] import <path.jsonl>|-     # - reads stdin
larp [--file <path>] export [--live]
larp [--file <path>] mcp                       # MCP server over stdio
larp --version
```

Default file is `memory.larp` in the current directory.

## JSONL interchange

One JSON object per line:

```json
{"id": 1, "type": "fact", "timestamp": 1793106955, "salience": 0.9, "ttl_secs": 0, "supersedes": 0, "text": "the user prefers rust"}
```

On import, ids are preserved when they do not collide with existing records
or each other; otherwise fresh ids are assigned, and `supersedes` references
inside the same batch are remapped to match.

## MCP server

`larp mcp` speaks JSON-RPC 2.0 over stdio (newline-delimited) and exposes:

| Tool | Arguments | Effect |
|---|---|---|
| `remember` | `text` (required), `type`, `salience`, `ttl_secs` | Appends a record, returns `stored #id` |
| `recall` | `query`, `max_tokens` (default 2000) | Returns best memories fitting the budget |
| `forget` | `id` (required) | Appends a correction superseding the record; history is preserved, never deleted |

Add it to any MCP-compatible client:

```json
{"mcpServers": {"larp": {"command": "larp", "args": ["--file", "/path/to/memory.larp", "mcp"]}}}
```

## Versioning

- The crate follows SemVer; the on-disk format has its own version in the
  header. Format changes bump the header version and ship a reader for the
  previous version plus an auto-migration path (as v1 -> v2 did).
- See [BENCHMARKS.md](BENCHMARKS.md) for measured size/latency numbers and
  the honest limitations of per-record compression.
