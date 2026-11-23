//! Semantic-vector sidecar (`<file>.vec`) for v1.4.
//!
//! The `.larp` file is never touched by semantic recall. Vectors live in a
//! rebuildable sidecar next to it: record id -> one float32 vector per
//! text chunk (see `docs/VECTORS.md` for the full spec). A lost or corrupt
//! sidecar is never data loss — `larp index` rebuilds it.
//!
//! The sidecar is a cache, so it carries no CRC and no repair path: a
//! sidecar that fails to parse, or whose header disagrees with the
//! `.larp` file, is stale and gets rebuilt.

use crate::{get_bytes, get_uvarint, put_uvarint, read_file, Record};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const SIDECAR_MAGIC: &[u8; 4] = b"LARV";
pub const SIDECAR_VERSION: u16 = 1;

/// The embedder a sidecar build needs. Implemented by the ONNX runtime in
/// `embed.rs` (Phase 2); tests use a stub. Chunking lives here: the
/// embedder splits text into at most-256-token chunks and returns one
/// vector per chunk.
pub trait Embedder {
    fn model_id(&self) -> &str;
    fn dim(&self) -> usize;
    /// One vector per chunk, in chunk order. An empty text embeds to no
    /// chunks; the record simply gets no vector.
    fn embed_chunks(&self, text: &str) -> Result<Vec<Vec<f32>>, String>;
}

/// Sibling sidecar path for a memory file: `memory.larp` -> `memory.larp.vec`.
pub fn sidecar_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".vec");
    PathBuf::from(s)
}

