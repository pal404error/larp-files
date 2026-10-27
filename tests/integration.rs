use larp::{
    append_record, compact, crc32, export_jsonl, import_jsonl, live_records, now_unix, read_file,
    recall, search, Record, RecordType,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn tmpfile(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("larp-test-{name}-{}.larp", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

fn rec(text: &str, salience: f32) -> Record {
    Record::new(RecordType::Fact, salience, 0, text.to_string())
}

/// Build a legacy v1 file (raw, uncompressed payloads) by hand.
fn write_v1_file(path: &PathBuf, texts: &[&str]) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"LARP");
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&[0u8, 0u8]);
    for (i, text) in texts.iter().enumerate() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&(i as u64 + 1).to_le_bytes());
        payload.push(0u8); // fact
        payload.extend_from_slice(&now_unix().to_le_bytes());
        payload.extend_from_slice(&0.9f32.to_le_bytes());
        payload.extend_from_slice(&0i64.to_le_bytes());
        payload.extend_from_slice(&0u64.to_le_bytes());
        payload.extend_from_slice(&(text.len() as u32).to_le_bytes());
        payload.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&crc32(&payload).to_le_bytes());
        bytes.extend_from_slice(&payload);
    }
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn append_list_roundtrip() {
    let f = tmpfile("roundtrip");
    append_record(&f, rec("the user prefers rust", 0.9)).unwrap();
    append_record(&f, rec("the user lives in ottawa", 0.7)).unwrap();
    let data = read_file(&f).unwrap();
    assert!(!data.torn);
    assert_eq!(data.version, 2);
    assert_eq!(data.records.len(), 2);
    assert_eq!(data.records[0].id, 1);
    assert_eq!(data.records[1].id, 2);
    assert_eq!(data.records[0].text, "the user prefers rust");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn torn_tail_is_ignored() {
    let f = tmpfile("torn");
    append_record(&f, rec("first memory", 0.9)).unwrap();
    append_record(&f, rec("second memory", 0.9)).unwrap();
    append_record(&f, rec("third memory", 0.9)).unwrap();
    // Simulate a kill mid-write: chop bytes off the end.
    let len = std::fs::metadata(&f).unwrap().len();
    let file = OpenOptions::new().write(true).open(&f).unwrap();
    file.set_len(len - 5).unwrap();
    let data = read_file(&f).unwrap();
    assert!(data.torn);
    assert_eq!(data.records.len(), 2);
    assert_eq!(data.records[1].text, "second memory");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn corrupt_record_is_ignored() {
    let f = tmpfile("corrupt");
    append_record(&f, rec("good memory", 0.9)).unwrap();
    append_record(&f, rec("doomed memory", 0.9)).unwrap();
    let mut bytes = std::fs::read(&f).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF; // flip a payload byte -> CRC mismatch
    std::fs::write(&f, bytes).unwrap();
    let data = read_file(&f).unwrap();
    assert!(data.torn);
    assert_eq!(data.records.len(), 1);
    let _ = std::fs::remove_file(&f);
}

#[test]
fn correction_supersedes() {
    let f = tmpfile("correction");
    append_record(&f, rec("the user lives in toronto", 0.9)).unwrap();
    let mut fix = Record::new(
        RecordType::Correction,
        1.0,
        0,
        "the user lives in ottawa".to_string(),
    );
    fix.supersedes = 1;
    append_record(&f, fix).unwrap();
    let data = read_file(&f).unwrap();
    let live = live_records(&data.records, now_unix());
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].text, "the user lives in ottawa");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn recall_respects_token_budget() {
    let f = tmpfile("recall");
    for i in 0..10 {
        append_record(
            &f,
            rec(&format!("memory number {i} about rust programming"), 0.9),
        )
        .unwrap();
    }
    let data = read_file(&f).unwrap();
    let (picked, used) = recall(&data.records, "rust", 20, now_unix());
    assert!(used <= 20);
    assert!(!picked.is_empty());
    let _ = std::fs::remove_file(&f);
}

#[test]
fn search_ranks_matching_first() {
    let f = tmpfile("search");
    append_record(&f, rec("i love gardening", 0.5)).unwrap();
    append_record(&f, rec("rust is my favourite language", 0.5)).unwrap();
    let data = read_file(&f).unwrap();
    let hits = search(&data.records, "rust language", now_unix());
    assert_eq!(hits.len(), 1);
    assert!(hits[0].record.text.contains("rust"));
    let _ = std::fs::remove_file(&f);
}

#[test]
fn compact_drops_superseded() {
    let f = tmpfile("compact");
    append_record(&f, rec("old fact", 0.9)).unwrap();
    let mut fix = Record::new(RecordType::Correction, 1.0, 0, "new fact".to_string());
    fix.supersedes = 1;
    append_record(&f, fix).unwrap();
    let kept = compact(&f).unwrap();
    assert_eq!(kept, 1);
    let data = read_file(&f).unwrap();
    assert_eq!(data.records[0].text, "new fact");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn v1_file_reads() {
    let f = tmpfile("v1read");
    write_v1_file(&f, &["legacy memory one", "legacy memory two"]);
    let data = read_file(&f).unwrap();
    assert_eq!(data.version, 1);
    assert!(!data.torn);
    assert_eq!(data.records.len(), 2);
    assert_eq!(data.records[1].text, "legacy memory two");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn append_migrates_v1_to_v2() {
    let f = tmpfile("v1migrate");
    write_v1_file(&f, &["legacy memory one"]);
    let id = append_record(&f, rec("brand new memory", 0.8)).unwrap();
    assert_eq!(id, 2);
    let data = read_file(&f).unwrap();
    assert_eq!(data.version, 2);
    assert_eq!(data.records.len(), 2);
    assert_eq!(data.records[0].text, "legacy memory one");
    assert_eq!(data.records[1].text, "brand new memory");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn jsonl_roundtrip() {
    let f = tmpfile("jsonl");
    append_record(&f, rec("json memory one", 0.8)).unwrap();
    append_record(&f, rec("json memory two", 0.6)).unwrap();
    let data = read_file(&f).unwrap();
    let jsonl = export_jsonl(&data.records).unwrap();
    let g = tmpfile("jsonl2");
    let ids = import_jsonl(&g, &jsonl).unwrap();
    assert_eq!(ids, vec![1, 2]);
    let data2 = read_file(&g).unwrap();
    assert_eq!(data2.records.len(), 2);
    assert_eq!(data2.records[0].text, "json memory one");
    assert!((data2.records[0].salience - 0.8).abs() < f32::EPSILON);
    let _ = std::fs::remove_file(&f);
    let _ = std::fs::remove_file(&g);
}

#[test]
fn jsonl_import_remaps_collisions() {
    let f = tmpfile("jsonlcol");
    append_record(&f, rec("original", 0.5)).unwrap();
    let data = read_file(&f).unwrap();
    let jsonl = export_jsonl(&data.records).unwrap();
    let ids = import_jsonl(&f, &jsonl).unwrap();
    assert_eq!(ids, vec![2]); // id 1 collided, reassigned
    let data2 = read_file(&f).unwrap();
    assert_eq!(data2.records.len(), 2);
    assert_eq!(data2.records[1].text, "original");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn mcp_end_to_end() {
    let f = tmpfile("mcp");
    let bin = env!("CARGO_BIN_EXE_larp");
    let mut child = Command::new(bin)
        .arg("--file")
        .arg(&f)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"0\"}}}\n",
        "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"remember\",\"arguments\":{\"text\":\"mcp test memory about rust\",\"type\":\"fact\",\"salience\":0.9}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"recall\",\"arguments\":{\"query\":\"rust\",\"max_tokens\":500}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"forget\",\"arguments\":{\"id\":1}}}\n",
    );
    // Writing then dropping stdin closes the pipe, so the server exits.
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    // Notifications get no reply: exactly 5 responses, ids 1..5.
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[0]["result"]["serverInfo"]["name"], "larp");
    let tools = lines[1]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"remember"));
    assert!(names.contains(&"recall"));
    assert!(names.contains(&"forget"));
    assert!(lines[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("stored #1"));
    assert!(lines[3]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("mcp test memory about rust"));
    assert!(lines[4]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("superseded #1"));
    let data = read_file(&f).unwrap();
    assert!(data
        .records
        .iter()
        .any(|r| r.rtype == RecordType::Correction && r.supersedes == 1));
    // The correction record itself is live; the forgotten memory is not.
    let live = live_records(&data.records, now_unix());
    assert_eq!(live.len(), 1);
    assert!(live.iter().all(|r| r.id != 1));
    assert!(live
        .iter()
        .all(|r| !r.text.contains("mcp test memory about rust")));
    let _ = std::fs::remove_file(&f);
}

#[test]
fn mcp_malformed_input() {
    let f = tmpfile("mcp-bad");
    let bin = env!("CARGO_BIN_EXE_larp");
    let mut child = Command::new(bin)
        .arg("--file")
        .arg(&f)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let input = concat!(
        "this is not json\n",
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2024-11-05\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"0\"}}}\n",
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["error"]["code"], -32700);
    assert_eq!(lines[1]["result"]["serverInfo"]["name"], "larp");
    let _ = std::fs::remove_file(&f);
}
