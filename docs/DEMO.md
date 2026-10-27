# Demo: an agent that remembers across sessions

## CLI demo (30 seconds)

```bash
larp append --type fact --salience 0.9 "the user prefers rust"
# appended #1
larp append --type preference "likes short answers"
# appended #2
larp recall --max-tokens 2000 rust
# #1 score=0.900 [fact] the user prefers rust
# tokens used: 6 / 2000
```

## Two-session MCP demo (the story that sells it)

Session 1 — the agent stores what it learns:

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools/call",
 "params": {"name": "remember",
            "arguments": {"text": "the user prefers rust", "type": "fact", "salience": 0.9}}}
```

Response: `stored #1`.

Session 2 — a fresh agent, same memory file, recalls before answering:

```json
{"jsonrpc": "2.0", "id": 2, "method": "tools/call",
 "params": {"name": "recall",
            "arguments": {"query": "rust", "max_tokens": 2000}}}
```

Response: the stored memory, ranked first — no re-reading a whole file,
no server, no embeddings. When a fact changes, `forget` supersedes it
instead of deleting history:

```json
{"jsonrpc": "2.0", "id": 3, "method": "tools/call",
 "params": {"name": "forget", "arguments": {"id": 1}}}
```

Response: `superseded #1` — and the old record is still in the file for
audit, just no longer live.

## One-liner for talks and posts

> "Markdown memory files waste tokens. Vector DBs need a server. .larp is
> the file format in between — 181 best memories in 1,991 tokens instead
> of ~287,000 for the full file."
