#!/usr/bin/env python3
"""Minimal stdlib-only MCP client for `larp mcp`.

Demonstrates remember/recall/forget over JSON-RPC 2.0 on stdio, so any
Python agent can use a .larp memory file without extra dependencies.
Requires the `larp` binary on PATH (see install.sh, or cargo install).

Run: python3 examples/larp_mcp_client.py
"""

import json
import subprocess
import sys
import tempfile
from pathlib import Path


class LarpMCP:
    def __init__(self, memory_file):
        self.proc = subprocess.Popen(
            ["larp", "mcp", "--file", str(memory_file)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
        )
        self._id = 0
        self._call("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "larp-python-demo", "version": "0"},
        })

    def _call(self, method, params=None):
        self._id += 1
        req = {"jsonrpc": "2.0", "id": self._id, "method": method}
        if params is not None:
            req["params"] = params
        self.proc.stdin.write(json.dumps(req) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            raise RuntimeError("larp mcp closed the connection")
        resp = json.loads(line)
        if "error" in resp:
            raise RuntimeError(f"MCP error: {resp['error']}")
        return resp.get("result")

    def tool(self, name, arguments):
        result = self._call("tools/call", {"name": name, "arguments": arguments})
        parts = result.get("content", [])
        return "\n".join(p.get("text", "") for p in parts)

    def remember(self, text, type="fact", salience=0.5, ttl_secs=0):
        return self.tool("remember", {
            "text": text, "type": type,
            "salience": salience, "ttl_secs": ttl_secs,
        })

    def recall(self, query, max_tokens=2000):
        return self.tool("recall", {"query": query, "max_tokens": max_tokens})

    def forget(self, record_id):
        return self.tool("forget", {"id": record_id})

    def close(self):
        try:
            self.proc.stdin.close()
        except BrokenPipeError:
            pass
        self.proc.wait(timeout=5)


def main():
    tmp = Path(tempfile.mkdtemp()) / "demo.larp"
    client = LarpMCP(tmp)
    try:
        print(client.remember("the user prefers rust", salience=0.9))
        print(client.remember("likes short answers", type="preference"))
        print("--- recall 'rust' ---")
        print(client.recall("rust"))
        print("--- forget #1, recall again ---")
        print(client.forget(1))
        print(client.recall("rust"))
    finally:
        client.close()


if __name__ == "__main__":
    sys.exit(main())
