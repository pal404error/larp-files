//! .larp — agent memory file format (v0.3).
//!
//! Layout: header (`LARP` magic + u16 version + u16 reserved) followed by
//! append-only frames.
//!
//! Format v3 frames (writers always emit v3):
//! `[payload_len uvarint][crc32 u32 LE][id uvarint][flags u8][payload]`
//! where flags bit 0 means zstd-compressed; when compression would not
//! shrink a payload it is stored raw instead. The CRC covers the id bytes,
//! the flags byte, and the stored payload bytes.
//!
//! Format v3 payload:
//! `[type u8][timestamp svarint][salience f32 LE][ttl svarint]`
//! `[supersedes uvarint][text_len uvarint][text]`
//! `[tag_count uvarint][tag_len uvarint + tag bytes]*`
//! `[meta_count uvarint][key_len uvarint + key + val_len uvarint + val]*`
//! Variable-length integers keep the common cases small: ttl and supersedes
//! are almost always 0, so they cost one byte each instead of eight.
//!
//! Format v1 payloads are stored raw and v2 payloads zstd-compressed, both
//! with the old 41-byte fixed header. The reader accepts v1, v2, and v3.
//! Appending to a v1/v2 file migrates it to v3.
//!
//! Safety properties:
//! - A torn or corrupt tail is detected via length/CRC checks and ignored
//!   on read, and **truncated before the next append**, so a crash
//!   mid-write can never make later appends silently unreachable.
//! - Whole-file rewrites (compact, import, migration) write a temp file,
//!   fsync it, then rename it over the original, so a crash mid-rewrite
//!   leaves the old file intact.
//! - Appends fsync before returning, so "appended" means durable.
//! - All mutations take an advisory exclusive lock on `<file>.lock`, so
//!   two writers cannot interleave or lose each other's records. (Advisory:
//!   it coordinates larp writers; it cannot stop other programs editing
//!   the file by hand.)

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAGIC: &[u8; 4] = b"LARP";
pub const FORMAT_VERSION: u16 = 3;
/// Oldest format version the reader still accepts.
pub const MIN_READER_VERSION: u16 = 1;
/// Refuse to decompress a single record beyond this (zip-bomb guard).
const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;
/// v3 frame flags bit: payload is zstd-compressed (clear = stored raw).
const FLAG_COMPRESSED: u8 = 0x01;

/// CRC-32 (IEEE) table, built at compile time. std-only, no crates needed.
const fn make_crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = make_crc_table();

pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

// ---------- Variable-length integers (LEB128, unsigned + zigzag signed) ----------

fn put_uvarint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

/// Parse a uvarint starting at `*off`, advancing it past the value.
/// Returns None on truncation or on an overlong/non-canonical encoding
/// (more than 10 bytes, or a 10th byte above 1).
fn get_uvarint(bytes: &[u8], off: &mut usize) -> Option<u64> {
    let mut result: u64 = 0;
    for i in 0..10 {
        let b = *bytes.get(*off)?;
        *off += 1;
        if i == 9 {
            if b > 1 {
                return None;
            }
            result |= (b as u64) << 63;
            return Some(result);
        }
        result |= ((b & 0x7F) as u64) << (i * 7);
        if b & 0x80 == 0 {
            return Some(result);
        }
    }
    None
}

fn put_svarint(out: &mut Vec<u8>, v: i64) {
    put_uvarint(out, ((v << 1) ^ (v >> 63)) as u64);
}

fn get_svarint(bytes: &[u8], off: &mut usize) -> Option<i64> {
    let u = get_uvarint(bytes, off)?;
    Some(((u >> 1) as i64) ^ -((u & 1) as i64))
}

fn get_bytes<'a>(bytes: &'a [u8], off: &mut usize, len: usize) -> Option<&'a [u8]> {
    let start = *off;
    let end = start.checked_add(len)?;
    if end > bytes.len() {
        return None;
    }
    *off = end;
    Some(&bytes[start..end])
}

fn get_string(bytes: &[u8], off: &mut usize) -> Option<String> {
    let len = get_uvarint(bytes, off)? as usize;
    let raw = get_bytes(bytes, off, len)?;
    String::from_utf8(raw.to_vec()).ok()
}

// ---------- Records ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    Fact,
    Preference,
    Episode,
    Correction,
}

impl RecordType {
    pub fn tag(self) -> u8 {
        match self {
            RecordType::Fact => 0,
            RecordType::Preference => 1,
            RecordType::Episode => 2,
            RecordType::Correction => 3,
        }
    }

    pub fn from_tag(tag: u8) -> Option<RecordType> {
        match tag {
            0 => Some(RecordType::Fact),
            1 => Some(RecordType::Preference),
            2 => Some(RecordType::Episode),
            3 => Some(RecordType::Correction),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RecordType::Fact => "fact",
            RecordType::Preference => "preference",
            RecordType::Episode => "episode",
            RecordType::Correction => "correction",
        }
    }

