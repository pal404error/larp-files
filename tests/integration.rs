use larp_memory::{
    append_record, compact, crc32, export_jsonl, import_jsonl, live_records, now_unix, read_file,
    recall, recall_tagged, search, search_tagged, Record, RecordType,
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
    assert_eq!(data.version, 3);
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
fn append_migrates_v1_to_v3() {
    let f = tmpfile("v1migrate");
    write_v1_file(&f, &["legacy memory one"]);
    let id = append_record(&f, rec("brand new memory", 0.8)).unwrap();
    assert_eq!(id, 2);
    let data = read_file(&f).unwrap();
    assert_eq!(data.version, 3);
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

#[test]
fn damage_then_append_recovers() {
    // Bug 1: a crash-damaged tail used to make every later append
    // silently unreachable (same id twice, nothing readable past the
    // damage). Appends now truncate to the last good frame first.
    let f = tmpfile("damage-append");
    append_record(&f, rec("first memory", 0.9)).unwrap();
    append_record(&f, rec("second memory", 0.9)).unwrap();
    let mut bytes = std::fs::read(&f).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF; // damage the last frame -> CRC mismatch
    std::fs::write(&f, bytes).unwrap();
    let data = read_file(&f).unwrap();
    assert!(data.torn);
    assert_eq!(data.records.len(), 1);
    let id3 = append_record(&f, rec("third memory", 0.9)).unwrap();
    assert_eq!(id3, 2);
    let id4 = append_record(&f, rec("fourth memory", 0.9)).unwrap();
    assert_eq!(id4, 3);
    let data = read_file(&f).unwrap();
    assert!(!data.torn);
    assert_eq!(data.records.len(), 3);
    assert_eq!(data.records[0].text, "first memory");
    assert_eq!(data.records[1].text, "third memory");
    assert_eq!(data.records[2].text, "fourth memory");
    assert_eq!(data.records[1].id, 2);
    assert_eq!(data.records[2].id, 3);
    let _ = std::fs::remove_file(&f);
}

#[test]
fn two_writers_at_once() {
    // Bug 4: two writers used to get duplicate ids and lose appends.
    // The lock serializes them; every append lands exactly once.
    let f = tmpfile("two-writers");
    std::thread::scope(|s| {
        for t in 0..4 {
            let f = &f;
            s.spawn(move || {
                for i in 0..25 {
                    append_record(f, rec(&format!("writer {t} memory {i}"), 0.5)).unwrap();
                }
            });
        }
    });
    let data = read_file(&f).unwrap();
    assert!(!data.torn);
    assert_eq!(data.records.len(), 100);
    let mut ids: Vec<u64> = data.records.iter().map(|r| r.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 100);
    assert_eq!(*ids.first().unwrap(), 1);
    assert_eq!(*ids.last().unwrap(), 100);
    let _ = std::fs::remove_file(&f);
    let mut lock = f.into_os_string();
    lock.push(".lock");
    let _ = std::fs::remove_file(lock);
}

#[test]
fn nan_salience_rejected_and_recall_survives() {
    // Bug 2: NaN salience used to pass clamp and crash recall's sort.
    let f = tmpfile("nan");
    let mut bad = rec("bad salience", 0.5);
    bad.salience = f32::NAN;
    assert!(append_record(&f, bad).is_err());
    // A NaN record already on disk (old file, hand edit) must not crash
    // recall: it scores 0 and sorts last.
    let mut good = rec("good memory about rust", 0.9);
    good.id = 1;
    let mut nan_rec = rec("nan memory", 0.5);
    nan_rec.id = 2;
    nan_rec.salience = f32::NAN;
    larp_memory::write_file(&f, &[good, nan_rec]).unwrap();
    let data = read_file(&f).unwrap();
    let (picked, _) = recall(&data.records, "", 2000, now_unix());
    assert_eq!(picked.len(), 2);
    assert_eq!(picked[0].record.text, "good memory about rust");
    let hits = search(&data.records, "rust", now_unix());
    assert_eq!(hits.len(), 1);
    let _ = std::fs::remove_file(&f);
}

#[test]
fn forget_of_forget_revives() {
    // Bug 7: forgetting a correction used to leave its target dead
    // forever. Parity semantics: the target revives.
    let f = tmpfile("revive");
    append_record(&f, rec("original fact", 0.9)).unwrap();
    let mut fix = Record::new(RecordType::Correction, 1.0, 0, "updated fact".to_string());
    fix.supersedes = 1;
    append_record(&f, fix).unwrap();
    let data = read_file(&f).unwrap();
    let live = live_records(&data.records, now_unix());
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, 2);
    // Forget the correction itself.
    let mut undo = Record::new(
        RecordType::Correction,
        1.0,
        0,
        "forgotten via mcp".to_string(),
    );
    undo.supersedes = 2;
    append_record(&f, undo).unwrap();
    let data = read_file(&f).unwrap();
    let live = live_records(&data.records, now_unix());
    let live_ids: Vec<u64> = live.iter().map(|r| r.id).collect();
    assert!(live_ids.contains(&1), "original revived, got {live_ids:?}");
    assert!(!live_ids.contains(&2));
    assert!(live_ids.contains(&3));
    // Compact keeps the revived record and the forget marker.
    let kept = compact(&f).unwrap();
    assert_eq!(kept, 2);
    let data = read_file(&f).unwrap();
    let live = live_records(&data.records, now_unix());
    assert!(live.iter().any(|r| r.id == 1));
    let _ = std::fs::remove_file(&f);
}

#[test]
fn whole_word_matching() {
    // Bug 8: "rust" used to match "trust" and "frustrated".
    let f = tmpfile("whole-word");
    append_record(&f, rec("trust the process", 0.5)).unwrap();
    append_record(&f, rec("rust is fast", 0.5)).unwrap();
    append_record(&f, rec("feeling frustrated today", 0.5)).unwrap();
    let data = read_file(&f).unwrap();
    let hits = search(&data.records, "rust", now_unix());
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.text, "rust is fast");
    let hits = search(&data.records, "trust", now_unix());
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.text, "trust the process");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn bm25_rare_terms_outweigh_common_ones() {
    // Bug 9: with plain match ratio, "the" counted as much as "zstd"
    // and ties fell back to insertion order. BM25 ranks the rare-term
    // document first even when it was stored last.
    let f = tmpfile("bm25");
    append_record(&f, rec("the user prefers coffee", 0.5)).unwrap();
    append_record(&f, rec("the user prefers tea", 0.5)).unwrap();
    append_record(&f, rec("zstd compression settings", 0.5)).unwrap();
    let data = read_file(&f).unwrap();
    let hits = search(&data.records, "the zstd", now_unix());
    assert_eq!(hits[0].record.text, "zstd compression settings");
    let _ = std::fs::remove_file(&f);
}

