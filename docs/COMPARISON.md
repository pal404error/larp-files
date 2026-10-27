# How .larp compares

The question every agent-memory tool has to answer: what does the agent
pay, per session, to remember? The table below is qualitative except for
the `.larp` column, which is measured (10,000 records, see
`docs/BENCHMARKS.md`). Competitor behavior is described at the level of
their architecture, not cherry-picked benchmarks.

| | Markdown file | SQLite | Vector DB | Hosted memory service | .larp |
|---|---|---|---|---|---|
| Setup | None | A schema | Server + embeddings + API key | Account + API key | One binary |
| Server needed | No | No | Yes | Yes (theirs) | No |
| Recall cost at 10k memories | Full-file read (~287k tokens) | Query, then you rank | Embedding + query | API call + query | 1,991 tokens at a 2,000 budget |
| Search | grep | SQL `LIKE` / FTS | Semantic | Semantic | Keyword x salience x recency |
| History / audit | Git, if committed | If you build it | Usually not | Varies | Built in: corrections supersede, never delete |
| Portability | Copy the file | Copy the file | Export tooling | Export API | Copy the file |
| Works offline | Yes | Yes | Self-hosted only | No | Yes |
| Best for | A dozen notes | Structured app data | Semantic search at scale | Managed convenience | Agent sessions on one machine |

## Honest limits

- `.larp` search is keyword-based, not semantic: it will not find a
  memory phrased completely differently from your query. If you need
  semantic recall, pair it with embeddings or use a vector store.
- Per-record compression is modest on short records (91.0% of Markdown
  size at 10k records). The win is tokens at recall time, not bytes on
  disk — see `docs/BENCHMARKS.md` for the full picture.
- Single-writer at a time: two agents appending to the same file
  concurrently can interleave. One agent (or one file per agent) is the
  intended shape.
