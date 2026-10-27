//! Phase 3 benchmark: .larp (v2, zstd) vs Markdown vs JSONL on 10,000 records.
//! Run: cargo run --release --example bench
//! Prints sizes, ratios, and search/recall timings to stdout.

use larp_memory::{
    append_record, dump_markdown, now_unix, read_file, recall, search, write_file, Record,
    RecordType,
};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn main() {
    // Deterministic pseudo-random generator (LCG) — no extra dependencies.
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 33) as usize
    };

    let langs = ["rust", "python", "go", "typescript", "java"];
    let cities = ["ottawa", "toronto", "montreal", "vancouver", "waterloo"];
    let topics = [
        "quarterly planning",
        "api rate limits",
        "onboarding flow",
        "database migration",
        "pricing page",
    ];
    let mut records = Vec::with_capacity(10_000);
    for i in 0..10_000 {
        let kind = next() % 4;
        let text = match kind {
            0 => format!("the user prefers {} for backend work", langs[next() % langs.len()]),
            1 => format!("the user lives in {}", cities[next() % cities.len()]),
            2 => format!(
                "meeting notes: discussed {} and agreed to follow up next week with the team",
                topics[next() % topics.len()]
            ),
            _ => format!(
                "episode {i}: the user debugged a flaky integration test for two hours and found a race condition in the retry logic"
            ),
        };
        let mut r = Record::new(
            if kind == 3 {
                RecordType::Episode
            } else {
                RecordType::Fact
            },
            0.3 + (next() % 70) as f32 / 100.0,
            0,
            text,
        );
        r.id = i as u64 + 1;
        records.push(r);
    }

    let dir = PathBuf::from("/tmp/larp-bench");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let larp_path = dir.join("memory.larp");
    let md_path = dir.join("memory.md");
    let jsonl_path = dir.join("memory.jsonl");

    let t = Instant::now();
    write_file(&larp_path, &records).unwrap();
    let write_ms = t.elapsed().as_millis();

    let md = dump_markdown(&records);
    std::fs::write(&md_path, &md).unwrap();
    let mut jsonl = String::new();
    for r in &records {
        jsonl.push_str(&format!(
            "{{\"id\":{},\"type\":\"{}\",\"salience\":{},\"text\":\"{}\"}}\n",
            r.id,
            r.rtype.name(),
            r.salience,
            json_escape(&r.text)
        ));
    }
    std::fs::write(&jsonl_path, &jsonl).unwrap();

    let larp_size = std::fs::metadata(&larp_path).unwrap().len();
    let md_size = std::fs::metadata(&md_path).unwrap().len();
    let jsonl_size = std::fs::metadata(&jsonl_path).unwrap().len();

    let data = read_file(&larp_path).unwrap();
    let now = now_unix();
    let t = Instant::now();
    for _ in 0..20 {
        let _ = search(&data.records, "rust backend", now);
    }
    let search_ms = t.elapsed().as_millis() as f64 / 20.0;
    let t = Instant::now();
    for _ in 0..20 {
        let _ = recall(&data.records, "rust", 2000, now);
    }
    let recall_ms = t.elapsed().as_millis() as f64 / 20.0;
    let t = Instant::now();
    append_record(
        &larp_path,
        Record::new(RecordType::Fact, 0.9, 0, "one more memory".to_string()),
    )
    .unwrap();
    let append_ms = t.elapsed().as_millis();
    let (picked, used) = recall(&data.records, "rust", 2000, now);

    let mut out = std::io::stdout();
    writeln!(out, "records: {}", records.len()).unwrap();
    writeln!(out, "larp_bytes: {larp_size}").unwrap();
    writeln!(out, "md_bytes: {md_size}").unwrap();
    writeln!(out, "jsonl_bytes: {jsonl_size}").unwrap();
    writeln!(
        out,
        "larp_vs_md_pct: {:.1}",
        larp_size as f64 / md_size as f64 * 100.0
    )
    .unwrap();
    writeln!(
        out,
        "larp_vs_jsonl_pct: {:.1}",
        larp_size as f64 / jsonl_size as f64 * 100.0
    )
    .unwrap();
    writeln!(out, "bulk_write_ms: {write_ms}").unwrap();
    writeln!(out, "search_avg_ms: {search_ms:.2}").unwrap();
    writeln!(out, "recall_avg_ms: {recall_ms:.2}").unwrap();
    writeln!(out, "single_append_ms: {append_ms}").unwrap();
    writeln!(
        out,
        "recall_2000_tokens: picked={} used={}",
        picked.len(),
        used
    )
    .unwrap();
}
