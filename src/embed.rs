//! Embedding runtime for v1.4: Xenova `all-MiniLM-L6-v2` via ONNX Runtime.
//!
//! The model (~90 MB) is **never** vendored into the repo and never
//! downloaded silently: on first `--semantic` use, `load_embedder`
//! prints a one-time notice naming the model and the cache dir, then
//! downloads `model.onnx` + `tokenizer.json` from Hugging Face. If the
//! model is absent and the download fails (offline, bad network), the
//! caller falls back to BM25 — the command must not fail for a missing
//! model.
//!
//! Pooling matches sentence-transformers: mean pooling over token
//! embeddings (attention-mask weighted), then L2-normalize. Text is
//! embedded in chunks of at most 256 tokens; recall scores a record by
//! its best chunk.

use crate::sidecar::Embedder;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const MODEL_ID: &str = "Xenova/all-MiniLM-L6-v2";
pub const MODEL_DIM: usize = 384;
/// Max tokens per chunk, including the CLS/SEP pair.
pub const MAX_TOKENS_PER_CHUNK: usize = 256;
const MODEL_URL: &str =
    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx";
const TOKENIZER_URL: &str =
    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json";
const MODEL_FILE: &str = "model.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";

/// Resolve the model cache dir: `$LARP_MODEL_DIR`, else
/// `$XDG_CACHE_HOME/larp/models`, else `~/.cache/larp/models`.
/// The pure core is `resolve_model_dir` (unit-tested without env vars).
pub fn resolve_model_dir(
    larp_model_dir: Option<&str>,
    xdg_cache_home: Option<&str>,
    home: Option<&str>,
) -> PathBuf {
    if let Some(d) = larp_model_dir.filter(|s| !s.is_empty()) {
        return PathBuf::from(d);
    }
    if let Some(x) = xdg_cache_home.filter(|s| !s.is_empty()) {
        return PathBuf::from(x).join("larp").join("models");
    }
    let home = home.unwrap_or("").to_string();
    if home.is_empty() {
        PathBuf::from(".cache").join("larp").join("models")
    } else {
        PathBuf::from(home)
            .join(".cache")
            .join("larp")
            .join("models")
    }
}

