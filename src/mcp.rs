//! Minimal MCP server over stdio (JSON-RPC 2.0, newline-delimited).
//!
//! Tools: `remember`, `recall`, `forget` (supersede, never delete history).
//! Each call re-reads the memory file, so state is always fresh. The loop
//! ends on stdin EOF, which is also how tests drive it.

use larp_memory::{append_record, now_unix, read_file, recall, Record, RecordType};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;

fn result(id: &Value, value: Value) -> String {
    serde_json::to_string(&json!({"jsonrpc": "2.0", "id": id, "result": value})).unwrap()
}

fn error(id: &Value, code: i64, message: &str) -> String {
    serde_json::to_string(
        &json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
    )
    .unwrap()
}

fn text_content(text: String) -> Value {
    json!({"content": [{"type": "text", "text": text}]})
}

fn text_error(text: String) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": true})
}

fn tools_list() -> Value {
    json!({"tools": [
        {"name": "remember", "description": "Store a memory. Types: fact, preference, episode, correction.",
         "inputSchema": {"type": "object",
            "properties": {"text": {"type": "string"},
                           "type": {"type": "string", "enum": ["fact", "preference", "episode", "correction"]},
                           "salience": {"type": "number"},
                           "ttl_secs": {"type": "integer"}},
            "required": ["text"]}},
        {"name": "recall", "description": "Recall memories fitting a token budget, best first.",
         "inputSchema": {"type": "object",
            "properties": {"query": {"type": "string"},
                           "max_tokens": {"type": "integer"}},
            "required": []}},
        {"name": "forget", "description": "Supersede a memory by id. History is preserved, never deleted.",
         "inputSchema": {"type": "object",
            "properties": {"id": {"type": "integer"}},
            "required": ["id"]}}
    ]})
}

fn arg_str(args: &Value, name: &str) -> Option<String> {
    args.get(name)?.as_str().map(|s| s.to_string())
}

fn arg_f64(args: &Value, name: &str) -> Option<f64> {
    args.get(name)?.as_f64()
}

fn arg_i64(args: &Value, name: &str) -> Option<i64> {
    args.get(name)?.as_i64()
}

fn arg_u64(args: &Value, name: &str) -> Option<u64> {
    args.get(name)?.as_u64()
}

fn call_tool(file: &Path, name: &str, args: &Value) -> Value {
    match name {
        "remember" => {
            let Some(text) = arg_str(args, "text") else {
                return text_error("remember needs a text argument".to_string());
            };
            let rtype = arg_str(args, "type")
                .and_then(|t| RecordType::parse(&t))
                .unwrap_or(RecordType::Fact);
            let salience = arg_f64(args, "salience").unwrap_or(0.5) as f32;
            let ttl = arg_i64(args, "ttl_secs").unwrap_or(0);
            match append_record(file, Record::new(rtype, salience, ttl, text)) {
                Ok(id) => text_content(format!("stored #{id}")),
                Err(e) => text_error(e),
            }
        }
        "recall" => {
            let query = arg_str(args, "query").unwrap_or_default();
            let max_tokens = arg_u64(args, "max_tokens").unwrap_or(2000) as usize;
            let data = match read_file(file) {
                Ok(d) => d,
                Err(e) => return text_error(e),
            };
            let (picked, used) = recall(&data.records, &query, max_tokens, now_unix());
            if picked.is_empty() {
                return text_content("no memories found".to_string());
            }
            let mut out = String::new();
            for s in &picked {
                out.push_str(&format!(
                    "#{} [{}] {}\n",
                    s.record.id,
                    s.record.rtype.name(),
                    s.record.text
                ));
            }
            out.push_str(&format!("tokens used: {used} / {max_tokens}"));
            text_content(out)
        }
        "forget" => {
            let Some(id) = arg_u64(args, "id") else {
                return text_error("forget needs an id argument".to_string());
            };
            let data = match read_file(file) {
                Ok(d) => d,
                Err(e) => return text_error(e),
            };
            if !data.records.iter().any(|r| r.id == id) {
                return text_error(format!("no such record #{id}"));
            }
            let mut fix = Record::new(
                RecordType::Correction,
                1.0,
                0,
                "forgotten via mcp".to_string(),
            );
            fix.supersedes = id;
            match append_record(file, fix) {
                Ok(_) => text_content(format!("superseded #{id}")),
                Err(e) => text_error(e),
            }
        }
        other => text_error(format!("unknown tool: {other}")),
    }
}

/// Handle one JSON-RPC message. Returns None for notifications (no reply).
fn handle(file: &Path, line: &str) -> Option<String> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Some(error(&Value::Null, -32700, "parse error")),
    };
    let method = msg.get("method")?.as_str()?.to_string();
    if method.starts_with("notifications/") {
        return None;
    }
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    match method.as_str() {
        "initialize" => {
            let version = msg
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or("2024-11-05")
                .to_string();
            Some(result(
                &id,
                json!({"protocolVersion": version,
                       "capabilities": {"tools": {}},
                       "serverInfo": {"name": "larp", "version": env!("CARGO_PKG_VERSION")}}),
            ))
        }
        "ping" => Some(result(&id, json!({}))),
        "tools/list" => Some(result(&id, tools_list())),
        "tools/call" => {
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Some(result(&id, call_tool(file, &name, &args)))
        }
        _ => Some(error(&id, -32601, "method not found")),
    }
}

pub fn run(file: &Path) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle(file, &line) {
            writeln!(stdout, "{resp}").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
