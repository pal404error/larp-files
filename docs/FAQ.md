# Frequently Asked Questions (FAQ)

Everything you need to know about `.larp`, design choices, troubleshooting, and edge cases.

---

### Is `.larp` a vector database?
**No.** Vector databases require an embedding model, an indexing daemon (or server), and float vector calculations. `.larp` is a **single-file, local-first format** that scores records deterministically using:

$$\text{Score} = \text{keyword\_match} \times \text{salience} \times 0.5^{(\text{age\_days} / 90)}$$

This runs in sub-millisecond time ($<1\text{ ms}$ on 10,000 records) on any machine with zero external infrastructure and zero API cost. If you need semantic search across paraphrased queries, pair it with an embedding sidecar or vector store (see [COMPARISON.md](COMPARISON.md)).

---

### Does `.larp` work completely offline?
**Yes.** One binary, one file, no background server, no API key, and zero telemetry pings. The CLI and the MCP server only touch the specific `.larp` file specified on your local disk.

---

### Can two agents share one memory file?
One writer at a time is the intended architecture. Two agents appending to the same file concurrently can interleave frames. Best practice: use one `.larp` file per agent/workspace, or serialize writes.

---

### What happens if the process crashes mid-append?
Only the tail can tear. Records are CRC32-checked and length-framed; a torn or corrupt tail is detected and safely ignored by readers, ensuring prior history is never corrupted. Run `larp compact` to write out a cleaned file.

---

### How are memories updated or deleted without breaking append-only guarantees?
`.larp` preserves complete auditability:
- **Updating a memory:** Append a record with `--type correction --supersedes <ID>`. The previous record remains in the file with its timestamp for audit, but is marked as superseded during `recall` and `list`.
- **Deleting a memory:** Use `forget(id)` via MCP or append a tombstone.
- **Permanent cleanup:** Run `larp compact` to safely purge superseded or expired records and reclaim disk space.

---

### How does `.larp` save tokens if it's not a full database?
Markdown memory files force agents to re-read thousands of lines of past logs at the start of every session (~287,000 tokens for 10,000 records). 

With `larp recall --max-tokens 2000 <query>`, `.larp` scores every live record against the query, applies recency decay, and only yields the highest-ranked records that fit strictly within your token budget (e.g., 181 records fitting into 1,991 tokens). You get **99.3% fewer tokens** ingested into your model prompt.

---

### Why is `.larp` on disk only ~9% smaller than Markdown?
Per-record zstd framing adds ~41 bytes of metadata overhead per record, and per-record compression cannot exploit repetition across records. The true breakthrough is **token savings** (1,991 vs ~287,000 tokens at 10,000 records), not raw byte savings. Shared zstd dictionaries and block compression are planned enhancements documented in [BENCHMARKS.md](BENCHMARKS.md).

---

### Windows & PowerShell Troubleshooting

#### 1. UTF-8 Output Redirection
In Windows PowerShell 5.1, stdout redirection with `>` defaults to UTF-16LE. When exporting:
```powershell
# Preferred in PowerShell:
cmd /c "larp export > memory.jsonl"

# Or in PowerShell 7+:
larp export | Out-File -Encoding utf8NoBOM memory.jsonl
```

#### 2. PATH not recognized after installation
Ensure `~/.cargo/bin` or your install directory is added to your environment `PATH`. Restart your terminal or reload environment variables in PowerShell:
```powershell
$env:Path = [System.Environment]::GetEnvironmentVariable("Path","User") + ";" + [System.Environment]::GetEnvironmentVariable("Path","Machine")
```

---

### Where do I report bugs or request features?
Open an issue in `.github/ISSUE_TEMPLATE/`. For security issues, refer to [SECURITY.md](../SECURITY.md).