#[test]
fn tags_filter_and_jsonl_roundtrip() {
    // Bug 16: tags travel through the file and JSONL, and recall can
    // be restricted to one project/namespace.
    let f = tmpfile("tags");
    let mut work = rec("work note about deploy", 0.9);
    work.tags = vec!["work".to_string()];
    append_record(&f, work).unwrap();
    let mut home = rec("home note about garden", 0.9);
    home.tags = vec!["home".to_string()];
    append_record(&f, home).unwrap();
    let data = read_file(&f).unwrap();
    let hits = search_tagged(&data.records, "note", "work", now_unix());
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.text, "work note about deploy");
    let (picked, _) = recall_tagged(&data.records, "", 2000, "home", now_unix());
    assert_eq!(picked.len(), 1);
    assert_eq!(picked[0].record.text, "home note about garden");
    let jsonl = export_jsonl(&data.records).unwrap();
    assert!(jsonl.contains("\"tags\":[\"work\"]"));
    let g = tmpfile("tags2");
    import_jsonl(&g, &jsonl).unwrap();
    let data2 = read_file(&g).unwrap();
    assert_eq!(data2.records[0].tags, vec!["work"]);
    assert_eq!(data2.records[1].tags, vec!["home"]);
    let _ = std::fs::remove_file(&f);
    let _ = std::fs::remove_file(&g);
}

#[test]
fn relevance_hit_at_one() {
    // Bug 20 (scoped): curated queries with known right answers.
    // No SQLite/embedding baseline here: those need a database crate
    // and a model download, which this offline crate deliberately
    // avoids. The semantic-paraphrase gap is documented by the
    // ignored test below instead.
    let f = tmpfile("relevance");
    append_record(&f, rec("trust the process and keep shipping", 0.5)).unwrap();
    append_record(&f, rec("the user prefers rust for backend work", 0.9)).unwrap();
    append_record(&f, rec("zstd level 3 compresses short records well", 0.9)).unwrap();
    append_record(&f, rec("swimming lessons every saturday morning", 0.8)).unwrap();
    append_record(&f, rec("the user lives in ottawa near carling", 0.9)).unwrap();
    append_record(&f, rec("client will not pay the outstanding invoice", 0.9)).unwrap();
    let data = read_file(&f).unwrap();
    let cases = [
        ("rust", "the user prefers rust for backend work"),
        ("zstd", "zstd level 3 compresses short records well"),
        ("swimming", "swimming lessons every saturday morning"),
        ("ottawa", "the user lives in ottawa near carling"),
        ("invoice", "client will not pay the outstanding invoice"),
    ];
    for (query, expected) in cases {
        let hits = search(&data.records, query, now_unix());
        assert!(!hits.is_empty(), "no hits for {query}");
        assert_eq!(hits[0].record.text, expected, "query: {query}");
    }
    let _ = std::fs::remove_file(&f);
}