pub fn model_dir() -> PathBuf {
    resolve_model_dir(
        std::env::var("LARP_MODEL_DIR").ok().as_deref(),
        std::env::var("XDG_CACHE_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

pub fn model_files_present(dir: &Path) -> bool {
    dir.join(MODEL_FILE).is_file() && dir.join(TOKENIZER_FILE).is_file()
}

/// Temp path for a download: one per process, so parallel `larp`
/// invocations downloading the same file never share (and rename away)
/// each other's temp file. The old fixed `dest.tmp` name raced in CI:
/// the semantic tests run concurrently on a cold model cache, one
/// process renamed `model.onnx.tmp` away while another was still
/// writing, and the loser failed its rename with "No such file or
/// directory". Renames are atomic and the bytes are identical, so two
/// processes landing the same file is harmless.
fn download_tmp_path(dest: &Path) -> PathBuf {
    dest.with_extension(format!("tmp-{}", std::process::id()))
}

fn download_to(url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("download {url}: {e}"))?;
    let mut reader = resp.into_reader();
    let tmp = download_tmp_path(dest);
    let result = (|| -> Result<(), String> {
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        std::io::copy(&mut reader, &mut f).map_err(|e| e.to_string())?;
        f.sync_data().map_err(|e| e.to_string())?;
        drop(f);
        std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Make sure the model files exist in `dir`, downloading them on first
/// use. Returns true when a download happened. Prints the one-time
/// notice **before** any bytes are fetched; never downloads silently.
pub fn ensure_model(dir: &Path) -> Result<bool, String> {
    if model_files_present(dir) {
        return Ok(false);
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    eprintln!(
        "downloading embedding model {MODEL_ID} (~90 MB) to {} — one-time; set LARP_MODEL_DIR to override",
        dir.display()
    );
    download_to(MODEL_URL, &dir.join(MODEL_FILE))?;
    download_to(TOKENIZER_URL, &dir.join(TOKENIZER_FILE))?;
    Ok(true)
}

/// Split content token ids into chunks of at most `max_len` tokens,
/// each wrapped with the CLS/SEP pair. Empty input yields no chunks.
pub fn chunk_ids(ids: &[u32], cls: u32, sep: u32, max_len: usize) -> Vec<Vec<u32>> {
    if ids.is_empty() {
        return Vec::new();
    }
    let window = max_len.saturating_sub(2).max(1);
    ids.chunks(window)
        .map(|w| {
            let mut chunk = Vec::with_capacity(w.len() + 2);
            chunk.push(cls);
            chunk.extend_from_slice(w);
            chunk.push(sep);
            chunk
        })
        .collect()
}

/// Mean pooling over token embeddings, then L2-normalize.
/// `hidden` is row-major [seq_len, dim]. Pure function, unit-tested.
pub fn mean_pool_normalize(hidden: &[f32], seq_len: usize, dim: usize) -> Vec<f32> {
    let mut out = vec![0f32; dim];
    if seq_len == 0 || dim == 0 {
        return out;
    }
    for t in 0..seq_len {
        for d in 0..dim {
            out[d] += hidden[t * dim + d];
        }
    }
    let n = seq_len as f32;
    for v in out.iter_mut() {
        *v /= n;
    }
    let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in out.iter_mut() {
            *v /= norm;
        }
    }
    out
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// The ONNX embedder: HF tokenizer + ort session, held behind a mutex
/// because `Session::run` needs `&mut` while the `Embedder` trait takes
/// `&self` (the MCP server calls it through a shared reference).
pub struct OnnxEmbedder {
    tokenizer: tokenizers::Tokenizer,
    session: Mutex<ort::session::Session>,
    _env: ort::environment::Environment,
    cls_id: u32,
    sep_id: u32,
}

impl OnnxEmbedder {
    /// Load from a directory that already holds `model.onnx` +
    /// `tokenizer.json`. Errors (never downloads) when files are missing
    /// or the model fails to load.
    pub fn load(dir: &Path) -> Result<OnnxEmbedder, String> {
        let model_path = dir.join(MODEL_FILE);
        let tokenizer_path = dir.join(TOKENIZER_FILE);
        if !model_path.is_file() || !tokenizer_path.is_file() {
            return Err(format!(
                "embedding model not found in {} (run with network once, or set LARP_MODEL_DIR to a directory holding model.onnx + tokenizer.json)",
                dir.display()
            ));
        }
        let mut tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("load tokenizer: {e}"))?;
        // The Xenova tokenizer.json ships with padding/truncation to 128
        // tokens enabled. We control both ourselves (chunking does it):
        // without this, encode() returns a fixed 128 ids — mostly [PAD]
        // with attention 1 — and every sentence embeds almost alike.
        // (That exact bug showed up as unrelated cosine 0.72.)
        // Clearing the config cannot fail (it only drops the settings).
        let _ = tokenizer.with_truncation(None);
        let _ = tokenizer.with_padding(None);
        // CLS/SEP come from the tokenizer itself (empty input + specials
        // is [CLS, SEP] for BERT-family models), never hardcoded.
        let specials = tokenizer
            .encode("", true)
            .map_err(|e| format!("tokenizer probe: {e}"))?;
        let special_ids = specials.get_ids();
        if special_ids.len() < 2 {
            return Err("tokenizer did not produce CLS/SEP special tokens".to_string());
        }
        let cls_id = special_ids[0];
        let sep_id = special_ids[special_ids.len() - 1];
        let env = ort::init()
            .with_name("larp")
            .build()
            .map_err(|e| format!("init ONNX Runtime: {e}"))?;
        let session = ort::session::Session::builder(&env)
            .map_err(|e| format!("build ONNX session: {e}"))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("load model: {e}"))?;
        // The fixed model must expose exactly the inputs/outputs we use.
        let in_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();
        for want in ["input_ids", "attention_mask", "token_type_ids"] {
            if !in_names.iter().any(|n| n == want) {
                return Err(format!(
                    "model is missing input '{want}' (has: {in_names:?})"
                ));
            }
        }
        if !session
            .outputs()
            .iter()
            .any(|o| o.name() == "last_hidden_state")
        {
            return Err("model is missing output 'last_hidden_state'".to_string());
        }
        Ok(OnnxEmbedder {
            tokenizer,
            session: Mutex::new(session),
            _env: env,
            cls_id,
            sep_id,
        })
    }

    /// Resolve the model dir, download on first use (with notice), then
    /// load. This is the call-site entry point; on Err the caller falls
    /// back to BM25 and reports the message as a notice.
    pub fn load_or_download() -> Result<OnnxEmbedder, String> {
        let dir = model_dir();
        ensure_model(&dir)?;
        OnnxEmbedder::load(&dir)
    }

    fn embed_chunk(&self, chunk: &[u32]) -> Result<Vec<f32>, String> {
        let n = chunk.len() as i64;
        let ids: Vec<i64> = chunk.iter().map(|&x| x as i64).collect();
        let mask: Vec<i64> = vec![1; chunk.len()];
        let types: Vec<i64> = vec![0; chunk.len()];
        let inputs = ort::inputs! {
            "input_ids" => ort::value::Tensor::from_array((vec![1, n], ids)).map_err(|e| e.to_string())?,
            "attention_mask" => ort::value::Tensor::from_array((vec![1, n], mask)).map_err(|e| e.to_string())?,
            "token_type_ids" => ort::value::Tensor::from_array((vec![1, n], types)).map_err(|e| e.to_string())?,
        };
        let mut session = self.session.lock().map_err(|e| e.to_string())?;
        let outputs = session.run(inputs).map_err(|e| e.to_string())?;
        let value = &outputs["last_hidden_state"];
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if shape.len() != 3 || shape[0] != 1 || shape[1] != n || shape[2] != MODEL_DIM as i64 {
            return Err(format!("unexpected last_hidden_state shape: {shape:?}"));
        }
        Ok(mean_pool_normalize(data, n as usize, MODEL_DIM))
    }
}

impl Embedder for OnnxEmbedder {
    fn model_id(&self) -> &str {
        MODEL_ID
    }

    fn dim(&self) -> usize {
        MODEL_DIM
    }

    fn embed_chunks(&self, text: &str) -> Result<Vec<Vec<f32>>, String> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| format!("tokenize: {e}"))?;
        // Real tokens only: if padding is ever on, the attention mask
        // marks the padded tail, and [PAD] ids must not be embedded.
        let mask = encoding.get_attention_mask();
        let real_len = mask.iter().position(|&m| m == 0).unwrap_or(mask.len());
        let ids = &encoding.get_ids()[..real_len];
        let chunks = chunk_ids(ids, self.cls_id, self.sep_id, MAX_TOKENS_PER_CHUNK);
        let mut out = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            out.push(self.embed_chunk(chunk)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    #[test]
    fn download_tmp_path_is_per_process_and_beside_dest() {
        let dest = PathBuf::from("/tmp/cache/model.onnx");
        let tmp = download_tmp_path(&dest);
        // Not the old shared name, and still next to the destination.
        assert_ne!(tmp, dest.with_extension("tmp"));
        assert_eq!(tmp.parent(), dest.parent());
        assert!(tmp
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("model.tmp-"));
        // Same process, same name: a retry overwrites its own temp file.
        assert_eq!(tmp, download_tmp_path(&dest));
    }

    #[test]
    fn chunk_ids_wraps_and_splits() {
        let ids: Vec<u32> = (0..600).collect();
        let chunks = chunk_ids(&ids, 101, 102, 256);
        assert_eq!(chunks.len(), 3);
        // 254 content tokens per chunk + CLS/SEP.
        assert_eq!(chunks[0].len(), 256);
        assert_eq!(chunks[1].len(), 256);
        assert_eq!(chunks[2].len(), 94);
        for c in &chunks {
            assert_eq!(c[0], 101);
            assert_eq!(*c.last().unwrap(), 102);
        }
        let content: Vec<u32> = chunks
            .iter()
            .flat_map(|c| c[1..c.len() - 1].to_vec())
            .collect();
        assert_eq!(content, ids);
        assert!(chunk_ids(&[], 101, 102, 256).is_empty());
        assert_eq!(chunk_ids(&[7], 101, 102, 256), vec![vec![101, 7, 102]]);
    }

    #[test]
    fn mean_pool_normalize_is_unit_length() {
        // seq 2, dim 2: rows [1,0] and [0,1] -> mean [0.5,0.5] -> normalized.
        let v = mean_pool_normalize(&[1.0, 0.0, 0.0, 1.0], 2, 2);
        let want = std::f32::consts::FRAC_1_SQRT_2;
        assert!((v[0] - want).abs() < 1e-6);
        assert!((v[1] - want).abs() < 1e-6);
        // Zero rows stay zero (no NaN from dividing by zero).
        assert_eq!(mean_pool_normalize(&[0.0, 0.0], 1, 2), vec![0.0, 0.0]);
        assert_eq!(mean_pool_normalize(&[], 0, 2), vec![0.0, 0.0]);
    }

    #[test]
    fn cosine_matches_identical_and_orthogonal() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn resolve_model_dir_prefers_env_over_xdg_over_home() {
        assert_eq!(
            resolve_model_dir(Some("/m"), Some("/x"), Some("/h")),
            PathBuf::from("/m")
        );
        assert_eq!(
            resolve_model_dir(None, Some("/x"), Some("/h")),
            PathBuf::from("/x/larp/models")
        );
        assert_eq!(
            resolve_model_dir(None, None, Some("/h")),
            PathBuf::from("/h/.cache/larp/models")
        );
        assert_eq!(
            resolve_model_dir(Some(""), None, Some("/h")),
            PathBuf::from("/h/.cache/larp/models")
        );
    }

    /// The real model, loaded once per process. Uses LARP_MODEL_DIR when
    /// set (local runs point it at the prototype's model dir); otherwise
    /// downloads with the one-time notice (CI has network). Fails loudly
    /// when the model cannot be obtained — a silent pass would hide a
    /// broken embedding path.
    fn shared_embedder() -> &'static OnnxEmbedder {
        static EMBEDDER: OnceLock<Result<OnnxEmbedder, String>> = OnceLock::new();
        let result = EMBEDDER.get_or_init(|| {
            let dir = model_dir();
            ensure_model(&dir).and_then(|_| OnnxEmbedder::load(&dir))
        });
        match result {
            Ok(e) => e,
            Err(msg) => panic!("real-model test could not load the embedder: {msg}"),
        }
    }

    #[test]
    fn onnx_embedder_paraphrase_beats_unrelated() {
        let e = shared_embedder();
        assert_eq!(e.dim(), 384);
        let a = e.embed_chunks("the cat sits on the mat").unwrap();
        let b = e.embed_chunks("a feline rests upon the rug").unwrap();
        let c = e
            .embed_chunks("quantum chromodynamics lattice gauge theory")
            .unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(c.len(), 1);
        let sim_ab = cosine(&a[0], &b[0]);
        let sim_ac = cosine(&a[0], &c[0]);
        let sim_aa = cosine(&a[0], &a[0]);
        assert!(
            (sim_aa - 1.0).abs() < 1e-5,
            "identical text must embed to cosine ~1, got {sim_aa}"
        );
        assert!(
            sim_ab > 0.5,
            "paraphrase similarity too low: {sim_ab} (expected > 0.5)"
        );
        assert!(
            sim_ac < 0.3,
            "unrelated similarity too high: {sim_ac} (expected < 0.3)"
        );
        assert!(sim_ab > sim_ac, "paraphrase must beat unrelated");
    }

    #[test]
    fn onnx_embedder_chunks_long_text() {
        let e = shared_embedder();
        let long = "word ".repeat(400);
        let chunks = e.embed_chunks(&long).unwrap();
        // 400 words is ~400 tokens -> 2 chunks of <=256.
        assert_eq!(chunks.len(), 2);
        for v in &chunks {
            assert_eq!(v.len(), 384);
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-5, "chunk vectors are L2-normalized");
        }
    }
}
