//! .larp — agent memory file format (v0.2).
//!
//! Layout: header (`LARP` magic + u16 version) followed by append-only
//! records. Each record frame is `[payload_len u32 LE][crc32 u32 LE][payload]`.
//! A torn or corrupt tail is detected via length/CRC checks and ignored, so a
//! killed writer can never corrupt earlier records.
//!
//! Format v1 payloads are stored raw; v2 payloads are zstd-compressed. The
//! reader accepts both; writers always emit v2, and appending to a v1 file
//! migrates it to v2.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAGIC: &[u8; 4] = b"LARP";
pub const FORMAT_VERSION: u16 = 2;
/// Oldest format version the reader still accepts.
pub const MIN_READER_VERSION: u16 = 1;
/// Refuse to decompress a single record beyond this (zip-bomb guard).
const MAX_RECORD_BYTES: u64 = 16 * 1024 * 1024;

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
    pub supersedes: u64,
    pub text: String,
}

impl Record {
    pub fn new(rtype: RecordType, salience: f32, ttl_secs: i64, text: String) -> Record {
        Record {
            id: 0,
            rtype,
            timestamp: now_unix(),
            salience: salience.clamp(0.0, 1.0),
            ttl_secs,
            supersedes: 0,
            text,
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

fn encode_record(r: &Record) -> Vec<u8> {
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

fn decode_record(payload: &[u8]) -> Option<Record> {
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
    })
}

fn header_bytes() -> Vec<u8> {
    let mut header = Vec::with_capacity(8);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    header.extend_from_slice(&[0u8, 0u8]);
    header
}

/// Encode one record as a v2 frame (zstd-compressed payload, CRC over the
/// compressed bytes).
fn encode_frame(r: &Record) -> Result<Vec<u8>, String> {
    let payload = encode_record(r);
    let compressed = zstd::bulk::compress(&payload, 3).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(8 + compressed.len());
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    out.extend_from_slice(&crc32(&compressed).to_le_bytes());
    out.extend_from_slice(&compressed);
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
}

/// Read all records from a file. Accepts format v1 (raw payloads) and v2
/// (zstd payloads).
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
            });
        }
        Err(e) => return Err(e.to_string()),
    }
    if bytes.is_empty() {
        return Ok(FileData {
            version: FORMAT_VERSION,
            records: Vec::new(),
            torn: false,
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
        match decode_record(&payload) {
            Some(r) => records.push(r),
            None => {
                torn = true;
                break;
            }
        }
        off += 8 + len;
    }
    Ok(FileData {
        version,
        records,
        torn,
    })
}

/// Rewrite the whole file in v2 format with exactly these records.
pub fn write_file(path: &Path, records: &[Record]) -> Result<(), String> {
    let mut bytes = header_bytes();
    for r in records {
        bytes.extend_from_slice(&encode_frame(r)?);
    }
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// Append a record, assigning the next id. Creates the file (with header) if
/// it does not exist yet. Appending to a v1 file migrates the whole file to
/// v2 (a torn tail, if any, is dropped — it was already unrecoverable).
pub fn append_record(path: &Path, mut record: Record) -> Result<u64, String> {
    let data = read_file(path)?;
    let next_id = data.records.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    record.id = next_id;
    if data.version == 1 && !data.records.is_empty() {
        let mut all = data.records;
        all.push(record);
        write_file(path, &all)?;
        return Ok(next_id);
    }
    let frame = encode_frame(&record)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() == 0 {
        file.write_all(&header_bytes()).map_err(|e| e.to_string())?;
    }
    file.write_all(&frame).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    Ok(next_id)
}

/// Records that are currently live: not expired, and not superseded by a
/// correction record.
pub fn live_records(records: &[Record], now: i64) -> Vec<&Record> {
    let superseded: std::collections::HashSet<u64> = records
        .iter()
        .filter(|r| r.rtype == RecordType::Correction && r.supersedes != 0)
        .map(|r| r.supersedes)
        .collect();
    records
        .iter()
        .filter(|r| !r.expired(now) && !superseded.contains(&r.id))
        .collect()
}

fn recency_decay(timestamp: i64, now: i64) -> f64 {
    let age_days = ((now - timestamp).max(0) as f64) / 86_400.0;
    0.5f64.powf(age_days / 90.0)
}

/// Score a record against a query. Empty query => salience x recency only.
pub fn score(record: &Record, terms: &[String], now: i64) -> f64 {
    let match_ratio = if terms.is_empty() {
        1.0
    } else {
        let hay = record.text.to_lowercase();
        let hits = terms.iter().filter(|t| hay.contains(t.as_str())).count();
        hits as f64 / terms.len() as f64
    };
    match_ratio * record.salience as f64 * recency_decay(record.timestamp, now)
}

pub struct Scored<'a> {
    pub record: &'a Record,
    pub score: f64,
}

/// Search live records, best first. Empty query returns everything ranked by
/// salience x recency.
pub fn search<'a>(records: &'a [Record], query: &str, now: i64) -> Vec<Scored<'a>> {
    let terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    let mut out: Vec<Scored> = live_records(records, now)
        .into_iter()
        .map(|r| Scored {
            record: r,
            score: score(r, &terms, now),
        })
        .filter(|s| terms.is_empty() || s.score > 0.0)
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    out
}