#[test]
#[ignore = "needs semantic search: keyword ranking cannot match a paraphrase with no shared words (bug 12)"]
fn semantic_paraphrase_gap() {
    let f = tmpfile("semantic-gap");
    append_record(&f, rec("client will not pay the outstanding invoice", 0.9)).unwrap();
    let data = read_file(&f).unwrap();
    let hits = search(&data.records, "outstanding AR dispute", now_unix());
    assert!(!hits.is_empty());
    let _ = std::fs::remove_file(&f);
}

#[test]
fn mcp_new_tools_and_protocol_version() {
    // Bugs 5, 15, 17, 18: remember takes supersedes, recall lines
    // carry salience/timestamp, list/stats tools exist, and the server
    // answers its own protocol version instead of echoing the client's.
    let f = tmpfile("mcp-new");
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
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"1999-01-01\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"0\"}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"remember\",\"arguments\":{\"text\":\"dup test memory\",\"type\":\"fact\",\"salience\":0.9}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"remember\",\"arguments\":{\"text\":\"dup test memory\",\"type\":\"fact\"}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"remember\",\"arguments\":{\"text\":\"dup test corrected\",\"type\":\"correction\",\"supersedes\":1}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"recall\",\"arguments\":{\"query\":\"\",\"max_tokens\":500}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{\"name\":\"list\",\"arguments\":{}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"tools/call\",\"params\":{\"name\":\"stats\",\"arguments\":{}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"tools/call\",\"params\":{\"name\":\"forget\",\"arguments\":{\"id\":2}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"tools/call\",\"params\":{\"name\":\"recall\",\"arguments\":{\"query\":\"\",\"max_tokens\":500}}}\n",
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
    assert_eq!(lines.len(), 10);
    assert_eq!(lines[0]["result"]["protocolVersion"], "2024-11-05");
    let tools = lines[1]["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"list"));
    assert!(names.contains(&"stats"));
    let txt = |i: usize| {
        lines[i]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(txt(2).contains("stored #1"));
    assert!(txt(3).contains("already stored as #1"));
    assert!(txt(4).contains("stored #2"));
    assert!(txt(5).contains("dup test corrected"));
    assert!(txt(5).contains("salience=") && txt(5).contains("ts="));
    assert!(txt(6).contains("dup test corrected"));
    assert!(txt(7).contains("live_records: 1"));
    assert!(txt(8).contains("superseded #2"));
    // Forgetting the correction revives the original over MCP too.
    assert!(txt(9).contains("dup test memory"));
    let _ = std::fs::remove_file(&f);
}

#[test]
fn stats_reports_counts() {
    let f = tmpfile("stats");
    let id1 = append_record(&f, rec("alpha fact", 0.9)).unwrap();
    append_record(&f, rec("beta fact", 0.5)).unwrap();
    let mut pref = Record::new(
        RecordType::Preference,
        0.7,
        0,
        "likes short answers".to_string(),
    );
    let _ = &mut pref;
    append_record(&f, pref).unwrap();
    let mut corr = Record::new(
        RecordType::Correction,
        1.0,
        0,
        "alpha fact is outdated".to_string(),
    );
    corr.supersedes = id1;
    append_record(&f, corr).unwrap();

    let bin = env!("CARGO_BIN_EXE_larp");
    let out = Command::new(bin)
        .args(["--file"])
        .arg(&f)
        .arg("stats")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("total_records: 4"), "{text}");
    assert!(text.contains("live_records: 3"), "{text}");
    assert!(text.contains("superseded_or_expired: 1"), "{text}");
    assert!(
        text.contains("live_by_type: fact=1 preference=1 episode=0 correction=1"),
        "{text}"
    );
    assert!(text.contains("live_tokens_est: "), "{text}");
    let _ = std::fs::remove_file(&f);
}
