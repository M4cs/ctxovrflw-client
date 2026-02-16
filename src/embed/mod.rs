use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::config::Config;

const MODEL_FILENAME: &str = "all-MiniLM-L6-v2-q8.onnx";
const TOKENIZER_FILENAME: &str = "tokenizer.json";
pub const EMBEDDING_DIM: usize = 384;

/// Global singleton embedder — loaded once, shared across HTTP, sync, MCP, CLI.
static GLOBAL_EMBEDDER: OnceLock<Arc<Mutex<Embedder>>> = OnceLock::new();

/// Get or initialize the global embedder singleton.
/// First call loads the model; subsequent calls return the cached instance.
pub fn get_or_init() -> Result<Arc<Mutex<Embedder>>> {
    if let Some(emb) = GLOBAL_EMBEDDER.get() {
        return Ok(emb.clone());
    }
    let emb = Embedder::new()?;
    let arc = Arc::new(Mutex::new(emb));
    Ok(GLOBAL_EMBEDDER.get_or_init(|| arc.clone()).clone())
}

/// Embedding model using ONNX Runtime + all-MiniLM-L6-v2
/// Only available when compiled with `--features onnx`
pub struct Embedder {
    #[cfg(feature = "onnx")]
    session: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
}

impl Embedder {
    pub fn new() -> Result<Self> {
        // Auto-set ORT_DYLIB_PATH if not set — look in common locations
        #[cfg(feature = "onnx")]
        Self::auto_discover_ort();

        let model_dir = Config::model_dir()?;
        let tokenizer_path = model_dir.join(TOKENIZER_FILENAME);

        if !tokenizer_path.exists() {
            anyhow::bail!(
                "Model files not found in {}. Run `ctxovrflw init` first.",
                model_dir.display()
            );
        }

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {e}"))?;

        #[cfg(feature = "onnx")]
        {
            let model_path = model_dir.join(MODEL_FILENAME);
            if !model_path.exists() {
                anyhow::bail!("ONNX model not found at {}", model_path.display());
            }

            // Use catch_unwind around the entire ONNX stack.
            // ort's load-dynamic feature can panic during dylib loading
            // (lazy static init), not just during session creation.
            // This catches panics at any level.
            let model_path_clone = model_path.clone();
            let session_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let b = ort::session::Session::builder().map_err(|e| anyhow::anyhow!("ONNX Session::builder failed: {e:?}"))?;
                let b2 = b.with_intra_threads(2).map_err(|e| anyhow::anyhow!("ONNX with_intra_threads failed: {e:?}"))?;
                b2.commit_from_file(&model_path_clone).map_err(|e| anyhow::anyhow!(
                    "ONNX commit_from_file failed: {e:?} (model: {}, ORT_DYLIB_PATH={})",
                    model_path_clone.display(),
                    std::env::var("ORT_DYLIB_PATH").unwrap_or_else(|_| "<not set>".into())
                ))
            }));

            let session = match session_result {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => return Err(e),
                Err(_) => anyhow::bail!(
                    "ONNX runtime failed to load. Ensure ORT_DYLIB_PATH is set correctly. \
                     Semantic search will be unavailable until this is fixed."
                ),
            };

            Ok(Self { session, tokenizer })
        }

        #[cfg(not(feature = "onnx"))]
        {
            Ok(Self { tokenizer })
        }
    }

    /// Generate embedding for a text string. Returns 384-dim f32 vector.
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        #[cfg(feature = "onnx")]
        {
            self.embed_onnx(text)
        }

        #[cfg(not(feature = "onnx"))]
        {
            Ok(tokenizer_hash_embed(&self.tokenizer, text))
        }
    }

    /// Check if ONNX embedding is available (vs hash fallback)
    #[allow(dead_code)]
    pub fn is_onnx(&self) -> bool {
        cfg!(feature = "onnx")
    }

    #[cfg(feature = "onnx")]
    fn embed_onnx(&mut self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {e}"))?;

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| m as i64)
            .collect();
        let token_type_ids: Vec<i64> = encoding
            .get_type_ids()
            .iter()
            .map(|&t| t as i64)
            .collect();

        let seq_len = input_ids.len();
        let shape: Vec<usize> = vec![1, seq_len];

        let ids_tensor =
            ort::value::TensorRef::from_array_view((&shape as &[usize], &*input_ids))?;
        let mask_tensor =
            ort::value::TensorRef::from_array_view((&shape as &[usize], &*attention_mask))?;
        let type_tensor =
            ort::value::TensorRef::from_array_view((&shape as &[usize], &*token_type_ids))?;

        let outputs =
            self.session
                .run(ort::inputs![ids_tensor, mask_tensor, type_tensor])?;

        let (_output_shape, output_data) = outputs[0].try_extract_tensor::<f32>()?;

        // Mean pooling over token dimension
        let mask = encoding.get_attention_mask();
        let mut pooled = vec![0.0f32; EMBEDDING_DIM];
        let mut mask_sum = 0.0f32;

        for (i, &m) in mask.iter().enumerate() {
            let m = m as f32;
            mask_sum += m;
            for j in 0..EMBEDDING_DIM {
                pooled[j] += output_data[i * EMBEDDING_DIM + j] * m;
            }
        }

        for v in &mut pooled {
            *v /= mask_sum.max(1e-9);
        }

        // L2 normalize
        let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut pooled {
                *v /= norm;
            }
        }

        Ok(pooled)
    }

    pub fn model_path() -> Result<PathBuf> {
        Ok(Config::model_dir()?.join(MODEL_FILENAME))
    }

    #[allow(dead_code)]
    pub fn tokenizer_path() -> Result<PathBuf> {
        Ok(Config::model_dir()?.join(TOKENIZER_FILENAME))
    }

    /// Auto-discover ONNX runtime library if ORT_DYLIB_PATH isn't set.
    /// Searches: next to binary, ~/.ctxovrflw/lib/, ~/.local/lib/, /usr/local/lib/
    #[cfg(feature = "onnx")]
    fn auto_discover_ort() {
        if std::env::var("ORT_DYLIB_PATH").is_ok() {
            return; // Already set
        }

        let lib_name = if cfg!(target_os = "macos") {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        };

        let mut search_paths: Vec<PathBuf> = Vec::new();

        // 1. Next to the binary
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                search_paths.push(dir.join(lib_name));
            }
        }

        // 2. ~/.ctxovrflw/ locations
        if let Some(home) = dirs::home_dir() {
            search_paths.push(home.join(".ctxovrflw").join("bin").join(lib_name));
            search_paths.push(home.join(".ctxovrflw").join("lib").join(lib_name));
            // 3. ~/.local/
            search_paths.push(home.join(".local").join("lib").join(lib_name));
            search_paths.push(home.join(".local").join("bin").join(lib_name));
            // 4. ~/.cargo/bin/ (common for Rust devs)
            search_paths.push(home.join(".cargo").join("bin").join(lib_name));
            // 5. Downloaded ORT release
            let ort_version = "1.23.0";
            let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x64" };
            let os = if cfg!(target_os = "macos") { "osx" } else { "linux" };
            search_paths.push(home.join(format!("onnxruntime-{os}-{arch}-{ort_version}")).join("lib").join(lib_name));
        }

        // 6. System paths
        search_paths.push(PathBuf::from("/usr/local/lib").join(lib_name));
        search_paths.push(PathBuf::from("/usr/lib").join(lib_name));

        for path in &search_paths {
            if path.exists() {
                // SAFETY: called once at startup before any threads use the env var
                unsafe { std::env::set_var("ORT_DYLIB_PATH", path); }
                tracing::info!("Auto-discovered ONNX runtime at {}", path.display());
                return;
            }
        }
    }
}

