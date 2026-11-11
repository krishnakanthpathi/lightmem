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
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|text| self.embed(text)).collect()
    }
}

pub struct OnnxEmbeddingProvider {
    model: Mutex<TextEmbedding>,
    name: String,
}

impl OnnxEmbeddingProvider {
    /// Initialize with a known model name ("bge-small", "minilm", "nomic")
    pub fn new(model_name: Option<&str>) -> Result<Self> {
        let (selected_model, display_name) = match model_name {
            Some("minilm") | Some("all-minilm-l6-v2") | Some("Xenova/all-MiniLM-L6-v2") => {
                (EmbeddingModel::AllMiniLML6V2, "Xenova/all-MiniLM-L6-v2")
            }
            Some("nomic") | Some("nomic-embed-text") | Some("nomic-ai/nomic-embed-text-v1.5") => (
                EmbeddingModel::NomicEmbedTextV15,
                "nomic-ai/nomic-embed-text-v1.5",
            ),
            None | Some("bge-small" | "bge-small-en-v1.5" | "Xenova/bge-small-en-v1.5") => {
                (EmbeddingModel::BGESmallENV15, "Xenova/bge-small-en-v1.5")
            }
            Some(other) => anyhow::bail!("Unknown ONNX model: {}", other),
        };

        let cache_dir = crate::config::LightMemConfig::config_dir().join("models");
        let _ = std::fs::create_dir_all(&cache_dir);
        let options = TextInitOptions::new(selected_model)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(true);
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
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut guard = self
            .model
            .lock()
            .map_err(|e| anyhow::anyhow!("Mutex lock error: {}", e))?;
        guard
            .embed(texts, Some(32))
            .map_err(|e| anyhow::anyhow!("ONNX batch inference error: {}", e))
    }
}

#[derive(Clone)]
pub struct OllamaEmbeddingProvider {
    pub url: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl OllamaEmbeddingProvider {
    pub fn new(url: String, model: String) -> Self {
        Self::with_api_key(url, model, None)
    }

    pub fn with_api_key(url: String, model: String, api_key: Option<String>) -> Self {
        let clean_url = url.trim_end_matches('/').to_string();
        let clean_key = api_key
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty() && !k.eq_ignore_ascii_case("none"));
        Self {
            url: clean_url,
            model,
            api_key: clean_key,
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

        let mut req = ureq::post(&endpoint).timeout(std::time::Duration::from_secs(30));
        if let Some(ref key) = self.api_key {
            req = req.set(
                "Authorization",
                &crate::models::config::format_auth_header(key),
            );
        }
        let resp_result = req.send_json(&body);

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
            Err(ureq::Error::Status(404, resp)) => {
                let err_body = resp.into_string().unwrap_or_default();
                let lower_err = err_body.to_lowercase();
                if lower_err.contains("model")
                    && (lower_err.contains("not found") || lower_err.contains("try pulling"))
                {
                    anyhow::bail!(
                        "Ollama model '{}' not found at {}. Have you pulled it with `ollama pull {}`? Details: {}",
                        self.model,
                        self.url,
                        self.model,
                        err_body.trim()
                    );
                }

                // If /api/embeddings failed with 404 because endpoint is missing, try /api/embed (newer Ollama format)
                let alt_endpoint = format!("{}/api/embed", self.url);
                #[derive(Serialize)]
                struct AltRequest<'a> {
                    model: &'a str,
                    input: &'a str,
                }

                let mut alt_req =
                    ureq::post(&alt_endpoint).timeout(std::time::Duration::from_secs(30));
                if let Some(ref key) = self.api_key {
                    alt_req = alt_req.set(
                        "Authorization",
                        &crate::models::config::format_auth_header(key),
                    );
                }
                let alt_resp_result = alt_req.send_json(&AltRequest {
                    model: &self.model,
                    input: text,
                });

                let alt_resp = match alt_resp_result {
                    Ok(r) => r,
                    Err(ureq::Error::Status(404, alt_err_resp)) => {
                        let alt_err_body = alt_err_resp.into_string().unwrap_or_default();
                        let alt_lower = alt_err_body.to_lowercase();
                        if alt_lower.contains("model")
                            && (alt_lower.contains("not found")
                                || alt_lower.contains("try pulling"))
                        {
                            anyhow::bail!(
                                "Ollama model '{}' not found at {}. Have you pulled it with `ollama pull {}`? Details: {}",
                                self.model,
                                self.url,
                                self.model,
                                alt_err_body.trim()
                            );
                        }
                        anyhow::bail!(
                            "Failed to call Ollama at {} and {}: endpoint not found (404). Details: {}",
                            endpoint,
                            alt_endpoint,
                            if !alt_err_body.trim().is_empty() {
                                alt_err_body.trim()
                            } else {
                                err_body.trim()
                            }
                        );
                    }
                    Err(e) => {
                        return Err(e).with_context(|| {
                            format!(
                                "Failed to call Ollama at {} (and {})",
                                endpoint, alt_endpoint
                            )
                        });
                    }
                };

                let parsed: OllamaEmbedResponse = alt_resp.into_json()?;
                if let Some(mut embs) = parsed.embeddings {
                    if !embs.is_empty() {
                        return Ok(embs.remove(0));
                    }
                }
                anyhow::bail!("Ollama returned invalid response format");
            }
            Err(e) => Err(e).with_context(|| format!("Failed to call Ollama at {}", endpoint)),
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
        anyhow::ensure!(
            !text.trim().is_empty(),
            "Cannot generate embedding for empty or whitespace-only input"
        );
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
        let sim = dot / denom;
        if sim.is_finite() {
            sim.clamp(-1.0, 1.0)
        } else {
            0.0
        }
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

pub fn validate_vector(vector: &[f32]) -> Result<()> {
    anyhow::ensure!(
        !vector.is_empty() && vector.iter().all(|v| v.is_finite()),
        "Embedding must be nonempty and contain only finite values"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_clamping_and_nan() {
        // Normal vectors
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 1.0);

        // Opposite vectors
        let c = vec![-1.0, 0.0];
        assert_eq!(cosine_similarity(&a, &c), -1.0);

        // Vector containing NaN should result in 0.0
        let nan_vec = vec![f32::NAN, 1.0];
        assert_eq!(cosine_similarity(&a, &nan_vec), 0.0);

        // Zero vectors
        let zero = vec![0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &zero), 0.0);

        // Mismatched lengths
        let short = vec![1.0];
        assert_eq!(cosine_similarity(&a, &short), 0.0);
    }

    #[test]
    fn test_hash_embedding_provider_rejects_empty_and_whitespace() {
        let provider = HashEmbeddingProvider;
        assert!(provider.embed("").is_err());
        assert!(provider.embed("   \t\n  ").is_err());
        assert!(provider.embed("valid content").is_ok());
    }
}
