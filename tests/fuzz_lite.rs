//! Fuzz-lite: deterministic random mutations of a valid .larp file.
//! The reader must never panic on any input — every outcome must be a clean
//! Ok (possibly with `torn = true`) or a clean Err. This is not a full
//! coverage-guided fuzzer (see cargo-fuzz as future work); it is a cheap
//! regression net for the parser's unsafe corners: huge length fields,
//! corrupt CRCs, truncated frames, and garbage bytes.

use larp::{append_record, read_file, Record, RecordType};
use std::path::PathBuf;

fn tmpfile(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("larp-fuzz-{name}-{}.larp", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn random_mutations_never_panic() {
    let f = tmpfile("mutations");
    for i in 0..20 {
        append_record(
            &f,
            Record::new(
                RecordType::Fact,
                0.8,
                0,
                format!("fuzz seed memory number {i} about rust"),
            ),
        )
        .unwrap();
    }
    let original = std::fs::read(&f).unwrap();

    // Deterministic LCG so failures reproduce exactly.
    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 33) as usize
    };

    for _ in 0..500 {
        let mut bytes = original.clone();
        match next() % 3 {
            0 => {
                // Flip a few random bytes (hits CRCs, lengths, tags).
                for _ in 0..(next() % 8 + 1) {
                    if !bytes.is_empty() {
                        let i = next() % bytes.len();
                        bytes[i] ^= 0xFF;
                    }
                }
            }
            1 => {
                // Truncate at a random point (torn writes).
                bytes.truncate(next() % (bytes.len() + 1));
            }
            _ => {
                // Append garbage (corrupt tail).
                bytes.extend(std::iter::repeat_n(0xFF, next() % 32));
            }
        }
        std::fs::write(&f, &bytes).unwrap();
        // No panic allowed; any Ok/Err outcome is acceptable.
        let _ = read_file(&f);
    }
    let _ = std::fs::remove_file(&f);
}

#[test]
fn pathological_inputs_never_panic() {
    let f = tmpfile("pathological");
    let mut huge_len = b"LARP".to_vec();
    huge_len.extend_from_slice(&2u16.to_le_bytes());
    huge_len.extend_from_slice(&[0u8, 0u8]);
    // Frame claiming a 4 GB payload in a tiny file.
    huge_len.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    huge_len.extend_from_slice(&0u32.to_le_bytes());
    let inputs: Vec<Vec<u8>> = vec![
        vec![],
        vec![0u8; 1024],
        vec![0xFF; 512],
        b"LARP".to_vec(),
        b"LARP\x02\x00".to_vec(),
        huge_len,
    ];
    for junk in inputs {
        std::fs::write(&f, &junk).unwrap();
        let _ = read_file(&f);
    }
    let _ = std::fs::remove_file(&f);
}
