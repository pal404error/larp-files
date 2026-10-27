//! `larp` CLI: append, list, dump, search, recall, compact, import, export, stats, mcp.
//! Argument parsing is std-only. Global flag `--file <path>` may appear anywhere;
//! default file is `memory.larp` in the current directory.

mod mcp;

use larp_memory::{
    append_record, compact, dump_markdown, export_jsonl, find_duplicate, import_jsonl,
    live_records, now_unix, read_file, recall, recall_tagged, search, search_tagged, stats_text,
    Record, RecordType,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn usage() -> &'static str {
    "usage: larp [--file <path>] <command> [args]\n\
     \n\
     commands:\n\
     \x20 append --type <fact|preference|episode|correction> [--salience <0..1>] [--ttl <secs>] [--supersedes <id>] [--tag <t>]... [--meta <k=v>]... <text>\n\
     \x20 list\n\
     \x20 dump\n\
     \x20 search [--tag <t>] <query>\n\
     \x20 recall [--max-tokens <n>] [--tag <t>] [query]\n\
     \x20 compact\n\
     \x20 stats\n\
     \x20 import <path.jsonl>|-\n\
     \x20 export [--live]\n\
     \x20 mcp"
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn flag_values(args: &[String], name: &str) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == name)
        .map(|w| w[1].clone())
        .collect()
}

