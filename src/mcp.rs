//! Minimal MCP server over stdio (JSON-RPC 2.0, newline-delimited).
//!
//! Tools: `remember`, `recall`, `forget` (supersede, never delete
//! history), `list`, `stats`.
//!
//! The server caches the file's records and reloads only when the file's
//! mtime or size changed, so a call no longer re-reads and unpacks the
//! whole file every time. Its own mutations invalidate the cache, and
//! other writers are picked up via the stat check. The protocol version
//! answered is the one this server implements, not the client's echo.

use larp_memory::{
    append_record, find_duplicate, live_records, now_unix, read_file, recall, recall_tagged,
    stats_text, Record, RecordType,
};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::SystemTime;

/// The MCP protocol version this server implements.
const PROTOCOL_VERSION: &str = "2024-11-05";

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

/// Cached file state for the server process. `valid` is cleared by our own
/// mutations; the stat check catches other writers.
#[derive(Default)]
struct Cache {
    records: Vec<Record>,
    mtime: Option<SystemTime>,
    len: u64,
    valid: bool,
}

impl Cache {
    fn load(&mut self, file: &Path) -> Result<&[Record], String> {
        let meta = std::fs::metadata(file).ok();
        let (mtime, len) = match &meta {
            Some(m) => (m.modified().ok(), m.len()),
            None => (None, 0),
        };
        if self.valid && self.mtime == mtime && self.len == len {
            return Ok(&self.records);
        }
        let data = read_file(file)?;
        self.records = data.records;
        self.mtime = mtime;
        self.len = len;
        self.valid = true;
        Ok(&self.records)
    }

    fn invalidate(&mut self) {
        self.valid = false;
    }
}