/// Tokenizer-aware hash embedding. Uses actual token IDs for better
/// semantic distribution than raw byte hashing. Used in non-ONNX builds.
#[allow(dead_code)]
fn tokenizer_hash_embed(tokenizer: &tokenizers::Tokenizer, text: &str) -> Vec<f32> {
    let mut embedding = vec![0.0f32; EMBEDDING_DIM];

    if let Ok(encoding) = tokenizer.encode(text, true) {
        for (i, &id) in encoding.get_ids().iter().enumerate() {
            let base = (id as usize * 7) % EMBEDDING_DIM;
            for k in 0..3 {
                let idx = (base + k * 131) % EMBEDDING_DIM;
                let sign = if (id as usize + k) % 2 == 0 { 1.0 } else { -1.0 };
                let decay = 1.0 / (1.0 + i as f32 * 0.1);
                embedding[idx] += sign * decay;
            }
        }
    }

    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut embedding { *v /= norm; }
    }
    embedding
}

/// Simple hash embedding for when no tokenizer is available (testing/fallback).
#[allow(dead_code)]
pub fn hash_embed(text: &str) -> Vec<f32> {
    let mut embedding = vec![0.0f32; EMBEDDING_DIM];
    let bytes = text.as_bytes();
    for (i, chunk) in bytes.chunks(2).enumerate() {
        let idx = i % EMBEDDING_DIM;
        let val = chunk
            .iter()
            .fold(0u32, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u32));
        embedding[idx] += (val as f32 / u32::MAX as f32) * 2.0 - 1.0;
    }
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut embedding {
            *v /= norm;
        }
    }
    embedding
}
