# .larp — agent memory file format

Markdown memory files waste tokens. Vector DBs need a server. `.larp` is the
file format in between: small on disk, cheap to load, searchable, versioned.

## Quick start

```bash
cargo build --release
./target/release/larp append --type fact --salience 0.9 "the user prefers rust"
./target/release/larp append --type preference "likes short answers"
./target/release/larp search rust
./target/release/larp recall --max-tokens 2000 rust
./target/release/larp dump
./target/release/larp compact
```

Use `--file <path>` to pick the memory file (default: `memory.larp`).

## Design

- **One file, no server.** Copy it, email it, commit it to git.
- **Token-budget recall.** `recall --max-tokens N` returns the highest-value
  memories that fit — never a blind full-file dump.
- **Versioned and correctable.** Corrections supersede old records; history
  is preserved, the latest truth always wins.
- **Crash-safe appends.** CRC-checked frames mean a killed writer can only
  tear the tail — earlier records are never corrupted.
- **Compressed.** v2 payloads are zstd-compressed; v1 files are read and
  auto-migrated on append.
- **Human-debuggable.** `dump` renders any file as Markdown.

See [SPEC.md](SPEC.md) for the full format specification and
[docs/BENCHMARKS.md](docs/BENCHMARKS.md) for measured numbers (including the
honest limitations of per-record compression).
