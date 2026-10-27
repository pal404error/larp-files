# Outreach drafts — NOT SENT

The human submits these. Verify each site's current submission process
before sending; directories change their rules.

## 1. Glama (MCP directory)

Submission blurb: ".larp — agent memory in a single file. A Rust MCP
server with three tools (remember, recall, forget) over stdio. No
server, no database, no API key. Token-budget recall: 181 best memories
in 1,991 tokens at 10,000 records. MIT. Repo:
https://github.com/pal404error/larp-files"

## 2. Smithery (MCP registry)

Same blurb as Glama, plus: install via the release binary or
`cargo install larp-memory` (once published); server command `larp`,
args `["mcp", "--file", "<path>"]`.

## 3. mcp.so (MCP directory)

Same blurb as Glama. Category: memory / developer tools.

## 4. awesome-mcp-servers (community list)

PR description: "Add .larp: single-file agent memory over MCP.
remember/recall/forget tools, token-budget recall, local-first, Rust,
MIT." Include the repo link and the one-line install.

## 5. Agent skills collections

Wherever skills are collected for Claude Code / Cursor / Codex, submit
`skills/larp-memory/SKILL.md` with: "Teaches an agent durable memory in
a .larp file: recall before answering, remember durable facts, supersede
instead of rewriting. Works with any MCP client."

## Follow-up features worth their own announcement (ranked)

1. **Shared zstd dictionary + block compression** (medium effort, big
   numbers story): closes the honest size gap documented in
   BENCHMARKS.md. Announcement: "now X% smaller than Markdown."
2. **Multi-file memory workspaces** (low-medium effort, broad appeal):
   one memory file per project, `larp` picks by directory. Announcement:
   "memory that follows the project."
3. **Optional semantic recall plugin** (high effort, high attention):
   an opt-in embedding sidecar for users who outgrow keyword search,
   keeping the core offline and dependency-light. Announcement: "semantic
   recall, still one file."
