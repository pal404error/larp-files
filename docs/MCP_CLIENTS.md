# Connecting your agent to .larp over MCP

`larp mcp` speaks JSON-RPC 2.0 over stdio. Every config below runs the
same thing: command `larp`, args `["mcp", "--file", "<path>"]`. If you
omit `--file`, the server uses `memory.larp` in the working directory.
Replace `/path/to/memory.larp` with your file.

## Claude Code

CLI:

```bash
claude mcp add larp-memory -- larp mcp --file /path/to/memory.larp
```

Or `.mcp.json` in your project:

```json
{
  "mcpServers": {
    "larp-memory": {
      "command": "larp",
      "args": ["mcp", "--file", "/path/to/memory.larp"]
    }
  }
}
```

## Cursor

`.cursor/mcp.json`:

```json
{
  "mcpServers": {
    "larp-memory": {
      "command": "larp",
      "args": ["mcp", "--file", "/path/to/memory.larp"]
    }
  }
}
```

## VS Code (Copilot)

`.vscode/mcp.json`:

```json
{
  "servers": {
    "larp-memory": {
      "command": "larp",
      "args": ["mcp", "--file", "/path/to/memory.larp"]
    }
  }
}
```

## Codex

`~/.codex/config.toml`:

```toml
[mcp_servers.larp-memory]
command = "larp"
args = ["mcp", "--file", "/path/to/memory.larp"]
```

## opencode

`opencode.json`:

```json
{
  "mcp": {
    "larp-memory": {
      "type": "local",
      "command": ["larp", "mcp", "--file", "/path/to/memory.larp"]
    }
  }
}
```

## Any other MCP client

Any stdio MCP client works: command `larp`, args
`["mcp", "--file", "<path>"]`. Tools exposed: `remember`, `recall`,
`forget`. See `docs/DEMO.md` for a two-session example, and
`skills/larp-memory/SKILL.md` for the agent skill that teaches an agent
when to use them.
