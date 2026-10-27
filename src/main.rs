//! `larp` CLI: append, list, dump, search, recall, compact, import, export, mcp.
//! Argument parsing is std-only. Global flag `--file <path>` may appear anywhere;
//! default file is `memory.larp` in the current directory.

mod mcp;

use larp::{
    append_record, compact, dump_markdown, export_jsonl, import_jsonl, live_records, now_unix,
    read_file, recall, search, Record, RecordType,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn usage() -> &'static str {
    "usage: larp [--file <path>] <command> [args]\n\
     \n\
     commands:\n\
     \x20 append --type <fact|preference|episode|correction> [--salience <0..1>] [--ttl <secs>] [--supersedes <id>] <text>\n\
     \x20 list\n\
     \x20 dump\n\
     \x20 search <query>\n\
     \x20 recall [--max-tokens <n>] [query]\n\
     \x20 compact\n\
     \x20 import <path.jsonl>|-\n\
     \x20 export [--live]\n\
     \x20 mcp"
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
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
            let ttl: i64 = flag_value(rest, "--ttl")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let supersedes: u64 = flag_value(rest, "--supersedes")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let skip = ["--type", "--salience", "--ttl", "--supersedes"];
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
            let mut record = Record::new(rtype, salience, ttl, text);
            record.supersedes = supersedes;
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
                    println!(
                        "#{} [{}] salience={:.2} ts={} {}",
                        r.id,
                        r.rtype.name(),
                        r.salience,
                        r.timestamp,
                        r.text
                    );
                }
            }
            Ok(())
        }
        "search" => {
            let query = rest.join(" ");
            let data = read_file(file)?;
            if data.torn {
                eprintln!("warning: ignored a torn/corrupt tail");
            }
            let records = data.records;
            for s in search(&records, &query, now_unix()) {
                println!(
                    "#{} score={:.3} [{}] {}",
                    s.record.id,
                    s.score,
                    s.record.rtype.name(),
                    s.record.text
                );
            }
            Ok(())
        }
        "recall" => {
            let max_tokens: usize = flag_value(rest, "--max-tokens")
                .and_then(|v| v.parse().ok())
                .unwrap_or(2000);
            // Drop the --max-tokens flag and its value; the rest is the query.
            let mut qparts: Vec<String> = Vec::new();
            let mut j = 0;
            while j < rest.len() {
                if rest[j] == "--max-tokens" {
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
            let (picked, used) = recall(&records, &query, max_tokens, now_unix());
            for s in &picked {
                println!(
                    "#{} score={:.3} [{}] {}",
                    s.record.id,
                    s.score,
                    s.record.rtype.name(),
                    s.record.text
                );
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
