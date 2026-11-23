//! Fixed-set semantic benchmark (v1.4): BM25 vs vector vs hybrid on a
//! small corpus shipped in-repo, so the numbers are reproducible and the
//! table in `docs/BENCHMARKS.md` can be re-measured by anyone.
//!
//! Run: `cargo run --release --example semantic_bench` (needs the
//! embedding model: `LARP_MODEL_DIR` locally, lazy download on CI).
//!
//! Corpus: 12 short records + 4 long records (~300 tokens each, so the
//! sidecar holds 2 chunks and the best-chunk rule is exercised).
//! Queries have known gold record ids (a set when two records answer):
//! keyword queries share tokens with the gold, paraphrase queries share
//! none, trap queries share a token with a distractor that outranks the
//! gold on keywords alone.

use larp_memory::embed::OnnxEmbedder;
use larp_memory::sidecar::{build_index, read_sidecar, sidecar_path, Embedder};
use larp_memory::{
    append_record, hybrid_search, now_unix, read_file, search, vector_search, Record, RecordType,
};

fn long_record(base: &str, repeats: usize) -> String {
    std::iter::repeat_n(format!("{base} "), repeats)
        .collect::<String>()
        .trim()
        .to_string()
}

fn main() {
    let dir = std::env::temp_dir().join(format!("larp-sem-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("memory.larp");

    let texts: Vec<String> = vec![
        "the user prefers rust for backend work".into(),
        "home base is Ottawa".into(),
        "client will not pay the outstanding invoice".into(),
        "the invoice template was updated last week".into(),
        "the user drinks espresso every morning".into(),
        "the user likes tea in the afternoon".into(),
        "meeting notes: discussed quarterly planning and agreed to follow up next week".into(),
        "the user debugged a flaky integration test for two hours and found a race condition in the retry logic".into(),
        "pricing page needs a clearer call to action".into(),
        "the user prefers python for data scripts".into(),
        "the dog's name is Biscuit".into(),
        "flight to Ahmedabad departs January 7".into(),
        long_record(
            "The quarterly planning session covered API rate limits, onboarding flow, and the pricing page. The team agreed to follow up next week with the owners of each workstream.",
            10,
        ),
        long_record(
            "The flaky integration test failed intermittently for two hours before the race condition in the retry logic was found and fixed with a backoff.",
            12,
        ),
        long_record(
            "The database migration runbook lists the steps for moving the production tables with zero downtime, including the rollback plan.",
            12,
        ),
        long_record(
            "The onboarding flow for new hires covers the codebase tour, the on-call rotation, and the first-week checklist.",
            12,
        ),
    ];
    for (i, text) in texts.iter().enumerate() {
        let id = append_record(&file, Record::new(RecordType::Fact, 0.9, 0, text.clone())).unwrap();
        assert_eq!(id, (i + 1) as u64, "record ids must match the list order");
    }

    let embedder = match OnnxEmbedder::load_or_download() {
        Ok(e) => e,
        Err(msg) => {
            eprintln!("semantic_bench needs the embedding model: {msg}");
            std::process::exit(1);
        }
    };
    build_index(&file, &embedder, false).unwrap();
    let sidecar = read_sidecar(&sidecar_path(&file)).unwrap().unwrap();
    let data = read_file(&file).unwrap();
    let records = &data.records;
    let now = now_unix();

    // (class, query, gold record ids)
    let queries: Vec<(&str, &str, &[u64])> = vec![
        ("keyword", "rust", &[1]),
        ("keyword", "invoice template", &[4]),
        ("keyword", "quarterly planning", &[7, 13]),
        ("keyword", "race condition", &[8, 14]),
        ("paraphrase", "unpaid bill", &[3]),
        ("paraphrase", "early caffeine", &[5]),
        ("paraphrase", "lives in the capital", &[2]),
        ("trap", "the user likes coffee", &[5]),
        ("trap", "tea every morning", &[5]),
    ];

    let mut bm25_hit1 = 0usize;
    let mut vec_hit1 = 0usize;
    let mut hyb_hit1 = 0usize;
    let mut bm25_hit5 = 0usize;
    let mut vec_hit5 = 0usize;
    let mut hyb_hit5 = 0usize;
    let mut bm25_hit10 = 0usize;
    let mut vec_hit10 = 0usize;
    let mut hyb_hit10 = 0usize;
    let n = queries.len();

    println!(
        "{:<11} {:<24} {:<8} {:<8} {:<8} {:<8}",
        "class", "query", "gold", "bm25@1", "vec@1", "hyb@1"
    );
    for (class, query, gold) in &queries {
        let qv = embedder
            .embed_chunks(query)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let bm25 = search(records, query, now);
        let vec = vector_search(records, &qv, &sidecar, None, now);
        let hyb = hybrid_search(records, query, Some(&qv), Some(&sidecar), None, now);
        let top = |hits: &[larp_memory::Scored]| -> u64 {
            hits.first().map(|s| s.record.id).unwrap_or(0)
        };
        let in_top = |hits: &[larp_memory::Scored], k: usize| -> bool {
            hits.iter().take(k).any(|s| gold.contains(&s.record.id))
        };
        if gold.contains(&top(&bm25)) {
            bm25_hit1 += 1;
        }
        if gold.contains(&top(&vec)) {
            vec_hit1 += 1;
        }
        if gold.contains(&top(&hyb)) {
            hyb_hit1 += 1;
        }
        if in_top(&bm25, 5) {
            bm25_hit5 += 1;
        }
        if in_top(&vec, 5) {
            vec_hit5 += 1;
        }
        if in_top(&hyb, 5) {
            hyb_hit5 += 1;
        }
        if in_top(&bm25, 10) {
            bm25_hit10 += 1;
        }
        if in_top(&vec, 10) {
            vec_hit10 += 1;
        }
        if in_top(&hyb, 10) {
            hyb_hit10 += 1;
        }
        println!(
            "{:<11} {:<24} {:<8} {:<8} {:<8} {:<8}",
            class,
            query,
            gold.iter()
                .map(|g| g.to_string())
                .collect::<Vec<_>>()
                .join(","),
            top(&bm25),
            top(&vec),
            top(&hyb)
        );
    }
    println!();
    println!("queries: {n} (4 keyword, 3 paraphrase, 2 trap), records: 16 (12 short, 4 long)");
    println!("| leg    | hit@1 | any-hit@5 | R@10 |");
    println!("|--------|-------|-----------|------|");
    println!("| BM25   | {bm25_hit1}/{n} | {bm25_hit5}/{n} | {bm25_hit10}/{n} |");
    println!("| vector | {vec_hit1}/{n} | {vec_hit5}/{n} | {vec_hit10}/{n} |");
    println!("| hybrid | {hyb_hit1}/{n} | {hyb_hit5}/{n} | {hyb_hit10}/{n} |");
    let _ = std::fs::remove_dir_all(&dir);
}
