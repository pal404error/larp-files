//! Quickstart: use .larp as a library.
//! Run: cargo run --example quickstart

use larp::{append_record, now_unix, read_file, recall, Record, RecordType};

fn main() {
    let path = std::env::temp_dir().join("larp-quickstart.larp");
    let _ = std::fs::remove_file(&path);

    append_record(
        &path,
        Record::new(
            RecordType::Fact,
            0.9,
            0,
            "the user prefers rust".to_string(),
        ),
    )
    .unwrap();
    append_record(
        &path,
        Record::new(
            RecordType::Preference,
            0.7,
            0,
            "likes short answers".to_string(),
        ),
    )
    .unwrap();

    let data = read_file(&path).unwrap();
    let (picked, used) = recall(&data.records, "rust", 2000, now_unix());
    for s in &picked {
        println!(
            "#{} [{}] {}",
            s.record.id,
            s.record.rtype.name(),
            s.record.text
        );
    }
    println!("tokens used: {used} / 2000");
    let _ = std::fs::remove_file(&path);
}
