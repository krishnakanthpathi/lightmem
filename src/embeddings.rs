use anyhow::{Context, Result};
use fastembed::{
    EmbeddingModel, InitOptionsUserDefined, TextEmbedding, TextInitOptions, TokenizerFiles,
    UserDefinedEmbeddingModel,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub trait EmbeddingProvider: Send + Sync {
    fn name(&self) -> &str;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

pub struct OnnxEmbeddingProvider {
    model: Mutex<TextEmbedding>,
    name: String,
}

impl OnnxEmbeddingProvider {
    /// Initialize with a known model name ("bge-small", "minilm", "nomic")
    pub fn new(model_name: Option<&str>) -> Result<Self> {
        let (selected_model, display_name) = match model_name {
            Some("minilm") | Some("all-minilm-l6-v2") => {
                (EmbeddingModel::AllMiniLML6V2, "all-minilm-l6-v2")
            }
            Some("nomic") | Some("nomic-embed-text") => {
                (EmbeddingModel::NomicEmbedTextV15, "nomic-embed-text")
            }
            _ => (EmbeddingModel::BGESmallENV15, "bge-small-en-v1.5"),
        };

        let options = TextInitOptions::new(selected_model).with_show_download_progress(true);
        let model = TextEmbedding::try_new(options)
            .map_err(|e| anyhow::anyhow!("Failed to initialize ONNX embedding model: {}", e))?;

        Ok(Self {
            model: Mutex::new(model),
            name: format!("onnx:{}", display_name),
        })
    }

    /// Load custom ONNX embedding model from local directory containing model.onnx and tokenizer.json
    pub fn new_custom_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dir = dir.as_ref();
        let onnx_path = if dir.join("model.onnx").exists() {
            dir.join("model.onnx")
        } else if dir.join("model_quantized.onnx").exists() {
            dir.join("model_quantized.onnx")
        } else {
            anyhow::bail!("No model.onnx or model_quantized.onnx found in {:?}", dir);
        };

        let onnx_file = std::fs::read(&onnx_path)
            .with_context(|| format!("Failed to read ONNX model at {:?}", onnx_path))?;

        let tokenizer_file = std::fs::read(dir.join("tokenizer.json"))
            .with_context(|| format!("Failed to read tokenizer.json in {:?}", dir))?;

        let config_file = std::fs::read(dir.join("config.json")).unwrap_or_default();

        let special_tokens_map_file =
            std::fs::read(dir.join("special_tokens_map.json")).unwrap_or_default();

        let tokenizer_config_file =
            std::fs::read(dir.join("tokenizer_config.json")).unwrap_or_default();

        let tokenizer_files = TokenizerFiles {
            tokenizer_file,
            config_file,
            special_tokens_map_file,
            tokenizer_config_file,
        };

        let user_model = UserDefinedEmbeddingModel::new(onnx_file, tokenizer_files);
        let model =
            TextEmbedding::try_new_from_user_defined(user_model, InitOptionsUserDefined::default())
                .map_err(|e| {
                    anyhow::anyhow!("Failed to load custom ONNX model from {:?}: {}", dir, e)
                })?;

        Ok(Self {
            model: Mutex::new(model),
            name: format!("onnx:custom:{}", dir.display()),
        })
    }
}

impl EmbeddingProvider for OnnxEmbeddingProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut guard = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Mutex lock error: {}", e))?;
        let embs = guard
            .embed(vec![text], None)
            .map_err(|e| anyhow::anyhow!("ONNX inference error: {}", e))?;
        embs.into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("ONNX returned empty embedding"))
    }
}

#[derive(Clone)]
pub struct OllamaEmbeddingProvider {
    pub url: String,
    pub model: String,
}

impl OllamaEmbeddingProvider {
    pub fn new(url: String, model: String) -> Self {
        let clean_url = url.trim_end_matches('/').to_string();
        Self {
            url: clean_url,
            model,
        }
    }
}

#[derive(Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embedding: Option<Vec<f32>>,
    embeddings: Option<Vec<Vec<f32>>>,
}

impl EmbeddingProvider for OllamaEmbeddingProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Try /api/embeddings first
        let endpoint = format!("{}/api/embeddings", self.url);
        let body = OllamaEmbedRequest {
            model: &self.model,
            prompt: text,
        };

        let resp_result = ureq::post(&endpoint)
            .timeout(std::time::Duration::from_secs(30))
            .send_json(&body);

        match resp_result {
            Ok(resp) => {
                let parsed: OllamaEmbedResponse = resp.into_json().with_context(|| {
                    format!("Failed to parse Ollama response from {}", endpoint)
                })?;

                if let Some(emb) = parsed.embedding {
                    return Ok(emb);
                }
                if let Some(mut embs) = parsed.embeddings {
                    if !embs.is_empty() {
                        return Ok(embs.remove(0));
                    }
                }
                anyhow::bail!("Ollama returned 200 OK but embedding array was empty");
            }
            Err(e) => {
                // If /api/embeddings failed with 404, try /api/embed (newer Ollama format)
                let alt_endpoint = format!("{}/api/embed", self.url);
                #[derive(Serialize)]
                struct AltRequest<'a> {
                    model: &'a str,
                    input: &'a str,
                }

                let alt_resp = ureq::post(&alt_endpoint)
                    .timeout(std::time::Duration::from_secs(30))
                    .send_json(&AltRequest {
                        model: &self.model,
                        input: text,
                    })
                    .with_context(|| {
                        format!(
                            "Failed to call Ollama at {} (and {}) : {}",
                            endpoint, alt_endpoint, e
                        )
                    })?;

                let parsed: OllamaEmbedResponse = alt_resp.into_json()?;
                if let Some(mut embs) = parsed.embeddings {
                    if !embs.is_empty() {
                        return Ok(embs.remove(0));
                    }
                }
                anyhow::bail!("Ollama returned invalid response format");
            }
        }
    }
}

/// Fallback deterministic 384-dimensional bag-of-words / character n-gram embedding
/// Used when offline, testing, or before an external provider is configured.
#[derive(Clone, Default)]
pub struct HashEmbeddingProvider;

impl EmbeddingProvider for HashEmbeddingProvider {
    fn name(&self) -> &str {
        "hash-fallback"
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        const DIMS: usize = 384;
        let mut vec = vec![0.0f32; DIMS];

        // Token n-gram hashing
        for word in text.split_whitespace() {
            let lower = word.to_lowercase();
            let hash = simple_hash(&lower);
            let idx = (hash as usize) % DIMS;
            let sign = if (hash >> 16) & 1 == 1 { 1.0 } else { -1.0 };
            vec[idx] += sign;

            // Bigram character chunks
            let chars: Vec<char> = lower.chars().collect();
            for chunk in chars.windows(2) {
                let s: String = chunk.iter().collect();
                let ch_hash = simple_hash(&s);
                let c_idx = (ch_hash as usize) % DIMS;
                vec[c_idx] += 0.5;
            }
        }

        // L2 normalize
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in vec.iter_mut() {
                *val /= norm;
            }
        }

        Ok(vec)
    }
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
