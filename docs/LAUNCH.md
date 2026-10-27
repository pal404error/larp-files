# Launch plan — .larp v1.0

## Tagline options (pick one)

1. "Agent memory that fits in a file, not a database."
2. "Markdown memory files waste tokens. Vector DBs need a server. .larp is
   the file format in between."
3. "Give your agent a memory file: small on disk, cheap to recall."

## Demo (60 seconds, record this first)

1. Agent without .larp: starts a second session, forgets everything.
2. Same agent with .larp via MCP: `remember` in session one, `recall` in
   session two — it remembers.
3. Flash the benchmark: 181 best memories in 1,991 tokens vs ~287,000 for
   the full file.

## Show HN draft

Title: "Show HN: .larp — a file format for agent memory"

Body: "Agents today store memory as flat Markdown (re-read in full every
session, so tokens and cost grow forever) or in a vector DB (a server,
an embedding model, setup). .larp is the middle ground: one portable file
with CRC-checked append-only records, corrections that supersede instead
of silently rewriting, expiry, and token-budget recall ranked by keyword
match x salience x recency decay. It's a Rust crate + CLI with an MCP
server, so any agent can use it as its memory. Format v2, MIT licensed.
Benchmarks and the full spec are in the repo — happy to answer questions
about the format tradeoffs."

## r/rust draft

Title: ".larp 1.0 — agent memory as a single file (Rust, MCP, zstd)"

Body: short version of the above + "What I'd love feedback on: the recall
ranking formula and whether per-record zstd or block compression is the
right next step — benchmarks show per-record framing limits size wins on
short records, and I documented that honestly in docs/BENCHMARKS.md."

## Product Hunt

Needs: tagline (above), 3 screenshots (CLI demo, recall output, benchmark
table), first comment = the Show HN body, shortened.

## Launch sequence

- Week before: blog post "Why agent memory needs a file format" + demo
  video on X/LinkedIn; 5 developer friends review the README.
- Launch day: Show HN in the morning, r/rust + agent communities after,
  Product Hunt listing (prepare tagline + screenshots the day before).
- Week after: answer every issue/PR within 24h; ship v1.0.1 with community
  fixes; post a "what I learned building .larp" follow-up.
- Ongoing: monthly benchmark updates, changelog post per release.

## What NOT to do

- Don't launch before the recall demo works — the demo IS the story.
- Don't buy stars or spam communities; one good Show HN beats ten
  low-effort posts.
- Don't promise a roadmap you can't ship; under-promise, over-deliver.
- Keep the format itself open forever — that is the trust moat. A paid
  tier (hosted sync, team memory server) only comes after real demand.