    pub fn parse(s: &str) -> Option<RecordType> {
        match s.to_ascii_lowercase().as_str() {
            "fact" => Some(RecordType::Fact),
            "preference" => Some(RecordType::Preference),
            "episode" => Some(RecordType::Episode),
            "correction" => Some(RecordType::Correction),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Record {
    pub id: u64,
    pub rtype: RecordType,
    pub timestamp: i64,
    pub salience: f32,
    /// Seconds after `timestamp` when this record expires. 0 = never.
    pub ttl_secs: i64,
    /// For corrections: id of the record this one supersedes. 0 = none.
    /// Only correction records supersede; the field is inert on other types.
    pub supersedes: u64,
    pub text: String,
    /// Project/namespace tags (v3 only; empty on v1/v2 records).
    pub tags: Vec<String>,
    /// Extensible key/value metadata (v3 only; empty on v1/v2 records).
    pub meta: Vec<(String, String)>,
}

impl Record {
    pub fn new(rtype: RecordType, salience: f32, ttl_secs: i64, text: String) -> Record {
        // f32::clamp passes NaN through (all comparisons are false), so
        // sanitize non-finite values explicitly: a NaN salience used to
        // crash recall's sort. 0.5 is the neutral default.
        let salience = if salience.is_finite() {
            salience.clamp(0.0, 1.0)
        } else {
            0.5
        };
        Record {
            id: 0,
            rtype,
            timestamp: now_unix(),
            salience,
            ttl_secs,
            supersedes: 0,
            text,
            tags: Vec::new(),
            meta: Vec::new(),
        }
    }

    pub fn expired(&self, now: i64) -> bool {
        self.ttl_secs > 0 && now > self.timestamp + self.ttl_secs
    }

    /// Rough token estimate for budgeting: ~4 chars per token.
    pub fn est_tokens(&self) -> usize {
        self.text.chars().count().div_ceil(4) + 1
    }
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Legacy v1/v2 payload encoding (41-byte fixed header + text).
/// Only used by tests now (writers emit v3); kept so the legacy path
/// stays roundtrip-tested.
#[cfg(test)]
fn encode_record_legacy(r: &Record) -> Vec<u8> {
    let text = r.text.as_bytes();
    let mut out = Vec::with_capacity(41 + text.len());
    out.extend_from_slice(&r.id.to_le_bytes());
    out.push(r.rtype.tag());
    out.extend_from_slice(&r.timestamp.to_le_bytes());
    out.extend_from_slice(&r.salience.to_le_bytes());
    out.extend_from_slice(&r.ttl_secs.to_le_bytes());
    out.extend_from_slice(&r.supersedes.to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(text);
    out
}

fn decode_record_legacy(payload: &[u8]) -> Option<Record> {
    if payload.len() < 41 {
        return None;
    }
    let id = u64::from_le_bytes(payload[0..8].try_into().ok()?);
    let rtype = RecordType::from_tag(payload[8])?;
    let timestamp = i64::from_le_bytes(payload[9..17].try_into().ok()?);
    let salience = f32::from_le_bytes(payload[17..21].try_into().ok()?);
    let ttl_secs = i64::from_le_bytes(payload[21..29].try_into().ok()?);
    let supersedes = u64::from_le_bytes(payload[29..37].try_into().ok()?);
    let text_len = u32::from_le_bytes(payload[37..41].try_into().ok()?) as usize;
    if payload.len() != 41 + text_len {
        return None;
    }
    let text = String::from_utf8(payload[41..].to_vec()).ok()?;
    Some(Record {
        id,
        rtype,
        timestamp,
        salience,
        ttl_secs,
        supersedes,
        text,
        tags: Vec::new(),
        meta: Vec::new(),
    })
}

/// v3 payload encoding (record id lives in the frame, not the payload).
fn encode_record_v3(r: &Record) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(r.rtype.tag());
    put_svarint(&mut out, r.timestamp);
    out.extend_from_slice(&r.salience.to_le_bytes());
    put_svarint(&mut out, r.ttl_secs);
    put_uvarint(&mut out, r.supersedes);
    put_uvarint(&mut out, r.text.len() as u64);
    out.extend_from_slice(r.text.as_bytes());
    put_uvarint(&mut out, r.tags.len() as u64);
    for t in &r.tags {
        put_uvarint(&mut out, t.len() as u64);
        out.extend_from_slice(t.as_bytes());
    }
    put_uvarint(&mut out, r.meta.len() as u64);
    for (k, v) in &r.meta {
        put_uvarint(&mut out, k.len() as u64);
        out.extend_from_slice(k.as_bytes());
        put_uvarint(&mut out, v.len() as u64);
        out.extend_from_slice(v.as_bytes());
    }
    out
}

fn decode_record_v3(payload: &[u8], id: u64) -> Option<Record> {
    let mut off = 0usize;
    let rtype = RecordType::from_tag(*payload.get(off)?)?;
    off += 1;
    let timestamp = get_svarint(payload, &mut off)?;
    let salience = f32::from_le_bytes(get_bytes(payload, &mut off, 4)?.try_into().ok()?);
    let ttl_secs = get_svarint(payload, &mut off)?;
    let supersedes = get_uvarint(payload, &mut off)?;
    let text = get_string(payload, &mut off)?;
    let tag_count = get_uvarint(payload, &mut off)? as usize;
    let mut tags = Vec::with_capacity(tag_count.min(1024));
    for _ in 0..tag_count {
        tags.push(get_string(payload, &mut off)?);
    }
    let meta_count = get_uvarint(payload, &mut off)? as usize;
    let mut meta = Vec::with_capacity(meta_count.min(1024));
    for _ in 0..meta_count {
        let k = get_string(payload, &mut off)?;
        let v = get_string(payload, &mut off)?;
        meta.push((k, v));
    }
    if off != payload.len() {
        return None;
    }
    Some(Record {
        id,
        rtype,
        timestamp,
        salience,
        ttl_secs,
        supersedes,
        text,
        tags,
        meta,
    })
}

fn header_bytes() -> Vec<u8> {
    let mut header = Vec::with_capacity(8);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    header.extend_from_slice(&[0u8, 0u8]);
    header
}

/// Encode one record as a v3 frame. The payload is zstd-compressed unless
/// compression would not shrink it, in which case it is stored raw.
fn encode_frame_v3(r: &Record) -> Result<Vec<u8>, String> {
    let payload = encode_record_v3(r);
    let compressed = zstd::bulk::compress(&payload, 3).map_err(|e| e.to_string())?;
    let (stored, flags) = if compressed.len() < payload.len() {
        (compressed, FLAG_COMPRESSED)
    } else {
        (payload, 0u8)
    };
    let mut id_bytes = Vec::new();
    put_uvarint(&mut id_bytes, r.id);
    let mut crc_data = Vec::with_capacity(id_bytes.len() + 1 + stored.len());
    crc_data.extend_from_slice(&id_bytes);
    crc_data.push(flags);
    crc_data.extend_from_slice(&stored);
    let mut out = Vec::new();
    put_uvarint(&mut out, stored.len() as u64);
    out.extend_from_slice(&crc32(&crc_data).to_le_bytes());
    out.extend_from_slice(&id_bytes);
    out.push(flags);
    out.extend_from_slice(&stored);
    Ok(out)
}

fn decompress_capped(data: &[u8]) -> Option<Vec<u8>> {
    let decoder = zstd::stream::Decoder::new(std::io::Cursor::new(data)).ok()?;
    let mut out = Vec::new();
    decoder
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut out)
        .ok()?;
    if out.len() as u64 > MAX_RECORD_BYTES {
        return None;
    }
    Some(out)
}

pub struct FileData {
    pub version: u16,
    pub records: Vec<Record>,
    /// True when a trailing partial/corrupt frame was found and ignored.
    pub torn: bool,
    /// Byte offset just past the last valid frame (== file length when
    /// not torn). Appends truncate back to this offset when torn is true.
    pub good_len: usize,
}

/// Read all records from a file. Accepts format v1 (raw payloads), v2
/// (zstd payloads), and v3 (varint frames, compressed or raw payloads).
pub fn read_file(path: &Path) -> Result<FileData, String> {
    let mut bytes = Vec::new();
    match File::open(path) {
        Ok(mut f) => {
            f.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileData {
                version: FORMAT_VERSION,
                records: Vec::new(),
                torn: false,
                good_len: 0,
            });
        }
        Err(e) => return Err(e.to_string()),
    }
    if bytes.is_empty() {
        return Ok(FileData {
            version: FORMAT_VERSION,
            records: Vec::new(),
            torn: false,
            good_len: 0,
        });
    }
    if bytes.len() < 8 || &bytes[0..4] != MAGIC {
        return Err("not a .larp file (bad magic)".to_string());
    }
    let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    if !(MIN_READER_VERSION..=FORMAT_VERSION).contains(&version) {
        return Err(format!("unsupported .larp format version: {version}"));
    }
    let mut records = Vec::new();
    let mut torn = false;
    let mut off = 8usize;
    let mut good_len = 8usize;
    if version == 3 {
        while off < bytes.len() {
            let mut p = off;
            let len = match get_uvarint(&bytes, &mut p) {
                Some(v) => v as usize,
                None => {
                    torn = true;
                    break;
                }
            };
            if bytes.len() - p < 4 {
                torn = true;
                break;
            }
            let crc = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
            p += 4;
            let id_start = p;
            let id = match get_uvarint(&bytes, &mut p) {
                Some(v) => v,
                None => {
                    torn = true;
                    break;
                }
            };
            let flags = match bytes.get(p) {
                Some(b) => *b,
                None => {
                    torn = true;
                    break;
                }
            };
            p += 1;
            if flags & !FLAG_COMPRESSED != 0 {
                torn = true;
                break;
            }
            if bytes.len() - p < len {
                torn = true;
                break;
            }
            let stored = &bytes[p..p + len];
            let mut crc_data = Vec::with_capacity((p - id_start) + 1 + len);
            crc_data.extend_from_slice(&bytes[id_start..p - 1]);
            crc_data.push(flags);
            crc_data.extend_from_slice(stored);
            if crc32(&crc_data) != crc {
                torn = true;
                break;
            }
            let payload = if flags & FLAG_COMPRESSED != 0 {
                match decompress_capped(stored) {
                    Some(p) => p,
                    None => {
                        torn = true;
                        break;
                    }
                }
            } else {
                stored.to_vec()
            };
            match decode_record_v3(&payload, id) {
                Some(r) => records.push(r),
                None => {
                    torn = true;
                    break;
                }
            }
            off = p + len;
            good_len = off;
        }
    } else {
        while off < bytes.len() {
            if bytes.len() - off < 8 {
                torn = true;
                break;
            }
            let len = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
            let crc = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
            if bytes.len() - off - 8 < len {
                torn = true;
                break;
            }
            let stored = &bytes[off + 8..off + 8 + len];
            if crc32(stored) != crc {
                torn = true;
                break;
            }
            let payload = if version == 1 {
                stored.to_vec()
            } else {
                match decompress_capped(stored) {
                    Some(p) => p,
                    None => {
                        torn = true;
                        break;
                    }
                }
            };
            match decode_record_legacy(&payload) {
                Some(r) => records.push(r),
                None => {
                    torn = true;
                    break;
                }
            }
            off += 8 + len;
            good_len = off;
        }
    }
    Ok(FileData {
        version,
        records,
        torn,
        good_len,
    })
}

/// Scan only v3 frame headers (CRC-checked, no decompression, no decoding).
/// Returns (last good record id, good_len, torn). Used by the fast append
/// path so appending no longer re-reads and unpacks the whole file.
fn scan_v3_headers(bytes: &[u8]) -> (u64, usize, bool) {
    let mut last_id = 0u64;
    let mut off = 8usize;
    let mut good_len = 8usize;
    let mut torn = false;
    while off < bytes.len() {
        let mut p = off;
        let len = match get_uvarint(bytes, &mut p) {
            Some(v) => v as usize,
            None => {
                torn = true;
                break;
            }
        };
        if bytes.len() - p < 4 {
            torn = true;
            break;
        }
        let crc = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap());
        p += 4;
        let id_start = p;
        let id = match get_uvarint(bytes, &mut p) {
            Some(v) => v,
            None => {
                torn = true;
                break;
            }
        };
        let flags = match bytes.get(p) {
            Some(b) => *b,
            None => {
                torn = true;
                break;
            }
        };
        p += 1;
        if flags & !FLAG_COMPRESSED != 0 {
            torn = true;
            break;
        }
        if bytes.len() - p < len {
            torn = true;
            break;
        }
        let stored = &bytes[p..p + len];
        let mut crc_data = Vec::with_capacity((p - id_start) + 1 + len);
        crc_data.extend_from_slice(&bytes[id_start..p - 1]);
        crc_data.push(flags);
        crc_data.extend_from_slice(stored);
        if crc32(&crc_data) != crc {
            torn = true;
            break;
        }
        last_id = id;
        off = p + len;
        good_len = off;
    }
    (last_id, good_len, torn)
}

// ---------- Durable, locked writes ----------

/// Sibling lock file for a data file: `memory.larp` -> `memory.larp.lock`.
/// The lock file is created on first mutation and left behind (empty);
/// it is never read as data.
fn lock_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".lock");
    PathBuf::from(s)
}

