# Why agent memory needs a file format

Every coding agent today has the same quiet problem: it forgets everything
between sessions, or it remembers everything at full price.

## The two bad defaults

**Flat Markdown files.** The agent keeps a `MEMORY.md` and re-reads the whole
thing every session. That works for ten memories. At a thousand, you are
paying thousands of tokens before the agent has done a single useful thing —
and the cost grows forever, because nothing ever gets forgotten, ranked, or
expired.

**Vector databases.** Powerful, but now your "memory" needs a server, an
embedding model, an API key, and setup. For a personal agent, a side project,
or a CLI tool, that is a skyscraper where a filing cabinet would do.

## The missing middle

`.larp` is a single file that sits between those two defaults:

- **One file, no server.** Copy it, email it, commit it to git. Your agent's
  memory travels with the project.
- **Token-budget recall.** Ask for memories that fit 2,000 tokens and get
  the *best* ones — ranked by keyword match x salience x recency decay —
  instead of a blind dump.
- **Corrections, not rewrites.** When a fact changes, the agent appends a
  correction that supersedes the old record. History stays traceable, and
  the latest truth always wins.
- **Crash-safe.** CRC-checked append-only records mean a killed process can
  only tear the tail. Earlier memories are never corrupted.
- **An MCP server.** `larp mcp` gives any agent three tools — `remember`,
  `recall`, `forget` — so memory works across sessions and across agents.

## Numbers, not adjectives

On 10,000 memories: recall at a 2,000-token budget serves the 181 best
memories in 1,991 tokens, versus ~287,000 tokens for the full file — 99.3%
fewer tokens, at sub-millisecond latency. Full method and the honest
limitations (per-record compression is modest on short records) are in
[docs/BENCHMARKS.md](BENCHMARKS.md).

## Try it in 60 seconds

```bash
cargo install --path .   # or: cargo install larp (once on crates.io)
larp append --type fact --salience 0.9 "the user prefers rust"
larp recall --max-tokens 2000 rust
```

Then point your agent at it over MCP and watch session two remember
session one. If the format's tradeoffs look wrong to you, the spec is
[docs/FORMAT.md](FORMAT.md) and the issue tracker is open — the fastest way
to make this better is to tell us what your agent actually needs.
