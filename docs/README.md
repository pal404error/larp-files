# Documentation Hub

Welcome to the `.larp` developer documentation. Explore the technical specifications, benchmark breakdowns, client setup guides, and format internals below.

---

## Table of Contents

| Document | Description |
|---|---|
| [**FAQ.md**](FAQ.md) | Answers to common questions, ranking math, offline usage, and Windows tips. |
| [**FORMAT.md**](FORMAT.md) | Formal specification of the binary header, CRC32-checked records, and v2 zstd compression. |
| [**BENCHMARKS.md**](BENCHMARKS.md) | Real-world benchmark analysis on 10,000 memories (tokens, latencies, compression). |
| [**MCP_CLIENTS.md**](MCP_CLIENTS.md) | Copy-paste JSON configurations for Claude Code, Cursor, VS Code, Codex, and OpenCode. |
| [**COMPARISON.md**](COMPARISON.md) | Deep architectural comparison: `.larp` vs Markdown vs Vector DBs vs SQLite. |
| [**DEMO.md**](DEMO.md) | Step-by-step transcript demonstrating multi-session continuity and memory superseding. |
| [**BLOG.md**](BLOG.md) | Deep-dive article: *"Why agent memory needs a file format, not another server."* |
| [**LAUNCH.md**](LAUNCH.md) | Launch strategy, target audiences, and community release guidelines. |
| [**LAUNCH_KIT.md**](LAUNCH_KIT.md) | Ready-to-use Show HN, Reddit, and Product Hunt drafts + 5 directory PRs. |
| [**OUTREACH.md**](OUTREACH.md) | Directory and community submission drafts (for maintainers). |
| [**SKILL.md**](../skills/larp-memory/SKILL.md) | Agent skill definition compliant with the Agent Skills standard. |
| [**CHANGELOG.md**](../CHANGELOG.md) | Full release history and upcoming roadmap. |

---

## Quick Navigation by Role

- **Agent Authors & Developers:** Start with [MCP_CLIENTS.md](MCP_CLIENTS.md) and [FAQ.md](FAQ.md).
- **Core Contributors & Systems Hackers:** Read [FORMAT.md](FORMAT.md) and [BENCHMARKS.md](BENCHMARKS.md).
- **Tooling & Integrators:** Check out [COMPARISON.md](COMPARISON.md) and the reference client in [`examples/larp_mcp_client.py`](../examples/larp_mcp_client.py).