/// An advisory exclusive lock held for one mutation. Dropping it unlocks
/// (and closing the fd would too). The guard owns the lock file, so there
/// is no leak even in a long-lived server process doing many mutations.
struct LockGuard(File);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

/// Take the advisory exclusive lock for one mutation. Blocking: a second
/// writer waits instead of interleaving. Callers must hold the guard for
/// the whole mutation.
fn acquire_lock(path: &Path) -> Result<LockGuard, String> {
    use fs2::FileExt;
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(lock_path(path))
        .map_err(|e| e.to_string())?;
    f.lock_exclusive().map_err(|e| e.to_string())?;
    Ok(LockGuard(f))
}

/// Write bytes atomically: temp file in the same directory, fsync, rename
/// over the original, best-effort directory fsync so the rename survives
/// a crash. A crash mid-write leaves the old file intact.
fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir: PathBuf = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let tmp_name = format!(
        ".{}.tmp.{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("larp"),
        std::process::id()
    );
    let tmp = dir.join(tmp_name);
    let result = (|| -> Result<(), String> {
        let mut f = File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(bytes).map_err(|e| e.to_string())?;
        f.sync_data().map_err(|e| e.to_string())?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    } else if let Ok(d) = File::open(&dir) {
        let _ = d.sync_all();
    }
    result
}