/// Render one record the way list/search/recall print it: id, score (when
/// ranked), salience, timestamp, type, text, and tags when present.
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

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    // Pull out the global --file flag wherever it appears.
    let mut file = PathBuf::from("memory.larp");
    let mut args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == "--file" && i + 1 < raw.len() {
            file = PathBuf::from(&raw[i + 1]);
            i += 2;
        } else {
            args.push(raw[i].clone());
            i += 1;
        }
    }
    if args.is_empty() {
        eprintln!("{}", usage());
        return ExitCode::from(2);
    }
    if args[0] == "--version" || args[0] == "-V" {
        println!("larp {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let cmd = args[0].as_str();
    let rest = &args[1..];
    match run(&file, cmd, rest) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(file: &Path, cmd: &str, rest: &[String]) -> Result<(), String> {
    match cmd {
        "append" => {
            let rtype = flag_value(rest, "--type")
                .and_then(|t| RecordType::parse(&t))
                .unwrap_or(RecordType::Fact);
            let salience: f32 = flag_value(rest, "--salience")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.5);
            if !salience.is_finite() {
                return Err("salience must be finite (not NaN or infinite)".to_string());
            }
            let ttl: i64 = flag_value(rest, "--ttl")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let supersedes: u64 = flag_value(rest, "--supersedes")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let tags = flag_values(rest, "--tag");
            let mut meta = Vec::new();
            for m in flag_values(rest, "--meta") {
                let (k, v) = m
                    .split_once('=')
                    .ok_or_else(|| format!("--meta needs k=v, got '{m}'"))?;
                meta.push((k.to_string(), v.to_string()));
            }
            let skip = [
                "--type",
                "--salience",
                "--ttl",
                "--supersedes",
                "--tag",
                "--meta",
            ];
            let mut text_parts: Vec<String> = Vec::new();
            let mut j = 0;
            while j < rest.len() {
                if skip.contains(&rest[j].as_str()) {
                    j += 2;
                } else {
                    text_parts.push(rest[j].clone());
                    j += 1;
                }
            }
            let text = text_parts.join(" ");
            if text.trim().is_empty() {
                return Err("append needs text".to_string());
            }
            // One read serves both checks: duplicates are not stored
            // twice, and a supersedes target must actually exist.
            let data = read_file(file)?;
            if let Some(id) = find_duplicate(&data.records, rtype, &text, now_unix()) {
                println!("already stored #{id}");
                return Ok(());
            }
            if supersedes != 0 && !data.records.iter().any(|r| r.id == supersedes) {
                return Err(format!("no such record #{supersedes}"));
            }
            let mut record = Record::new(rtype, salience, ttl, text);
            record.supersedes = supersedes;
            record.tags = tags;
            record.meta = meta;
            let id = append_record(file, record)?;
            println!("appended #{id}");
            Ok(())
        }
        "list" | "dump" => {
            let data = read_file(file)?;
            if data.torn {
                eprintln!("warning: ignored a torn/corrupt tail");
            }
            let records = data.records;
            if cmd == "dump" {
                print!("{}", dump_markdown(&records));
            } else {
                for r in &records {
                    println!("{}", record_line(r, None));
                }
            }
            Ok(())
        }
        "search" => {
            let tag = flag_value(rest, "--tag");
            let mut qparts: Vec<String> = Vec::new();
            let mut j = 0;
            while j < rest.len() {
                if rest[j] == "--tag" {
                    j += 2;
                } else {
                    qparts.push(rest[j].clone());
                    j += 1;
                }
            }
            let query = qparts.join(" ");
            let data = read_file(file)?;
            if data.torn {
                eprintln!("warning: ignored a torn/corrupt tail");
            }
            let records = data.records;
            let hits = match tag.as_deref() {
                Some(t) => search_tagged(&records, &query, t, now_unix()),
                None => search(&records, &query, now_unix()),
            };
            for s in hits {
                println!("{}", record_line(s.record, Some(s.score)));
            }
            Ok(())
        }
        "recall" => {
            let max_tokens: usize = flag_value(rest, "--max-tokens")
                .and_then(|v| v.parse().ok())
                .unwrap_or(2000);
            let tag = flag_value(rest, "--tag");
            // Drop the flags and their values; the rest is the query.
            let mut qparts: Vec<String> = Vec::new();
            let mut j = 0;
            while j < rest.len() {
                if rest[j] == "--max-tokens" || rest[j] == "--tag" {
                    j += 2;
                } else {
                    qparts.push(rest[j].clone());
                    j += 1;
                }
            }
            let query = qparts.join(" ");
            let data = read_file(file)?;
            if data.torn {
                eprintln!("warning: ignored a torn/corrupt tail");
            }
            let records = data.records;
            let (picked, used) = match tag.as_deref() {
                Some(t) => recall_tagged(&records, &query, max_tokens, t, now_unix()),
                None => recall(&records, &query, max_tokens, now_unix()),
            };
            for s in &picked {
                println!("{}", record_line(s.record, Some(s.score)));
            }
            println!("tokens used: {used} / {max_tokens}");
            Ok(())
        }
        "compact" => {
            let before = read_file(file)?.records;
            let live_before = live_records(&before, now_unix()).len();
            let kept = compact(file)?;
            println!("compacted: {live_before} live -> {kept} records kept");
            Ok(())
        }
        "stats" => {
            let data = read_file(file)?;
            if data.torn {
                eprintln!("warning: ignored a torn/corrupt tail");
            }
            let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
            println!("file: {}", file.display());
            println!("{}", stats_text(&data.records, size, now_unix()));
            Ok(())
        }
        "import" => {
            let Some(src) = rest.first() else {
                return Err("import needs a path (or - for stdin)".to_string());
            };
            let input = if src == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(|e| e.to_string())?;
                buf
            } else {
                std::fs::read_to_string(src).map_err(|e| e.to_string())?
            };
            let ids = import_jsonl(file, &input)?;
            println!("imported {} records: {:?}", ids.len(), ids);
            Ok(())
        }
        "export" => {
            let data = read_file(file)?;
            if data.torn {
                eprintln!("warning: ignored a torn/corrupt tail");
            }
            let records: Vec<Record> = if rest.iter().any(|a| a == "--live") {
                live_records(&data.records, now_unix())
                    .into_iter()
                    .cloned()
                    .collect()
            } else {
                data.records
            };
            print!("{}", export_jsonl(&records)?);
            Ok(())
        }
        "mcp" => mcp::run(file),
        _ => {
            let _ = has_flag;
            Err(format!("unknown command: {cmd}\n{}", usage()))
        }
    }
}
