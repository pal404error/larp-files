# FAQ

## Is .larp a vector database?

No. Search is keyword-based, ranked by keyword match x salience x
recency decay. It will not find a memory phrased completely differently
from your query. If you need semantic recall, pair it with embeddings
or use a vector store. See [COMPARISON.md](COMPARISON.md).

## Does it work offline?

Yes. One binary, one file, no server, no API key, no network after
install.

## Can two agents share one memory file?

One writer at a time is the intended shape. Two agents appending to the
same file concurrently can interleave records. Use one file per agent,
or serialize writes.

## What happens if the process crashes mid-append?

Only the tail can tear. Records are CRC-checked and length-framed, so a
torn or corrupt tail is detected and ignored; earlier records are never
harmed. Run `larp compact` to rewrite a clean file.

## Do corrections delete history?

No. A correction appends a new record that supersedes the old one. The
old record stays in the file for audit but stops being live. `larp
export --live` shows only live records.

## Is my memory file private?

It is a local file — nothing leaves your machine unless you copy it.
Never store secrets, tokens, or credentials as memories.

## Why is .larp only ~9% smaller than Markdown?

Per-record zstd framing dominates on short records, and per-record
compression cannot exploit repetition across records. The real win is
recall tokens (1,991 vs ~287,000 at 10,000 records), not bytes. Shared
dictionaries and block compression are documented follow-ups in
[BENCHMARKS.md](BENCHMARKS.md).

## How do I install it on Windows?

Download `larp-Windows.exe` from the
[releases page](https://github.com/pal404error/larp-files/releases),
rename it to `larp.exe`, and put it on your PATH. The `install.sh`
one-liner covers Linux and macOS only.

## Where do I report bugs or request features?

Open an issue — templates for bugs and feature requests are in
`.github/ISSUE_TEMPLATE/`. For a security issue, use GitHub's private
vulnerability reporting (see [SECURITY.md](../SECURITY.md)).