/// Rewrite the whole file in v3 format with exactly these records.
/// Locked: callers must not hold the lock already (see write_file_inner).
pub fn write_file(path: &Path, records: &[Record]) -> Result<(), String> {
    let _guard = acquire_lock(path)?;
    write_file_inner(path, records)
}

fn write_file_inner(path: &Path, records: &[Record]) -> Result<(), String> {
    let mut bytes = header_bytes();
    for r in records {
        bytes.extend_from_slice(&encode_frame_v3(r)?);
    }
    write_bytes_atomic(path, &bytes)
}

/// Append a record, assigning the next id. Creates the file (with header)
/// if it does not exist yet. Appending to a v1/v2 file migrates the whole
/// file to v3 (a torn tail, if any, is dropped — it was already
/// unrecoverable). A torn v3 tail is truncated to the last good frame
/// first, so the new record is reachable and ids never repeat.
/// Rejects non-finite salience and blank text: both used to corrupt
/// later reads (NaN salience crashed recall's sort).
pub fn append_record(path: &Path, record: Record) -> Result<u64, String> {
    if !record.salience.is_finite() {
        return Err("salience must be finite (not NaN or infinite)".to_string());
    }
    if record.text.trim().is_empty() {
        return Err("append needs text".to_string());
    }
    let _guard = acquire_lock(path)?;
    append_inner(path, record)
}