/// Recall records fitting a token budget, best first. Records larger than the
/// remaining budget are skipped, not truncated.
pub fn recall<'a>(
    records: &'a [Record],
    query: &str,
    max_tokens: usize,
    now: i64,
) -> (Vec<Scored<'a>>, usize) {
    let ranked = search(records, query, now);
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

/// Rewrite the file dropping expired and superseded records (correction
/// records themselves are folded away with their targets).
pub fn compact(path: &Path) -> Result<usize, String> {
    let data = read_file(path)?;
    let now = now_unix();
    let live = live_records(&data.records, now);
    let live_ids: std::collections::HashSet<u64> = live.iter().map(|r| r.id).collect();
    let kept: Vec<Record> = data
        .records
        .iter()
        .filter(|r| live_ids.contains(&r.id))
        .cloned()
        .collect();
    let n = kept.len();
    write_file(path, &kept)?;
    Ok(n)
}

/// Render records as Markdown (human-debuggable dump).
pub fn dump_markdown(records: &[Record]) -> String {
    let mut out = String::new();
    for r in records {
        out.push_str(&format!(
            "- [{}] #{} (salience {:.2}, ts {}) {}\n",
            r.rtype.name(),
            r.id,
            r.salience,
            r.timestamp,
            r.text
        ));
    }
    out
}

// ---------- JSONL interchange (Phase 4) ----------

use serde::{Deserialize, Serialize};

/// One record as it appears in JSONL interchange. `id` is optional on import
/// (fresh ids are assigned); it is always present on export.
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
        parsed.push(Record {
            id: jr.id.unwrap_or(0),
            rtype,
            timestamp: if jr.timestamp != 0 {
                jr.timestamp
            } else {
                now_unix()
            },
            salience: jr.salience.clamp(0.0, 1.0),
            ttl_secs: jr.ttl_secs,
            supersedes: jr.supersedes,
            text: jr.text,
        });
    }
    let existing = read_file(path)?;
    let mut used: std::collections::HashSet<u64> = existing.records.iter().map(|r| r.id).collect();
    let mut next_id = used.iter().max().copied().unwrap_or(0) + 1;
    let mut id_map: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
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
    write_file(path, &all)?;
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
    fn record_roundtrip() {
        let mut r = Record::new(RecordType::Fact, 0.8, 0, "hello world".to_string());
        r.id = 7;
        let decoded = decode_record(&encode_record(&r)).unwrap();
        assert_eq!(decoded.id, 7);
        assert_eq!(decoded.text, "hello world");
        assert!((decoded.salience - 0.8).abs() < f32::EPSILON);
    }
}
