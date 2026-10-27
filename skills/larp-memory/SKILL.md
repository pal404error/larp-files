---
name: larp-memory
description: Give the agent durable memory in a single .larp file — remember facts, preferences, and episodes across sessions, and recall only the memories that fit a token budget instead of re-reading a whole file. Use when starting a session that should remember earlier work, when the user says "remember this", or when a task needs context from past sessions. Works with any MCP client; no server, no database, no API key.
---

# larp-memory skill

## When to use this

- At the start of a session, recall relevant memories before answering.
- When the user states a durable fact or preference ("remember this",
  "I prefer..."), store it with `remember`.
- When a fact changes, supersede it — never silently rewrite history.

## Setup

Install the binary (or build from source with `cargo build --release`):

```bash
curl -fsSL https://raw.githubusercontent.com/pal404error/larp-files/main/install.sh | sh
```

Point your MCP client at it (see `docs/MCP_CLIENTS.md` for Claude Code,
Cursor, VS Code, Codex, and opencode configs). The server command is
`larp`, with args `["mcp", "--file", "<path-to-memory.larp>"]`.

## Tools (via MCP)

- `remember` — store a memory. Arguments: `text` (required), `type`
  (`fact` | `preference` | `episode`, default `fact`), `salience`
  (0–1, default 0.5), `ttl_secs` (0 = never expires).
- `recall` — retrieve memories. Arguments: `query`, `max_tokens`
  (default 2000). Returns the best memories that fit the budget, ranked
  by keyword match x salience x recency decay.
- `forget` — supersede a memory by id. The old record stays in the file
  for audit; it just stops being live.

## CLI equivalents

```bash
larp append --type fact --salience 0.9 "the user prefers rust"
larp recall --max-tokens 2000 rust
larp stats        # counts, size, live vs superseded, token estimate
larp compact      # drop expired/superseded records
```

## Rules

- Recall before assuming: if a question touches past sessions, call
  `recall` first.
- Prefer high salience (0.8+) for facts that change future decisions;
  keep routine notes at 0.5 or below.
- Never store secrets, tokens, or credentials as memories.