fn append_inner(path: &Path, mut record: Record) -> Result<u64, String> {
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.to_string()),
    };
    if raw.is_empty() {
        record.id = 1;
        let mut bytes = header_bytes();
        bytes.extend_from_slice(&encode_frame_v3(&record)?);
        write_bytes_atomic(path, &bytes)?;
        return Ok(1);
    }
    if raw.len() < 8 || &raw[0..4] != MAGIC {
        return Err("not a .larp file (bad magic)".to_string());
    }
    let version = u16::from_le_bytes(raw[4..6].try_into().unwrap());
    if !(MIN_READER_VERSION..=FORMAT_VERSION).contains(&version) {
        return Err(format!("unsupported .larp format version: {version}"));
    }
    if version == FORMAT_VERSION {
        let (last_id, good_len, torn) = scan_v3_headers(&raw);
        if torn {
            // Cut back to the last good frame before appending, so the
            // new record is readable and its id does not collide.
            let f = OpenOptions::new()
                .write(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            f.set_len(good_len as u64).map_err(|e| e.to_string())?;
            f.sync_data().map_err(|e| e.to_string())?;
        }
        let next_id = last_id + 1;
        record.id = next_id;
        let frame = encode_frame_v3(&record)?;
        let mut f = OpenOptions::new()
            .append(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        f.write_all(&frame).map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
        f.sync_data().map_err(|e| e.to_string())?;
        return Ok(next_id);
    }
    // v1/v2: full read, then rewrite everything as v3 (torn tail dropped).
    let data = read_file(path)?;
    let next_id = data.records.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    record.id = next_id;
    let mut all = data.records;
    all.push(record);
    write_file_inner(path, &all)?;
    Ok(next_id)
}

// ---------- Liveness (with forget-of-forget revival) ----------

/// Records that are currently live: not expired, and not superseded by a
/// live correction record.
///
/// Supersession is evaluated by parity along each chain: a correction
/// supersedes its target, but if that correction is itself superseded
/// (e.g. forgotten), the target revives. So forgetting a correction
/// undoes the forget. Expired corrections stop superseding too.
pub fn live_records(records: &[Record], now: i64) -> Vec<&Record> {
    // victim id -> killer (correction) ids.
    let mut killers: HashMap<u64, Vec<u64>> = HashMap::new();
    for r in records {
        if r.rtype == RecordType::Correction && r.supersedes != 0 {
            killers.entry(r.supersedes).or_default().push(r.id);
        }
    }
    let is_live = |id: u64, live: &HashSet<u64>| -> bool {
        killers
            .get(&id)
            .map(|ks| ks.iter().all(|k| !live.contains(k)))
            .unwrap_or(true)
    };
    // Fast path: killers normally have higher ids than their victims, so
    // one descending pass decides every record.
    let mut live: HashSet<u64> = HashSet::new();
    let mut killed_by_live: HashSet<u64> = HashSet::new();
    let mut sorted: Vec<&Record> = records.iter().collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.id));
    for r in &sorted {
        if !r.expired(now) && !killed_by_live.contains(&r.id) {
            live.insert(r.id);
            if r.rtype == RecordType::Correction && r.supersedes != 0 {
                killed_by_live.insert(r.supersedes);
            }
        }
    }
    // Verify; if any record disagrees (forward references from imports),
    // iterate to the fixpoint. Cyclic supersession can only come from
    // hand-crafted files; the loop is capped and the last iterate wins,
    // deterministically, and the records stay in the file either way.
    let mut verified = true;
    for r in records {
        let expected = !r.expired(now) && is_live(r.id, &live);
        if expected != live.contains(&r.id) {
            verified = false;
            break;
        }
    }
    if !verified {
        let mut cur: HashSet<u64> = records
            .iter()
            .filter(|r| !r.expired(now))
            .map(|r| r.id)
            .collect();
        for _ in 0..=records.len() {
            let next: HashSet<u64> = records
                .iter()
                .filter(|r| !r.expired(now) && is_live(r.id, &cur))
                .map(|r| r.id)
                .collect();
            if next == cur {
                break;
            }
            cur = next;
        }
        live = cur;
    }
    records.iter().filter(|r| live.contains(&r.id)).collect()
}

// ---------- Ranking: whole-word BM25 + salience + type-based decay ----------