fn tools_list() -> Value {
    json!({"tools": [
        {"name": "remember", "description": "Store a memory. Types: fact, preference, episode, correction. A correction with supersedes replaces that record (only corrections supersede). A near-duplicate of a live record is not stored twice; the existing id is returned instead.",
         "inputSchema": {"type": "object",
            "properties": {"text": {"type": "string"},
                           "type": {"type": "string", "enum": ["fact", "preference", "episode", "correction"]},
                           "salience": {"type": "number"},
                           "ttl_secs": {"type": "integer"},
                           "supersedes": {"type": "integer"},
                           "tags": {"type": "array", "items": {"type": "string"}},
                           "meta": {"type": "object", "additionalProperties": {"type": "string"}}},
            "required": ["text"]}},
        {"name": "recall", "description": "Recall memories fitting a token budget, best first. Each line carries score, salience, and timestamp so the caller can judge staleness. Optional tag restricts to one project/namespace.",
         "inputSchema": {"type": "object",
            "properties": {"query": {"type": "string"},
                           "max_tokens": {"type": "integer"},
                           "tag": {"type": "string"}},
            "required": []}},
        {"name": "forget", "description": "Supersede a memory by id. History is preserved, never deleted. Forgetting a correction revives the record it replaced.",
         "inputSchema": {"type": "object",
            "properties": {"id": {"type": "integer"}},
            "required": ["id"]}},
        {"name": "list", "description": "List live memories, newest first by id within the limit.",
         "inputSchema": {"type": "object",
            "properties": {"limit": {"type": "integer"}},
            "required": []}},
        {"name": "stats", "description": "File statistics: totals, live counts by type, token estimate.",
         "inputSchema": {"type": "object",
            "properties": {},
            "required": []}}
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

fn arg_tags(args: &Value) -> Vec<String> {
    args.get("tags")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn arg_meta(args: &Value) -> Vec<(String, String)> {
    args.get("meta")
        .and_then(|v| v.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn record_line(r: &Record, score: Option<f64>) -> String {
    let score_part = match score {
        Some(s) => format!(" score={s:.3}"),
        None => String::new(),
    };
    let tags_part = if r.tags.is_empty() {
        String::new()
    } else {
        format!(" [tags: {}]", r.tags.join(", "))
    };
    format!(
        "#{} [{}]{} salience={:.2} ts={} {}{}",
        r.id,
        r.rtype.name(),
        score_part,
        r.salience,
        r.timestamp,
        r.text,
        tags_part
    )
}

fn call_tool(file: &Path, cache: &mut Cache, name: &str, args: &Value) -> Value {
    match name {
        "remember" => {
            let Some(text) = arg_str(args, "text") else {
                return text_error("remember needs a text argument".to_string());
            };
            if text.trim().is_empty() {
                return text_error("remember needs non-empty text".to_string());
            }
            let rtype = arg_str(args, "type")
                .and_then(|t| RecordType::parse(&t))
                .unwrap_or(RecordType::Fact);
            let salience = arg_f64(args, "salience").unwrap_or(0.5) as f32;
            if !salience.is_finite() {
                return text_error("salience must be finite".to_string());
            }
            let ttl = arg_i64(args, "ttl_secs").unwrap_or(0);
            let supersedes = arg_u64(args, "supersedes").unwrap_or(0);
            let tags = arg_tags(args);
            let meta = arg_meta(args);
            // One cached read serves both checks: no duplicate storage,
            // and a supersedes target must actually exist.
            let records = match cache.load(file) {
                Ok(r) => r,
                Err(e) => return text_error(e),
            };
            if let Some(id) = find_duplicate(records, rtype, &text, now_unix()) {
                return text_content(format!("already stored as #{id}"));
            }
            if supersedes != 0 && !records.iter().any(|r| r.id == supersedes) {
                return text_error(format!("no such record #{supersedes}"));
            }
            let mut record = Record::new(rtype, salience, ttl, text);
            record.supersedes = supersedes;
            record.tags = tags;
            record.meta = meta;
            match append_record(file, record) {
                Ok(id) => {
                    cache.invalidate();
                    text_content(format!("stored #{id}"))
                }
                Err(e) => text_error(e),
            }
        }
        "recall" => {
            let query = arg_str(args, "query").unwrap_or_default();
            let max_tokens = arg_u64(args, "max_tokens").unwrap_or(2000) as usize;
            let tag = arg_str(args, "tag");
            let records = match cache.load(file) {
                Ok(r) => r,
                Err(e) => return text_error(e),
            };
            let (picked, used) = match tag.as_deref() {
                Some(t) => recall_tagged(records, &query, max_tokens, t, now_unix()),
                None => recall(records, &query, max_tokens, now_unix()),
            };
            if picked.is_empty() {
                return text_content("no memories found".to_string());
            }
            let mut out = String::new();
            for s in &picked {
                out.push_str(&record_line(s.record, Some(s.score)));
                out.push('\n');
            }
            out.push_str(&format!("tokens used: {used} / {max_tokens}"));
            text_content(out)
        }
        "forget" => {
            let Some(id) = arg_u64(args, "id") else {
                return text_error("forget needs an id argument".to_string());
            };
            let exists = match cache.load(file) {
                Ok(records) => records.iter().any(|r| r.id == id),
                Err(e) => return text_error(e),
            };
            if !exists {
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
                Ok(_) => {
                    cache.invalidate();
                    text_content(format!("superseded #{id}"))
                }
                Err(e) => text_error(e),
            }
        }
        "list" => {
            let limit = arg_u64(args, "limit").unwrap_or(100) as usize;
            let records = match cache.load(file) {
                Ok(r) => r,
                Err(e) => return text_error(e),
            };
            let live = live_records(records, now_unix());
            if live.is_empty() {
                return text_content("no memories found".to_string());
            }
            let mut out = String::new();
            for r in live.iter().take(limit) {
                out.push_str(&record_line(r, None));
                out.push('\n');
            }
            text_content(out.trim_end().to_string())
        }
        "stats" => {
            let records = match cache.load(file) {
                Ok(r) => r,
                Err(e) => return text_error(e),
            };
            let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
            text_content(format!(
                "file: {}\n{}",
                file.display(),
                stats_text(records, size, now_unix())
            ))
        }
        other => text_error(format!("unknown tool: {other}")),
    }
}

/// Handle one JSON-RPC message. Returns None for notifications (no reply).
fn handle(file: &Path, cache: &mut Cache, line: &str) -> Option<String> {
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
        "initialize" => Some(result(
            &id,
            json!({"protocolVersion": PROTOCOL_VERSION,
                   "capabilities": {"tools": {}},
                   "serverInfo": {"name": "larp", "version": env!("CARGO_PKG_VERSION")}}),
        )),
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
            Some(result(&id, call_tool(file, cache, &name, &args)))
        }
        _ => Some(error(&id, -32601, "method not found")),
    }
}

pub fn run(file: &Path) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut cache = Cache::default();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(resp) = handle(file, &mut cache, &line) {
            writeln!(stdout, "{resp}").map_err(|e| e.to_string())?;
            stdout.flush().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