/// (mtime in nanoseconds since the Unix epoch, byte length) for a file.
pub fn file_stat(path: &Path) -> Result<(u64, u64), String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() * 1_000_000_000 + d.subsec_nanos() as u64)
        .unwrap_or(0);
    Ok((mtime_ns, meta.len()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarHeader {
    pub dim: u16,
    pub model_id: String,
    pub source_mtime_ns: u64,
    pub source_len: u64,
    pub source_count: u64,
}

impl SidecarHeader {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(SIDECAR_MAGIC);
        out.extend_from_slice(&SIDECAR_VERSION.to_le_bytes());
        out.extend_from_slice(&self.dim.to_le_bytes());
        put_uvarint(&mut out, self.model_id.len() as u64);
        out.extend_from_slice(self.model_id.as_bytes());
        out.extend_from_slice(&self.source_mtime_ns.to_le_bytes());
        out.extend_from_slice(&self.source_len.to_le_bytes());
        out.extend_from_slice(&self.source_count.to_le_bytes());
        out
    }

    /// Parse a header at the start of `bytes`, returning the header and
    /// the offset just past it. None on bad magic, unsupported version,
    /// or truncation.
    fn decode(bytes: &[u8]) -> Option<(SidecarHeader, usize)> {
        if bytes.len() < 8 || &bytes[0..4] != SIDECAR_MAGIC {
            return None;
        }
        let version = u16::from_le_bytes(bytes[4..6].try_into().ok()?);
        if version != SIDECAR_VERSION {
            return None;
        }
        let dim = u16::from_le_bytes(bytes[6..8].try_into().ok()?);
        let mut off = 8usize;
        let model_len = get_uvarint(bytes, &mut off)? as usize;
        let model_bytes = get_bytes(bytes, &mut off, model_len)?;
        let model_id = String::from_utf8(model_bytes.to_vec()).ok()?;
        let mtime = u64::from_le_bytes(get_bytes(bytes, &mut off, 8)?.try_into().ok()?);
        let len = u64::from_le_bytes(get_bytes(bytes, &mut off, 8)?.try_into().ok()?);
        let count = u64::from_le_bytes(get_bytes(bytes, &mut off, 8)?.try_into().ok()?);
        Some((
            SidecarHeader {
                dim,
                model_id,
                source_mtime_ns: mtime,
                source_len: len,
                source_count: count,
            },
            off,
        ))
    }
}

/// A parsed sidecar: header plus record id -> chunk vectors in order.
#[derive(Debug, Clone)]
pub struct Sidecar {
    pub header: SidecarHeader,
    /// Record id -> one vector per chunk, chunk order preserved.
    pub vectors: HashMap<u64, Vec<Vec<f32>>>,
}

impl Sidecar {
    pub fn new(header: SidecarHeader) -> Sidecar {
        Sidecar {
            header,
            vectors: HashMap::new(),
        }
    }

    /// True when this sidecar matches the given embedder and `.larp`
    /// file state. Any mismatch means stale: rebuild, never repair.
    pub fn is_fresh(
        &self,
        model_id: &str,
        dim: usize,
        source_mtime_ns: u64,
        source_len: u64,
        source_count: u64,
    ) -> bool {
        self.header.model_id == model_id
            && self.header.dim as usize == dim
            && self.header.source_mtime_ns == source_mtime_ns
            && self.header.source_len == source_len
            && self.header.source_count == source_count
    }

    /// Chunk vectors for one record, in chunk order. Empty when the
    /// record has no vectors (never indexed, or empty text).
    pub fn vectors_for(&self, id: u64) -> &[Vec<f32>] {
        self.vectors.get(&id).map(|v| v.as_slice()).unwrap_or(&[])
    }

    fn encode(&self) -> Vec<u8> {
        let mut out = self.header.encode();
        let mut ids: Vec<u64> = self.vectors.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let chunks = &self.vectors[&id];
            for (chunk_idx, vec) in chunks.iter().enumerate() {
                put_uvarint(&mut out, id);
                put_uvarint(&mut out, chunk_idx as u64);
                for v in vec {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        out
    }

    /// Parse a whole sidecar file. Err on bad magic, unsupported version,
    /// truncation, or a vector whose length disagrees with the header dim.
    /// Callers treat Err as "stale, rebuild".
    fn decode(bytes: &[u8]) -> Result<Sidecar, String> {
        let (header, mut off) = SidecarHeader::decode(bytes)
            .ok_or_else(|| "bad sidecar header (magic/version/truncated)".to_string())?;
        let dim = header.dim as usize;
        let mut vectors: HashMap<u64, Vec<Vec<f32>>> = HashMap::new();
        while off < bytes.len() {
            let id = get_uvarint(bytes, &mut off)
                .ok_or_else(|| "truncated sidecar entry (id)".to_string())?;
            let chunk = get_uvarint(bytes, &mut off)
                .ok_or_else(|| "truncated sidecar entry (chunk)".to_string())?
                as usize;
            let raw = get_bytes(bytes, &mut off, dim * 4)
                .ok_or_else(|| "truncated sidecar entry (vector)".to_string())?;
            let mut vec = Vec::with_capacity(dim);
            for i in 0..dim {
                vec.push(f32::from_le_bytes(
                    raw[i * 4..i * 4 + 4].try_into().unwrap(),
                ));
            }
            let chunks = vectors.entry(id).or_default();
            // Entries are written sorted by (id, chunk); a chunk index
            // that is not the next one means the file is not ours.
            if chunks.len() != chunk {
                return Err(format!(
                    "sidecar entries out of order (record {id} chunk {chunk})"
                ));
            }
            chunks.push(vec);
        }
        Ok(Sidecar { header, vectors })
    }
}

/// Read a sidecar file. Ok(None) when it does not exist yet; Err when it
/// exists but cannot be parsed (caller rebuilds).
pub fn read_sidecar(path: &Path) -> Result<Option<Sidecar>, String> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(Sidecar::decode(&bytes)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Write a sidecar atomically (temp file + fsync + rename).
pub fn write_sidecar(path: &Path, sidecar: &Sidecar) -> Result<(), String> {
    crate::write_bytes_atomic(path, &sidecar.encode())
}

/// Result of a (re)build: what was embedded and what the sidecar holds.
pub struct IndexReport {
    /// Records embedded in this run (0 when already fresh).
    pub indexed: usize,
    /// Chunks written in this run.
    pub chunks: usize,
    /// Records in the `.larp` file.
    pub total_records: usize,
    /// Vectors now in the sidecar (records x chunks).
    pub total_chunks: usize,
    /// Sidecar bytes on disk.
    pub sidecar_bytes: u64,
    /// True when the sidecar was rebuilt from scratch.
    pub rebuilt: bool,
}

/// Build (or incrementally update) the sidecar for a `.larp` file.
///
/// - Fresh sidecar (header matches embedder + file state): nothing is
///   embedded, the existing sidecar is returned.
/// - Same model, changed file: vectors for surviving record ids are
///   kept; only missing ids are embedded.
/// - Model change, corrupt sidecar, or `force`: full rebuild.
///
/// The record list is snapshotted under the `.larp` advisory lock, then
/// embedding runs unlocked. Before the atomic sidecar write, the file is
/// re-statted; if it moved, the snapshot is retaken (up to 2 retries),
/// so an append mid-index can never leave a sidecar whose header
/// disagrees with the file it describes.
pub fn build_index(
    larp_path: &Path,
    embedder: &dyn Embedder,
    force: bool,
) -> Result<IndexReport, String> {
    let dim = embedder.dim();
    if dim == 0 || dim > u16::MAX as usize {
        return Err(format!("embedder dim out of range: {dim}"));
    }
    let vec_path = sidecar_path(larp_path);
    for _attempt in 0..3 {
        // Snapshot under the lock: records + file stat, consistent.
        let (records, mtime_ns, len) = {
            let _guard = crate::acquire_lock(larp_path)?;
            let data = read_file(larp_path)?;
            let (mtime_ns, len) = file_stat(larp_path)?;
            (data.records, mtime_ns, len)
        };
        let count = records.len() as u64;
        let header = SidecarHeader {
            dim: dim as u16,
            model_id: embedder.model_id().to_string(),
            source_mtime_ns: mtime_ns,
            source_len: len,
            source_count: count,
        };
        if !force {
            if let Some(existing) = read_sidecar(&vec_path)? {
                if existing.is_fresh(embedder.model_id(), dim, mtime_ns, len, count) {
                    let total_chunks: usize = existing.vectors.values().map(|c| c.len()).sum();
                    return Ok(IndexReport {
                        indexed: 0,
                        chunks: 0,
                        total_records: records.len(),
                        total_chunks,
                        sidecar_bytes: std::fs::metadata(&vec_path).map(|m| m.len()).unwrap_or(0),
                        rebuilt: false,
                    });
                }
                if existing.header.model_id == embedder.model_id()
                    && existing.header.dim as usize == dim
                {
                    // Incremental: keep vectors for ids that still exist.
                    let mut sidecar = Sidecar::new(header.clone());
                    let mut indexed = 0usize;
                    let mut chunks = 0usize;
                    for r in &records {
                        if let Some(v) = existing.vectors.get(&r.id) {
                            sidecar.vectors.insert(r.id, v.clone());
                        } else {
                            let cv = embedder.embed_chunks(&r.text)?;
                            chunks += cv.len();
                            if !cv.is_empty() {
                                sidecar.vectors.insert(r.id, cv);
                            }
                            indexed += 1;
                        }
                    }
                    return finish_index(&vec_path, sidecar, &records, indexed, chunks, false);
                }
            }
        }
        // Full rebuild.
        let mut sidecar = Sidecar::new(header);
        let mut chunks = 0usize;
        for r in &records {
            let cv = embedder.embed_chunks(&r.text)?;
            chunks += cv.len();
            if !cv.is_empty() {
                sidecar.vectors.insert(r.id, cv);
            }
        }
        let indexed = records.len();
        // Re-check the file before writing: if it moved, retake the
        // snapshot (the loop's next attempt). Otherwise write.
        let (mtime2, len2) = file_stat(larp_path)?;
        if mtime2 == mtime_ns && len2 == len {
            return finish_index(&vec_path, sidecar, &records, indexed, chunks, true);
        }
    }
    // The file kept moving under us. One last snapshot + write: the
    // header describes exactly this snapshot, so a later use detects
    // any further drift as stale and rebuilds.
    let (records, mtime_ns, len) = {
        let _guard = crate::acquire_lock(larp_path)?;
        let data = read_file(larp_path)?;
        let (mtime_ns, len) = file_stat(larp_path)?;
        (data.records, mtime_ns, len)
    };
    let header = SidecarHeader {
        dim: dim as u16,
        model_id: embedder.model_id().to_string(),
        source_mtime_ns: mtime_ns,
        source_len: len,
        source_count: records.len() as u64,
    };
    let mut sidecar = Sidecar::new(header);
    let mut chunks = 0usize;
    for r in &records {
        let cv = embedder.embed_chunks(&r.text)?;
        chunks += cv.len();
        if !cv.is_empty() {
            sidecar.vectors.insert(r.id, cv);
        }
    }
    let indexed = records.len();
    finish_index(&vec_path, sidecar, &records, indexed, chunks, true)
}

fn finish_index(
    vec_path: &Path,
    sidecar: Sidecar,
    records: &[Record],
    indexed: usize,
    chunks: usize,
    rebuilt: bool,
) -> Result<IndexReport, String> {
    let total_chunks: usize = sidecar.vectors.values().map(|c| c.len()).sum();
    write_sidecar(vec_path, &sidecar)?;
    let sidecar_bytes = std::fs::metadata(vec_path).map(|m| m.len()).unwrap_or(0);
    Ok(IndexReport {
        indexed,
        chunks,
        total_records: records.len(),
        total_chunks,
        sidecar_bytes,
        rebuilt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{append_record, RecordType};
    use std::cell::Cell;

    /// Deterministic stub: splits text into `chunk_chars`-sized chunks,
    /// one fixed vector per chunk (chunk i is all (i+1) as f32). Counts
    /// embed calls so incremental builds can prove they embed less.
    struct StubEmbedder {
        dim: usize,
        chunk_chars: usize,
        calls: Cell<usize>,
    }

    impl StubEmbedder {
        fn new(dim: usize, chunk_chars: usize) -> StubEmbedder {
            StubEmbedder {
                dim,
                chunk_chars,
                calls: Cell::new(0),
            }
        }
    }

    impl Embedder for StubEmbedder {
        fn model_id(&self) -> &str {
            "test/stub-embedder"
        }

        fn dim(&self) -> usize {
            self.dim
        }

        fn embed_chunks(&self, text: &str) -> Result<Vec<Vec<f32>>, String> {
            self.calls.set(self.calls.get() + 1);
            if text.is_empty() {
                return Ok(Vec::new());
            }
            let n = text.chars().count().div_ceil(self.chunk_chars).max(1);
            Ok((0..n).map(|i| vec![(i + 1) as f32; self.dim]).collect())
        }
    }

    fn tmpfile(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("larp-sidecar-{name}-{}.larp", std::process::id()))
    }

    fn rec(text: &str) -> Record {
        Record::new(RecordType::Fact, 0.9, 0, text.to_string())
    }

    #[test]
    fn sidecar_roundtrip_with_multi_chunk_entries() {
        let f = tmpfile("roundtrip");
        append_record(&f, rec("short")).unwrap();
        append_record(&f, rec("this is a much longer record text")).unwrap();
        let embedder = StubEmbedder::new(4, 10);
        let report = build_index(&f, &embedder, false).unwrap();
        assert_eq!(report.total_records, 2);
        assert!(report.rebuilt);
        let sidecar = read_sidecar(&sidecar_path(&f)).unwrap().unwrap();
        // "short" -> 1 chunk; the long text (34 chars, 10-char chunks) -> 4.
        assert_eq!(sidecar.vectors_for(1).len(), 1);
        assert_eq!(sidecar.vectors_for(2).len(), 4);
        assert_eq!(sidecar.vectors_for(2)[0], vec![1.0; 4]);
        assert_eq!(sidecar.vectors_for(2)[3], vec![4.0; 4]);
        assert_eq!(sidecar.vectors_for(99).len(), 0);
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_file(sidecar_path(&f));
    }

    #[test]
    fn staleness_detects_mtime_len_count_and_model_mismatch() {
        let f = tmpfile("stale");
        append_record(&f, rec("one")).unwrap();
        let embedder = StubEmbedder::new(4, 10);
        build_index(&f, &embedder, false).unwrap();
        let sidecar = read_sidecar(&sidecar_path(&f)).unwrap().unwrap();
        let h = &sidecar.header;
        assert!(sidecar.is_fresh(
            "test/stub-embedder",
            4,
            h.source_mtime_ns,
            h.source_len,
            h.source_count
        ));
        assert!(!sidecar.is_fresh(
            "other/model",
            4,
            h.source_mtime_ns,
            h.source_len,
            h.source_count
        ));
        assert!(!sidecar.is_fresh(
            "test/stub-embedder",
            8,
            h.source_mtime_ns,
            h.source_len,
            h.source_count
        ));
        assert!(!sidecar.is_fresh(
            "test/stub-embedder",
            4,
            h.source_mtime_ns + 1,
            h.source_len,
            h.source_count
        ));
        assert!(!sidecar.is_fresh(
            "test/stub-embedder",
            4,
            h.source_mtime_ns,
            h.source_len + 1,
            h.source_count
        ));
        assert!(!sidecar.is_fresh(
            "test/stub-embedder",
            4,
            h.source_mtime_ns,
            h.source_len,
            h.source_count + 1
        ));
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_file(sidecar_path(&f));
    }

    #[test]
    fn incremental_build_embeds_only_missing_ids() {
        let f = tmpfile("incremental");
        append_record(&f, rec("first")).unwrap();
        append_record(&f, rec("second")).unwrap();
        let embedder = StubEmbedder::new(4, 10);
        build_index(&f, &embedder, false).unwrap();
        assert_eq!(embedder.calls.get(), 2);
        // Fresh: second build embeds nothing.
        let report = build_index(&f, &embedder, false).unwrap();
        assert_eq!(report.indexed, 0);
        assert!(!report.rebuilt);
        assert_eq!(embedder.calls.get(), 2);
        // One append: only the new record is embedded.
        append_record(&f, rec("third")).unwrap();
        let report = build_index(&f, &embedder, false).unwrap();
        assert_eq!(report.indexed, 1);
        assert!(!report.rebuilt);
        assert_eq!(embedder.calls.get(), 3);
        let sidecar = read_sidecar(&sidecar_path(&f)).unwrap().unwrap();
        assert_eq!(sidecar.vectors.len(), 3);
        // Force: full rebuild embeds everything again.
        let report = build_index(&f, &embedder, true).unwrap();
        assert!(report.rebuilt);
        assert_eq!(report.indexed, 3);
        assert_eq!(embedder.calls.get(), 6);
        let _ = std::fs::remove_file(&f);
        let _ = std::fs::remove_file(sidecar_path(&f));
    }

    #[test]
    fn corrupt_sidecar_is_an_error_not_a_panic() {
        let f = tmpfile("corrupt");
        let vec_path = sidecar_path(&f);
        std::fs::write(&vec_path, b"NOT A SIDECAR").unwrap();
        assert!(read_sidecar(&vec_path).is_err());
        std::fs::write(&vec_path, b"LARV\x01\x00").unwrap();
        assert!(read_sidecar(&vec_path).is_err());
        // Valid header, truncated entry.
        let mut header = SidecarHeader {
            dim: 2,
            model_id: "m".to_string(),
            source_mtime_ns: 0,
            source_len: 0,
            source_count: 0,
        }
        .encode();
        header.extend_from_slice(&[1u8]); // id, then nothing
        std::fs::write(&vec_path, &header).unwrap();
        assert!(read_sidecar(&vec_path).is_err());
        let _ = std::fs::remove_file(&vec_path);
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn missing_sidecar_reads_as_none() {
        let f = tmpfile("missing");
        assert!(read_sidecar(&sidecar_path(&f)).unwrap().is_none());
        let _ = std::fs::remove_file(&f);
    }
}