/// Split text into lowercase alphanumeric words. Matching is whole-word:
/// "rust" no longer matches "trust" or "frustrated".
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            cur.extend(c.to_lowercase());
        } else if !cur.is_empty() {
            tokens.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

/// Word spans of a text (alphanumeric runs), borrowed, no allocation.
/// Query terms are pre-lowercased, so matching is case-insensitive here
/// instead of lowercasing every document. ASCII text (the common case)
/// takes the byte-table fast path; only non-ASCII text pays for Unicode
/// alphanumeric checks.
fn word_spans(text: &str) -> impl Iterator<Item = &str> {
    let pred: fn(char) -> bool = if text.is_ascii() {
        |c| !c.is_ascii_alphanumeric()
    } else {
        |c| !c.is_alphanumeric()
    };
    text.split(pred).filter(|s| !s.is_empty())
}

/// Case-insensitive span match against a lowercase query term. ASCII
/// spans compare without allocating; only non-ASCII spans are folded.
fn span_matches(span: &str, term: &str) -> bool {
    if span.is_ascii() {
        span.eq_ignore_ascii_case(term)
    } else {
        span.to_lowercase() == term
    }
}

/// Facts and preferences do not fade; episodes halve every 90 days.
/// A year-old preference used to lose to a weak match from today.
fn type_decay(rtype: RecordType, timestamp: i64, now: i64) -> f64 {
    match rtype {
        RecordType::Episode => {
            let age_days = ((now - timestamp).max(0) as f64) / 86_400.0;
            0.5f64.powf(age_days / 90.0)
        }
        _ => 1.0,
    }
}

/// Salience as a finite multiplier. Records from damaged old files may
/// carry NaN; they score 0 instead of crashing the sort.
fn salience_for_score(r: &Record) -> f64 {
    if r.salience.is_finite() {
        r.salience as f64
    } else {
        0.0
    }
}

pub struct Scored<'a> {
    pub record: &'a Record,
    pub score: f64,
}

fn search_impl<'a>(
    records: &'a [Record],
    query: &str,
    tag: Option<&str>,
    now: i64,
) -> Vec<Scored<'a>> {
    let terms = tokenize(query);
    let mut unique_terms: Vec<String> = Vec::new();
    for t in terms {
        if !unique_terms.contains(&t) {
            unique_terms.push(t);
        }
    }
    let live: Vec<&Record> = live_records(records, now)
        .into_iter()
        .filter(|r| tag.map(|t| r.tags.iter().any(|x| x == t)).unwrap_or(true))
        .collect();
    let empty = unique_terms.is_empty();
    if empty {
        let mut out: Vec<Scored> = live
            .into_iter()
            .map(|r| Scored {
                record: r,
                score: salience_for_score(r) * type_decay(r.rtype, r.timestamp, now),
            })
            .collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        return out;
    }
    // BM25 (k1 = 1.2, b = 0.75). Rare terms score higher than common
    // ones, so "zstd" outweighs "the". One pass over the documents
    // collects per-document term counts, document lengths, and document
    // frequencies for the query terms only — no per-document allocation,
    // which is what keeps this fast on large files.
    let n = unique_terms.len();
    let mut doc_lens: Vec<usize> = Vec::with_capacity(live.len());
    let mut counts: Vec<u32> = vec![0; live.len() * n];
    let mut df: Vec<usize> = vec![0; n];
    let mut seen = vec![false; n];
    let mut total_len = 0usize;
    for (i, r) in live.iter().enumerate() {
        let mut len = 0usize;
        let mut touched: Vec<usize> = Vec::new();
        for span in word_spans(&r.text) {
            len += 1;
            for (j, term) in unique_terms.iter().enumerate() {
                if span_matches(span, term) {
                    counts[i * n + j] += 1;
                    if !seen[j] {
                        seen[j] = true;
                        touched.push(j);
                    }
                }
            }
        }
        for j in touched {
            df[j] += 1;
            seen[j] = false;
        }
        doc_lens.push(len);
        total_len += len;
    }
    let doc_count = live.len() as f64;
    let avg_len = if live.is_empty() {
        0.0
    } else {
        total_len as f64 / live.len() as f64
    };
    let k1 = 1.2f64;
    let b = 0.75f64;
    let mut out: Vec<Scored> = Vec::new();
    for (i, r) in live.into_iter().enumerate() {
        let doc_len = doc_lens[i] as f64;
        let mut bm = 0.0;
        for j in 0..n {
            let count = counts[i * n + j] as f64;
            if count == 0.0 {
                continue;
            }
            let dfj = df[j] as f64;
            let idf = (1.0 + (doc_count - dfj + 0.5) / (dfj + 0.5)).ln();
            let denom = count + k1 * (1.0 - b + b * doc_len / avg_len.max(1.0));
            bm += idf * (count * (k1 + 1.0)) / denom;
        }
        let score = bm * salience_for_score(r) * type_decay(r.rtype, r.timestamp, now);
        if score > 0.0 {
            out.push(Scored { record: r, score });
        }
    }
    // total_cmp: NaN can never appear (salience sanitized, BM25 finite),
    // but sort must not panic even on hostile old files.
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

/// Search live records, best first. Empty query returns everything ranked
/// by salience (episodes decayed).
pub fn search<'a>(records: &'a [Record], query: &str, now: i64) -> Vec<Scored<'a>> {
    search_impl(records, query, None, now)
}

/// Search live records carrying a given tag, best first.
pub fn search_tagged<'a>(
    records: &'a [Record],
    query: &str,
    tag: &str,
    now: i64,
) -> Vec<Scored<'a>> {
    search_impl(records, query, Some(tag), now)
}

fn recall_impl<'a>(
    records: &'a [Record],
    query: &str,
    max_tokens: usize,
    tag: Option<&str>,
    now: i64,
) -> (Vec<Scored<'a>>, usize) {
    let ranked = search_impl(records, query, tag, now);
    let mut picked = Vec::new();
    let mut used = 0usize;
    for s in ranked {
        let t = s.record.est_tokens();
        if used + t <= max_tokens {
            used += t;
            picked.push(s);
        }
    }
    (picked, used)
}

/// Recall records fitting a token budget, best first. Records larger than
/// the remaining budget are skipped, not truncated.
pub fn recall<'a>(
    records: &'a [Record],
    query: &str,
    max_tokens: usize,
    now: i64,
) -> (Vec<Scored<'a>>, usize) {
    recall_impl(records, query, max_tokens, None, now)
}

/// Recall records carrying a given tag, fitting a token budget.
pub fn recall_tagged<'a>(
    records: &'a [Record],
    query: &str,
    max_tokens: usize,
    tag: &str,
    now: i64,
) -> (Vec<Scored<'a>>, usize) {
    recall_impl(records, query, max_tokens, Some(tag), now)
}

/// Find a live record of the same type whose text is a duplicate of
/// `text`: identical after normalization, or word-set Jaccard >= 0.9.
/// Returns the existing record's id. Used by the store entry points
/// (CLI append, MCP remember) so the same fact is not stored twice.
pub fn find_duplicate(records: &[Record], rtype: RecordType, text: &str, now: i64) -> Option<u64> {
    let norm = tokenize(text).join(" ");
    if norm.is_empty() {
        return None;
    }
    let norm_set: HashSet<&str> = norm.split(' ').collect();
    for r in live_records(records, now) {
        if r.rtype != rtype {
            continue;
        }
        let other = tokenize(&r.text).join(" ");
        if other == norm {
            return Some(r.id);
        }
        let other_set: HashSet<&str> = other.split(' ').collect();
        let inter = norm_set.intersection(&other_set).count();
        let union = norm_set.union(&other_set).count();
        if union > 0 && inter as f64 / union as f64 >= 0.9 {
            return Some(r.id);
        }
    }
    None
}

// ---------- Maintenance ----------

/// Rewrite the file dropping expired and superseded records (correction
/// records themselves are folded away with their targets, except live
/// corrections, which stay).
pub fn compact(path: &Path) -> Result<usize, String> {
    let _guard = acquire_lock(path)?;
    let data = read_file(path)?;
    let now = now_unix();
    let live = live_records(&data.records, now);
    let live_ids: HashSet<u64> = live.iter().map(|r| r.id).collect();
    let kept: Vec<Record> = data
        .records
        .iter()
        .filter(|r| live_ids.contains(&r.id))
        .cloned()
        .collect();
    let n = kept.len();
    write_file_inner(path, &kept)?;
    Ok(n)
}

/// One-line-per-record stats body (the `file:` line is added by callers
/// that know the path). Shared by the CLI `stats` command and the MCP
/// `stats` tool so they can never disagree.
pub fn stats_text(records: &[Record], size: u64, now: i64) -> String {
    let live = live_records(records, now);
    let total = records.len();
    let mut by_type = [0usize; 4];
    for r in &live {
        by_type[r.rtype.tag() as usize] += 1;
    }
    let tokens: usize = live.iter().map(|r| r.est_tokens()).sum();
    let oldest = live.iter().map(|r| r.timestamp).min();
    let newest = live.iter().map(|r| r.timestamp).max();
    let range = match (oldest, newest) {
        (Some(a), Some(b)) => format!("{a}..{b}"),
        _ => "none".to_string(),
    };
    format!(
        "size_bytes: {size}\n\
         total_records: {total}\n\
         live_records: {}\n\
         superseded_or_expired: {}\n\
         live_by_type: fact={} preference={} episode={} correction={}\n\
         live_tokens_est: {tokens}\n\
         live_range_ts: {range}",
        live.len(),
        total - live.len(),
        by_type[0],
        by_type[1],
        by_type[2],
        by_type[3],
    )
}

/// Render records as Markdown (human-debuggable dump).
pub fn dump_markdown(records: &[Record]) -> String {
    let mut out = String::new();
    for r in records {
        let tags = if r.tags.is_empty() {
            String::new()
        } else {
            format!(" [tags: {}]", r.tags.join(", "))
        };
        out.push_str(&format!(
            "- [{}] #{} (salience {:.2}, ts {}) {}{}\n",
            r.rtype.name(),
            r.id,
            r.salience,
            r.timestamp,
            r.text,
            tags
        ));
    }
    out
}

// ---------- JSONL interchange ----------

use serde::{Deserialize, Serialize};

/// One record as it appears in JSONL interchange. `id` is optional on import
/// (fresh ids are assigned); it is always present on export. `tags` and
/// `meta` are omitted when empty, so old interchange files still parse.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(rename = "type")]
    pub rtype: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub salience: f32,
    #[serde(default)]
    pub ttl_secs: i64,
    #[serde(default)]
    pub supersedes: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub meta: std::collections::BTreeMap<String, String>,
}

pub fn export_jsonl(records: &[Record]) -> Result<String, String> {
    let mut out = String::new();
    for r in records {
        let jr = JsonRecord {
            id: Some(r.id),
            rtype: r.rtype.name().to_string(),
            timestamp: r.timestamp,
            salience: r.salience,
            ttl_secs: r.ttl_secs,
            supersedes: r.supersedes,
            text: r.text.clone(),
            tags: r.tags.clone(),
            meta: r.meta.iter().cloned().collect(),
        };
        out.push_str(&serde_json::to_string(&jr).map_err(|e| e.to_string())?);
        out.push('\n');
    }
    Ok(out)
}

/// Import JSONL into a file. Ids are preserved when they do not collide with
/// existing records or each other; otherwise fresh ids are assigned, and
/// `supersedes` references inside the same batch are remapped to match.
/// Returns the ids of the imported records.
pub fn import_jsonl(path: &Path, input: &str) -> Result<Vec<u64>, String> {
    let mut parsed: Vec<Record> = Vec::new();
    for (i, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let jr: JsonRecord =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        let rtype = RecordType::parse(&jr.rtype)
            .ok_or_else(|| format!("line {}: unknown type '{}'", i + 1, jr.rtype))?;
        let salience = if jr.salience.is_finite() {
            jr.salience.clamp(0.0, 1.0)
        } else {
            0.5
        };
        parsed.push(Record {
            id: jr.id.unwrap_or(0),
            rtype,
            timestamp: if jr.timestamp != 0 {
                jr.timestamp
            } else {
                now_unix()
            },
            salience,
            ttl_secs: jr.ttl_secs,
            supersedes: jr.supersedes,
            text: jr.text,
            tags: jr.tags,
            meta: jr.meta.into_iter().collect(),
        });
    }
    let _guard = acquire_lock(path)?;
    let existing = read_file(path)?;
    let mut used: HashSet<u64> = existing.records.iter().map(|r| r.id).collect();
    let mut next_id = used.iter().max().copied().unwrap_or(0) + 1;
    let mut id_map: HashMap<u64, u64> = HashMap::new();
    for r in &mut parsed {
        let old = r.id;
        if old == 0 || used.contains(&old) {
            let fresh = next_id;
            next_id += 1;
            if old != 0 {
                id_map.insert(old, fresh);
            }
            r.id = fresh;
        }
        used.insert(r.id);
    }
    for r in &mut parsed {
        if r.supersedes != 0 {
            if let Some(new) = id_map.get(&r.supersedes) {
                r.supersedes = *new;
            }
        }
    }
    let new_ids: Vec<u64> = parsed.iter().map(|r| r.id).collect();
    let mut all = existing.records;
    all.extend(parsed);
    write_file_inner(path, &all)?;
    Ok(new_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn legacy_record_roundtrip() {
        let mut r = Record::new(RecordType::Fact, 0.8, 0, "hello world".to_string());
        r.id = 7;
        let decoded = decode_record_legacy(&encode_record_legacy(&r)).unwrap();
        assert_eq!(decoded.id, 7);
        assert_eq!(decoded.text, "hello world");
        assert!((decoded.salience - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn v3_record_roundtrip_with_tags_and_meta() {
        let mut r = Record::new(RecordType::Fact, 0.8, 0, "hello world".to_string());
        r.id = 7;
        r.tags = vec!["work".to_string(), "rust".to_string()];
        r.meta = vec![("source".to_string(), "cli".to_string())];
        let frame = encode_frame_v3(&r).unwrap();
        // Decode the frame the way the reader does.
        let mut p = 0usize;
        let len = get_uvarint(&frame, &mut p).unwrap() as usize;
        p += 4; // crc
        let id = get_uvarint(&frame, &mut p).unwrap();
        let flags = frame[p];
        p += 1;
        let stored = &frame[p..p + len];
        let payload = if flags & FLAG_COMPRESSED != 0 {
            decompress_capped(stored).unwrap()
        } else {
            stored.to_vec()
        };
        let decoded = decode_record_v3(&payload, id).unwrap();
        assert_eq!(decoded.id, 7);
        assert_eq!(decoded.text, "hello world");
        assert_eq!(decoded.tags, vec!["work", "rust"]);
        assert_eq!(
            decoded.meta,
            vec![("source".to_string(), "cli".to_string())]
        );
    }

    #[test]
    fn nan_salience_is_sanitized() {
        let r = Record::new(RecordType::Fact, f32::NAN, 0, "x".to_string());
        assert!(r.salience.is_finite());
        assert!((r.salience - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn varint_roundtrip() {
        for v in [0u64, 1, 127, 128, 300, 16384, u64::MAX] {
            let mut buf = Vec::new();
            put_uvarint(&mut buf, v);
            let mut off = 0;
            assert_eq!(get_uvarint(&buf, &mut off).unwrap(), v);
            assert_eq!(off, buf.len());
        }
        for v in [0i64, -1, 1, -300, 1_793_000_000, i64::MIN, i64::MAX] {
            let mut buf = Vec::new();
            put_svarint(&mut buf, v);
            let mut off = 0;
            assert_eq!(get_svarint(&buf, &mut off).unwrap(), v);
        }
        // Overlong encodings are rejected, not read past the buffer.
        let mut off = 0;
        assert!(get_uvarint(&[0xFF; 12], &mut off).is_none());
    }
}
