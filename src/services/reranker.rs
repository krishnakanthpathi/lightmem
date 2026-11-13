use crate::models::{LightMemConfig, MemoryRecord, ScoredMemory};
use anyhow::{Context, Result};
use ndarray::Array2;
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Value,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokenizers::{Tokenizer, TruncationParams, TruncationStrategy};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerResult {
    pub answer: String,
    pub selected_memory: Option<MemoryRecord>,
    pub confidence: f32,
    pub reranker_used: String,
}

pub trait Reranker: Send + Sync {
    fn name(&self) -> &str;
    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult>;
    fn merge_conflict(&self, older: &MemoryRecord, newer: &MemoryRecord) -> Result<MemoryRecord> {
        default_merge_conflict(older, newer)
    }
}

/// Mode 1: Fast / Instant Top-1 Reranker
/// Returns Rank-1 candidate from Hybrid Search with 0ms latency and 0MB extra RAM.
pub struct Top1Reranker;

impl Reranker for Top1Reranker {
    fn name(&self) -> &str {
        "top1"
    }

    fn answer(&self, _question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        let Some(best) = candidates.first() else {
            return Ok(AnswerResult {
                answer: "No relevant memories found to answer this question.".to_string(),
                selected_memory: None,
                confidence: 0.0,
                reranker_used: "top1".to_string(),
            });
        };

        if best.bm25_rank.is_none() && best.score < 0.35 {
            return Ok(no_evidence("top1"));
        }

        Ok(AnswerResult {
            answer: best.memory.content.clone(),
            selected_memory: Some(best.memory.clone()),
            confidence: best.memory.confidence,
            reranker_used: "top1".to_string(),
        })
    }
}

struct OnnxQaEngine {
    tokenizer: Tokenizer,
    session: Mutex<Session>,
    need_token_type_ids: bool,
    provider_label: String,
}

struct QaModelPreset {
    slug: &'static str,
    model_url: &'static str,
    tokenizer_url: &'static str,
}

fn resolve_qa_preset(spec: &str) -> Option<QaModelPreset> {
    match spec.trim().to_lowercase().as_str() {
        ""
        | "onnx"
        | "qa"
        | "precision"
        | "minilm"
        | "minilm-squad2"
        | "deepset/minilm-uncased-squad2"
        | "lquint/minilm-uncased-squad2-onnx" => Some(QaModelPreset {
            slug: "minilm-squad2",
            model_url: "https://huggingface.co/lquint/minilm-uncased-squad2-onnx/resolve/main/model.onnx",
            tokenizer_url:
                "https://huggingface.co/lquint/minilm-uncased-squad2-onnx/resolve/main/tokenizer.json",
        }),
        "tinyroberta"
        | "tinyroberta-squad2"
        | "deepset/tinyroberta-squad2"
        | "onnx-community/tinyroberta-squad2-onnx" => Some(QaModelPreset {
            slug: "tinyroberta-squad2",
            model_url:
                "https://huggingface.co/onnx-community/tinyroberta-squad2-ONNX/resolve/main/onnx/model.onnx",
            tokenizer_url:
                "https://huggingface.co/onnx-community/tinyroberta-squad2-ONNX/resolve/main/tokenizer.json",
        }),
        _ => None,
    }
}

fn download_file_atomic(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    let response = ureq::get(url)
        .timeout(Duration::from_secs(300))
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to download {}: {}", url, e))?;

    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .with_context(|| format!("Failed reading stream from {}", url))?;

    anyhow::ensure!(!bytes.is_empty(), "Downloaded empty file from {}", url);

    let tmp_path = dest.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp_path, &bytes)
        .with_context(|| format!("Failed writing temporary file {}", tmp_path.display()))?;
    if let Err(err) = std::fs::rename(&tmp_path, dest) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err.into());
    }
    Ok(())
}

/// Mode 2: Pure-Rust ONNX SQuAD-2.0 Extractive QA Reranker (`minilm-squad2` / `tinyroberta-squad2` / custom dir)
#[derive(Debug, Clone, Default)]
pub struct OnnxQaReranker {
    model_spec: Option<String>,
}

impl OnnxQaReranker {
    pub fn new(model_spec: Option<String>) -> Self {
        Self { model_spec }
    }

    pub fn provider_label(&self) -> String {
        let env_spec = std::env::var("LIGHTMEM_QA_MODEL").ok();
        let effective_spec = self
            .model_spec
            .as_deref()
            .or(env_spec.as_deref())
            .unwrap_or("minilm-squad2");
        let clean_spec = effective_spec
            .strip_prefix("onnx:")
            .unwrap_or(effective_spec)
            .trim();
        if let Some(preset) = resolve_qa_preset(clean_spec) {
            preset.slug.to_string()
        } else {
            Path::new(clean_spec)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("onnx-qa")
                .to_string()
        }
    }

    /// Ensure the specified ONNX QA model (`minilm-squad2` or `tinyroberta-squad2` or custom path) is present on disk.
    pub fn ensure_model_downloaded(model_spec: Option<&str>) -> Result<(PathBuf, String)> {
        let raw_spec = model_spec
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("minilm-squad2");
        let clean_spec = raw_spec.strip_prefix("onnx:").unwrap_or(raw_spec).trim();

        let custom_path = Path::new(clean_spec);
        if custom_path.is_dir() {
            let onnx_file = if custom_path.join("model.onnx").exists() {
                custom_path.join("model.onnx")
            } else if custom_path.join("model_quantized.onnx").exists() {
                custom_path.join("model_quantized.onnx")
            } else {
                anyhow::bail!(
                    "Custom ONNX QA directory '{}' is missing model.onnx",
                    custom_path.display()
                );
            };
            anyhow::ensure!(
                custom_path.join("tokenizer.json").exists(),
                "Custom ONNX QA directory '{}' is missing tokenizer.json",
                custom_path.display()
            );
            let label = custom_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("onnx-qa")
                .to_string();
            let _ = onnx_file;
            return Ok((custom_path.to_path_buf(), label));
        }

        let preset = resolve_qa_preset(clean_spec).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown ONNX QA model '{}'. Choose 'minilm-squad2', 'tinyroberta-squad2', or a local directory path.",
                clean_spec
            )
        })?;

        let primary_dir = LightMemConfig::config_dir()
            .join("models")
            .join("qa")
            .join(preset.slug);

        // If running inside an isolated test LIGHTMEM_CONFIG_DIR, check ~/.lightmem/models/qa/<slug> first to reuse cache
        if !primary_dir.join("model.onnx").exists() {
            if let Some(home) = dirs::home_dir() {
                let global_dir = home
                    .join(".lightmem")
                    .join("models")
                    .join("qa")
                    .join(preset.slug);
                if global_dir.join("model.onnx").exists()
                    && global_dir.join("tokenizer.json").exists()
                {
                    return Ok((global_dir, preset.slug.to_string()));
                }
            }
        }

        let model_path = primary_dir.join("model.onnx");
        let tokenizer_path = primary_dir.join("tokenizer.json");

        let tokenizer_valid = tokenizer_path
            .metadata()
            .map(|m| m.len() > 1_000)
            .unwrap_or(false);
        if !tokenizer_valid {
            eprintln!("◈ Downloading ONNX QA tokenizer ({})...", preset.slug);
            download_file_atomic(preset.tokenizer_url, &tokenizer_path)?;
        }
        let model_valid = model_path
            .metadata()
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false);
        if !model_valid {
            eprintln!("◈ Downloading ONNX QA model ({})...", preset.slug);
            download_file_atomic(preset.model_url, &model_path)?;
        }

        Ok((primary_dir, preset.slug.to_string()))
    }

    fn get_or_load_engine(&self) -> Result<Arc<OnnxQaEngine>> {
        static ENGINES: OnceLock<Mutex<HashMap<String, Arc<OnnxQaEngine>>>> = OnceLock::new();
        let cache = ENGINES.get_or_init(|| Mutex::new(HashMap::new()));

        let env_spec = std::env::var("LIGHTMEM_QA_MODEL").ok();
        let effective_spec = self
            .model_spec
            .as_deref()
            .or(env_spec.as_deref())
            .unwrap_or("minilm-squad2");

        let (dir, label) = Self::ensure_model_downloaded(Some(effective_spec))?;
        let cache_key = dir.display().to_string();

        {
            let guard = cache
                .lock()
                .map_err(|e| anyhow::anyhow!("QA engine cache lock poisoned: {}", e))?;
            if let Some(existing) = guard.get(&cache_key) {
                return Ok(Arc::clone(existing));
            }
        }

        let onnx_path = if dir.join("model.onnx").exists() {
            dir.join("model.onnx")
        } else {
            dir.join("model_quantized.onnx")
        };
        let tokenizer_path = dir.join("tokenizer.json");

        let mut tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            anyhow::anyhow!("Failed to load tokenizer at {:?}: {}", tokenizer_path, e)
        })?;
        let _ = tokenizer.with_truncation(Some(TruncationParams {
            max_length: 384,
            strategy: TruncationStrategy::OnlySecond,
            ..Default::default()
        }));

        let threads = std::thread::available_parallelism()
            .map(|n| n.get().min(4))
            .unwrap_or(2);
        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("ORT session builder error: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow::anyhow!("ORT optimization level error: {}", e))?
            .with_intra_threads(threads)
            .map_err(|e| anyhow::anyhow!("ORT intra threads error: {}", e))?
            .commit_from_file(&onnx_path)
            .map_err(|e| {
                anyhow::anyhow!("Failed to load ONNX QA model at {:?}: {}", onnx_path, e)
            })?;

        let need_token_type_ids = session
            .inputs()
            .iter()
            .any(|input| input.name() == "token_type_ids");

        let engine = Arc::new(OnnxQaEngine {
            tokenizer,
            session: Mutex::new(session),
            need_token_type_ids,
            provider_label: label,
        });

        let mut guard = cache
            .lock()
            .map_err(|e| anyhow::anyhow!("QA engine cache lock poisoned: {}", e))?;
        guard.insert(cache_key, Arc::clone(&engine));
        Ok(engine)
    }

    fn extract_span(
        engine: &OnnxQaEngine,
        question: &str,
        context: &str,
    ) -> Result<Option<(String, f32)>> {
        let encoding = engine
            .tokenizer
            .encode((question, context), true)
            .map_err(|e| anyhow::anyhow!("QA tokenization failed: {}", e))?;

        let seq_len = encoding.len();
        if seq_len == 0 {
            return Ok(None);
        }

        let ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect();
        let type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&x| x as i64).collect();

        let input_ids = Array2::from_shape_vec((1, seq_len), ids)?;
        let attention_mask = Array2::from_shape_vec((1, seq_len), mask)?;

        let mut session_inputs = ort::inputs![
            "input_ids" => Value::from_array(input_ids)?,
            "attention_mask" => Value::from_array(attention_mask)?,
        ];
        if engine.need_token_type_ids {
            let token_type_ids = Array2::from_shape_vec((1, seq_len), type_ids)?;
            session_inputs.push((
                "token_type_ids".into(),
                Value::from_array(token_type_ids)?.into(),
            ));
        }

        let mut session = engine
            .session
            .lock()
            .map_err(|e| anyhow::anyhow!("ORT session mutex poisoned: {}", e))?;
        let outputs = session
            .run(session_inputs)
            .map_err(|e| anyhow::anyhow!("ORT QA inference failed: {}", e))?;

        let start_logits = outputs
            .get("start_logits")
            .ok_or_else(|| anyhow::anyhow!("ONNX QA model missing 'start_logits' output"))?
            .try_extract_array::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to extract start_logits: {}", e))?;
        let end_logits = outputs
            .get("end_logits")
            .ok_or_else(|| anyhow::anyhow!("ONNX QA model missing 'end_logits' output"))?
            .try_extract_array::<f32>()
            .map_err(|e| anyhow::anyhow!("Failed to extract end_logits: {}", e))?;

        let cls_logit = start_logits[[0, 0]] + end_logits[[0, 0]];
        let seq_ids = encoding.get_sequence_ids();
        let offsets = encoding.get_offsets();

        let ctx_indices: Vec<usize> = seq_ids
            .iter()
            .enumerate()
            .filter_map(|(idx, sid)| (*sid == Some(1)).then_some(idx))
            .collect();

        if ctx_indices.is_empty() {
            return Ok(None);
        }

        let mut best_score = f32::NEG_INFINITY;
        let mut best_span = (ctx_indices[0], ctx_indices[0]);

        for &i in &ctx_indices {
            let s_val = start_logits[[0, i]];
            for &j in &ctx_indices {
                if j < i || j - i + 1 > 20 {
                    continue;
                }
                let score = s_val + end_logits[[0, j]];
                if score > best_score {
                    best_score = score;
                    best_span = (i, j);
                }
            }
        }

        let margin = best_score - cls_logit;
        if !margin.is_finite() {
            return Ok(None);
        }

        let (start_byte, _) = offsets[best_span.0];
        let (_, end_byte) = offsets[best_span.1];
        let Some(raw_slice) = context.get(start_byte..end_byte) else {
            return Ok(None);
        };

        let q_tokens = content_tokens(question);
        let cleaned = clean_extracted_span_for_question(raw_slice, &q_tokens);
        if cleaned.is_empty() || !grounded(&cleaned, context) {
            return Ok(None);
        }

        Ok(Some((cleaned, margin)))
    }
}

impl Reranker for OnnxQaReranker {
    fn name(&self) -> &str {
        "minilm-squad2"
    }

    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        if candidates.is_empty() {
            return Ok(no_evidence(&self.provider_label()));
        }

        if std::env::var_os("LIGHTMEM_QA_DISABLE").is_some() {
            return Ok(no_evidence("none"));
        }

        let gated: Vec<&ScoredMemory> = candidates
            .iter()
            .filter(|c| c.bm25_rank.is_some() || c.score >= 0.35)
            .take(10)
            .collect();

        if gated.is_empty() {
            return Ok(no_evidence(&self.provider_label()));
        }

        let engine = self.get_or_load_engine()?;

        let q_tokens = content_tokens(question);

        let mut best_hit: Option<(String, f32, &MemoryRecord)> = None;

        for (idx, candidate) in gated.iter().enumerate() {
            let Some((span, margin)) =
                Self::extract_span(&engine, question, &candidate.memory.content)?
            else {
                continue;
            };

            if !is_valid_qa_span(&span, margin, &q_tokens, engine.need_token_type_ids) {
                continue;
            }

            // Early exit if Rank-1 candidate has an unambiguous high-confidence span
            if idx == 0 && margin >= 11.0 {
                let confidence = margin_to_confidence(margin);
                return Ok(AnswerResult {
                    answer: span,
                    selected_memory: Some(candidate.memory.clone()),
                    confidence,
                    reranker_used: engine.provider_label.clone(),
                });
            }

            let combined_score = margin + candidate.score * 0.5;
            if best_hit
                .as_ref()
                .map(|(_, prev_score, _)| combined_score > *prev_score)
                .unwrap_or(true)
            {
                best_hit = Some((span, combined_score, &candidate.memory));
            }
        }

        if let Some((answer, combined_score, memory)) = best_hit {
            let confidence = margin_to_confidence(combined_score);
            return Ok(AnswerResult {
                answer,
                selected_memory: Some(memory.clone()),
                confidence,
                reranker_used: engine.provider_label.clone(),
            });
        }

        Ok(no_evidence(&engine.provider_label))
    }

    fn merge_conflict(&self, older: &MemoryRecord, newer: &MemoryRecord) -> Result<MemoryRecord> {
        if std::env::var_os("LIGHTMEM_QA_DISABLE").is_none() {
            if let Ok(engine) = self.get_or_load_engine() {
                let old_toks = content_tokens(&older.content);
                let new_toks = content_tokens(&newer.content);
                let shared: Vec<&String> = old_toks.intersection(&new_toks).take(4).collect();
                if !shared.is_empty() {
                    let probe_q = format!(
                        "What is {}?",
                        shared
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    );
                    let old_margin = Self::extract_span(&engine, &probe_q, &older.content)
                        .ok()
                        .flatten()
                        .map(|(_, m)| m)
                        .unwrap_or(f32::NEG_INFINITY);
                    let new_margin = Self::extract_span(&engine, &probe_q, &newer.content)
                        .ok()
                        .flatten()
                        .map(|(_, m)| m)
                        .unwrap_or(f32::NEG_INFINITY);
                    // If older is strictly richer (newer's tokens are a subset of older's) AND older has higher QA confidence, keep older's content
                    if new_toks.is_subset(&old_toks)
                        && old_toks.len() > new_toks.len()
                        && old_margin >= new_margin
                    {
                        return Ok(build_merged_record(
                            older,
                            newer,
                            older.content.clone(),
                            older.title.clone(),
                        ));
                    }
                }
            }
        }
        default_merge_conflict(older, newer)
    }
}

/// Mode 3: Swappable Ollama QA Reranker (`--reranker ollama` or `--reranker ollama:<model>`)
#[derive(Debug, Clone)]
pub struct OllamaReranker {
    pub ollama_url: String,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

pub type LlmReranker = OllamaReranker;

impl OllamaReranker {
    pub fn new(ollama_url: String, model: Option<String>) -> Self {
        Self::with_api_key(ollama_url, model, None)
    }

    pub fn with_api_key(
        ollama_url: String,
        model: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            ollama_url: ollama_url.trim_end_matches('/').to_string(),
            model: model
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty()),
            api_key: api_key
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty() && !k.eq_ignore_ascii_case("none")),
        }
    }

    pub fn resolve_model(&self) -> String {
        if let Some(ref explicit) = self.model {
            let l = explicit.trim().to_lowercase();
            if !matches!(
                l.as_str(),
                "minilm-squad2"
                    | "tinyroberta-squad2"
                    | "onnx"
                    | "qa"
                    | "auto"
                    | "ollama"
                    | "openai"
                    | "llm"
                    | "default"
                    | ""
            ) {
                return explicit.clone();
            }
        }

        let is_cloud_endpoint = self.ollama_url.to_lowercase().contains("ollama.com");
        let preferred_local = [
            "bonsai-2:2b",
            "bonsai-2:latest",
            "qwen2.5:3b",
            "qwen2.5:1.5b",
            "llama3.2:3b",
            "llama3.2:1b",
            "gemma4:31b",
            "gpt-4o-mini",
            "qwen2.5:7b",
            "llama3.1:8b",
        ];
        let preferred_cloud = [
            "gemma4:31b-cloud",
            "gemma4:31b",
            "gpt-4o-mini",
            "qwen2.5:3b",
            "qwen2.5:1.5b",
            "llama3.2:3b",
            "llama3.2:1b",
            "bonsai-2:2b",
            "bonsai-2:latest",
        ];
        let preferred: &[&str] = if is_cloud_endpoint {
            &preferred_cloud
        } else {
            &preferred_local
        };

        let is_non_chat_model = |name: &str| {
            let l = name.to_lowercase();
            l.contains("embed")
                || l.contains("bge")
                || l.contains("minilm")
                || l.contains("vl")
                || l.contains("ocr")
        };
        let is_cloud_tag = |name: &str| {
            let l = name.to_lowercase();
            l.contains("-cloud") || l.ends_with(":cloud")
        };

        let select_model = |names: &[String]| -> Option<String> {
            for p in preferred {
                if let Some(found) = names.iter().find(|n| n.eq_ignore_ascii_case(p)) {
                    return Some(found.clone());
                }
            }
            if !is_cloud_endpoint {
                if let Some(found) = names
                    .iter()
                    .find(|n| !is_non_chat_model(n) && !is_cloud_tag(n))
                {
                    return Some(found.clone());
                }
                if let Some(found) = names
                    .iter()
                    .find(|n| n.eq_ignore_ascii_case("gemma4:31b-cloud"))
                {
                    return Some(found.clone());
                }
            }
            names.iter().find(|n| !is_non_chat_model(n)).cloned()
        };

        // 1. Try standard OpenAI-compatible `/v1/models` endpoint
        let v1_models_url = format!("{}/v1/models", self.ollama_url);
        let mut req = ureq::get(&v1_models_url).timeout(Duration::from_secs(2));
        if let Some(ref key) = self.api_key {
            req = req.set(
                "Authorization",
                &crate::models::config::format_auth_header(key),
            );
        }
        if let Ok(resp) = req.call() {
            if let Ok(json) = resp.into_json::<serde_json::Value>() {
                if let Some(data) = json.get("data").and_then(|v| v.as_array()) {
                    let names: Vec<String> = data
                        .iter()
                        .filter_map(|m| m.get("id").and_then(|n| n.as_str()).map(String::from))
                        .collect();

                    if let Some(chosen) = select_model(&names) {
                        return chosen;
                    }
                }
            }
        }

        // 2. Fallback: query Ollama-specific `/api/tags`
        let tags_url = format!("{}/api/tags", self.ollama_url);
        let mut req_tags = ureq::get(&tags_url).timeout(Duration::from_secs(2));
        if let Some(ref key) = self.api_key {
            req_tags = req_tags.set(
                "Authorization",
                &crate::models::config::format_auth_header(key),
            );
        }
        if let Ok(resp) = req_tags.call() {
            if let Ok(json) = resp.into_json::<serde_json::Value>() {
                if let Some(models) = json.get("models").and_then(|v| v.as_array()) {
                    let names: Vec<String> = models
                        .iter()
                        .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                        .collect();

                    if let Some(chosen) = select_model(&names) {
                        return chosen;
                    }
                }
            }
        }

        if is_cloud_endpoint {
            "gemma4:31b-cloud".to_string()
        } else {
            "bonsai-2:2b".to_string()
        }
    }

    fn request_json_completion(
        &self,
        model_name: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
    ) -> Result<String> {
        let chat_url = format!("{}/v1/chat/completions", self.ollama_url);
        let chat_payload = serde_json::json!({
            "model": model_name,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "temperature": 0.0,
            "max_tokens": max_tokens,
            "reasoning_effort": "none",
            "response_format": { "type": "json_object" }
        });

        let send_chat = |payload: serde_json::Value| {
            let mut req = ureq::post(&chat_url).timeout(Duration::from_secs(30));
            if let Some(ref key) = self.api_key {
                req = req.set(
                    "Authorization",
                    &crate::models::config::format_auth_header(key),
                );
            }
            req.send_json(payload).map_err(Box::new)
        };

        let chat_result = match send_chat(chat_payload) {
            Err(err) if matches!(*err, ureq::Error::Status(400, _)) => {
                // Retry without optional OpenAI/Ollama extension fields for strict OpenAI proxies.
                let minimal_payload = serde_json::json!({
                    "model": model_name,
                    "messages": [
                        {
                            "role": "system",
                            "content": system_prompt
                        },
                        {
                            "role": "user",
                            "content": user_prompt
                        }
                    ],
                    "temperature": 0.0,
                    "max_tokens": max_tokens
                });
                send_chat(minimal_payload)
            }
            other => other,
        };

        match chat_result {
            Ok(resp) => {
                let body: serde_json::Value = resp
                    .into_json()
                    .context("Failed to parse JSON response from /v1/chat/completions")?;
                Ok(body
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c0| c0.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|v| v.as_str())
                    .or_else(|| body.get("response").and_then(|v| v.as_str()))
                    .unwrap_or("{}")
                    .to_string())
            }
            Err(err) if matches!(*err, ureq::Error::Status(404, _)) => {
                // Fallback to legacy Ollama /api/generate
                let gen_url = format!("{}/api/generate", self.ollama_url);
                let gen_prompt = format!("{}\n\n{}", system_prompt, user_prompt);
                let send_generate = |with_think_flag: bool| {
                    let mut fallback_req = ureq::post(&gen_url).timeout(Duration::from_secs(30));
                    if let Some(ref key) = self.api_key {
                        fallback_req = fallback_req.set(
                            "Authorization",
                            &crate::models::config::format_auth_header(key),
                        );
                    }
                    let gen_payload = if with_think_flag {
                        serde_json::json!({
                            "model": model_name,
                            "prompt": &gen_prompt,
                            "stream": false,
                            "format": "json",
                            "think": false,
                            "options": {
                                "temperature": 0.0,
                                "num_predict": max_tokens
                            }
                        })
                    } else {
                        serde_json::json!({
                            "model": model_name,
                            "prompt": &gen_prompt,
                            "stream": false,
                            "format": "json",
                            "options": {
                                "temperature": 0.0,
                                "num_predict": max_tokens
                            }
                        })
                    };
                    fallback_req.send_json(gen_payload).map_err(Box::new)
                };

                let resp = match send_generate(true) {
                    Err(err) if matches!(*err, ureq::Error::Status(400, _)) => send_generate(false),
                    other => other,
                }
                .map_err(|e| anyhow::anyhow!("Ollama QA request to {} failed: {}", gen_url, e))?;
                let body: serde_json::Value = resp.into_json()?;
                Ok(body
                    .get("response")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}")
                    .to_string())
            }
            Err(e) => {
                anyhow::bail!(
                    "Chat completions request to {} (model '{}') failed: {}",
                    chat_url,
                    model_name,
                    e
                );
            }
        }
    }
}

impl Reranker for OllamaReranker {
    fn name(&self) -> &str {
        "ollama"
    }

    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        let model_name = self.resolve_model();
        let provider_label = format!("ollama:{}", model_name);

        let gated: Vec<&ScoredMemory> = candidates
            .iter()
            .filter(|c| c.bm25_rank.is_some() || c.score >= 0.35)
            .take(10)
            .collect();

        if gated.is_empty() {
            return Ok(no_evidence(&provider_label));
        }

        let synthesis = is_synthesis_query(question);
        let mut memories_block = String::new();
        for (idx, c) in gated.iter().enumerate() {
            if synthesis {
                memories_block.push_str(&format!(
                    "[{}] ({} · {}) {}\n",
                    idx,
                    c.memory.category.as_str(),
                    c.memory.title,
                    c.memory.content
                ));
            } else {
                memories_block.push_str(&format!("[{}] {}\n", idx, c.memory.content));
            }
        }

        let (system_prompt, max_tokens) = if synthesis {
            (
                "You are a grounded memory synthesis engine. Summarize or list information using ONLY the provided memory records.\n\
                 Rules:\n\
                 1. Synthesize a concise, factual answer combining details from ALL relevant provided memories that match EVERY topic and qualifier in the question (for example, if asked about \"sprint goals\" and no memory mentions sprint, return null).\n\
                 2. Cite zero ungrounded facts: include only facts explicitly present in the provided memories.\n\
                 3. If the provided memories do NOT contain relevant records matching the question's specific topic/qualifiers, return null for \"answer\" and null for \"memory_index\".\n\
                 4. Respond with valid JSON matching: {\"answer\": string_or_null, \"memory_index\": integer_or_null, \"confidence\": number_between_0_and_1} where \"memory_index\" is the 0-based index of the primary supporting memory.",
                512,
            )
        } else {
            (
                "You are a strict extractive question-answering engine. Answer the question using ONLY the provided memory records.\n\
                 Rules:\n\
                 1. Extract the exact concise factual value (e.g. name, ID, account number, roll number, port, OS, URL, college name, score) from the single best matching memory.\n\
                 2. Do NOT include labels or full sentences — return ONLY the extracted value itself.\n\
                 3. If none of the memories contain the answer, or if a memory states the information is not recorded, return null for \"answer\" and null for \"memory_index\".\n\
                 4. Respond with valid JSON matching: {\"answer\": string_or_null, \"memory_index\": integer_or_null, \"confidence\": number_between_0_and_1}",
                256,
            )
        };

        let user_prompt = format!("Memories:\n{}\nQuestion: {}", memories_block, question);
        let raw_response =
            self.request_json_completion(&model_name, system_prompt, &user_prompt, max_tokens)?;

        if let Some((answer, mem_idx, confidence)) =
            parse_and_ground_ollama_answer_with_mode(&raw_response, &gated, synthesis)
        {
            return Ok(AnswerResult {
                answer,
                selected_memory: Some(gated[mem_idx].memory.clone()),
                confidence,
                reranker_used: provider_label,
            });
        }

        Ok(no_evidence(&provider_label))
    }

    fn merge_conflict(&self, older: &MemoryRecord, newer: &MemoryRecord) -> Result<MemoryRecord> {
        let model_name = self.resolve_model();
        let system_prompt = "You are a memory consolidation and conflict resolution engine.\n\
             Merge the following two overlapping memories into a single accurate memory.\n\
             Rules:\n\
             1. Always prefer factual values (numbers, ports, statuses, URLs, IDs) from the [NEWER] memory when they differ from [OLDER].\n\
             2. Preserve any additional non-contradictory details from [OLDER] so no context is lost.\n\
             3. Do NOT invent any facts not present in the two memories.\n\
             4. Respond with valid JSON: {\"title\": string, \"content\": string}";

        let user_prompt = format!(
            "[OLDER · {}] Title: {}\nContent: {}\n\n[NEWER · {}] Title: {}\nContent: {}\n",
            older.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
            older.title,
            older.content,
            newer.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
            newer.title,
            newer.content,
        );

        if let Ok(raw) = self.request_json_completion(&model_name, system_prompt, &user_prompt, 512)
        {
            if let Some(json_slice) = extract_json_slice(&raw) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_slice) {
                    if let Some(merged_c) = parsed
                        .get("content")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        let merged_t = parsed
                            .get("title")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .unwrap_or(&newer.title);
                        return Ok(build_merged_record(
                            older,
                            newer,
                            merged_c.to_string(),
                            merged_t.to_string(),
                        ));
                    }
                }
            }
        }

        default_merge_conflict(older, newer)
    }
}

pub fn build_merged_record(
    older: &MemoryRecord,
    newer: &MemoryRecord,
    content: String,
    title: String,
) -> MemoryRecord {
    let mut merged = newer.clone();
    merged.content = content;
    merged.title = title;
    merged.confidence = older.confidence.max(newer.confidence);
    merged.updated_at = chrono::Utc::now();

    let mut seen: HashSet<String> = merged.tags.iter().map(|t| t.to_lowercase()).collect();
    for tag in &older.tags {
        if seen.insert(tag.to_lowercase()) {
            merged.tags.push(tag.clone());
        }
    }
    let mut seen_sha: HashSet<String> = merged
        .attachments
        .iter()
        .map(|a| a.sha256.clone())
        .collect();
    for att in &older.attachments {
        if seen_sha.insert(att.sha256.clone()) {
            let mut carried = att.clone();
            carried.memory_id = merged.id.clone();
            merged.attachments.push(carried);
        }
    }
    for ent in &older.entities {
        if !merged
            .entities
            .iter()
            .any(|e| e.name == ent.name && e.val == ent.val)
        {
            merged.entities.push(ent.clone());
        }
    }
    merged
}

pub fn default_merge_conflict(older: &MemoryRecord, newer: &MemoryRecord) -> Result<MemoryRecord> {
    // Local fallback for conflict merge: prefer newer content to allow supersede,
    // but preserve older's attachments when newer has none.
    // This is a simple strategy; for semantic conflicts, configure an explicit merge provider.
    let merged_content = if newer.content.trim().is_empty() {
        older.content.clone()
    } else {
        newer.content.clone()
    };
    let merged_title = if newer.title.trim().is_empty() {
        older.title.clone()
    } else {
        newer.title.clone()
    };
    Ok(build_merged_record(
        older,
        newer,
        merged_content,
        merged_title,
    ))
}

pub(crate) fn is_synthesis_query(question: &str) -> bool {
    let lower = question.trim().to_lowercase();
    let padded = format!(" {} ", lower);
    if ["summarize", "summarise", "summary", "overview", "recap"]
        .iter()
        .any(|kw| {
            lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|tok| tok == *kw)
        })
    {
        return true;
    }

    [
        " list all ",
        " list my ",
        " all my ",
        " all of my ",
        " what are my ",
        " what are all ",
        " tell me about my ",
        " tell me about all ",
        " describe my ",
        " explain my ",
    ]
    .iter()
    .any(|phrase| padded.contains(phrase))
}

fn find_thinking_ranges(raw: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel_open) = raw[cursor..].find("<think>") {
        let open_start = cursor + rel_open;
        let body_start = open_start + "<think>".len();
        if let Some(rel_close) = raw[body_start..].find("</think>") {
            let close_end = body_start + rel_close + "</think>".len();
            ranges.push((open_start, close_end));
            cursor = close_end;
        } else {
            break;
        }
    }
    ranges
}

pub(crate) fn extract_json_slice(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let think_ranges = find_thinking_ranges(trimmed);

    let mut valid_objects: Vec<(usize, &str)> = Vec::new();
    let mut chars = trimmed.char_indices().peekable();
    while let Some((byte_idx, ch)) = chars.next() {
        if let Some((_, skip_end)) = think_ranges
            .iter()
            .find(|(st, en)| byte_idx >= *st && byte_idx < *en)
        {
            while let Some(&(next_idx, _)) = chars.peek() {
                if next_idx < *skip_end {
                    chars.next();
                } else {
                    break;
                }
            }
            continue;
        }
        if ch == '{' {
            let start = byte_idx;
            let mut depth = 1usize;
            let mut in_string = false;
            let mut escaped = false;
            let mut end_opt = None;
            for (inner_idx, inner_ch) in chars.by_ref() {
                if in_string {
                    if escaped {
                        escaped = false;
                    } else if inner_ch == '\\' {
                        escaped = true;
                    } else if inner_ch == '"' {
                        in_string = false;
                    }
                    continue;
                }
                match inner_ch {
                    '"' => in_string = true,
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end_opt = Some(inner_idx);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end_idx) = end_opt {
                let candidate = &trimmed[start..=end_idx];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    valid_objects.push((start, candidate));
                }
            }
        }
    }

    if !valid_objects.is_empty() {
        // When a reasoning model emits `{...}\n</think>\n\n{...}` or `<think>...</think>\n{...}`,
        // prefer the first valid JSON object that appears cleanly after the last `</think>` tag.
        if let Some(last_close) = trimmed.rfind("</think>") {
            let after_close = last_close + "</think>".len();
            if let Some((_, slice)) = valid_objects.iter().find(|(st, _)| *st >= after_close) {
                return Some(*slice);
            }
        }
        return Some(valid_objects[0].1);
    }

    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if start <= end {
        let candidate = &trimmed[start..=end];
        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
            return Some(candidate);
        }
    }
    None
}

fn is_answer_grounded_in(answer: &str, mem: &MemoryRecord) -> bool {
    if grounded(answer, &mem.content) || (!mem.title.is_empty() && grounded(answer, &mem.title)) {
        return true;
    }
    // For multi-word descriptive answers from LLMs (e.g. "who is kk"), verify every
    // content token (and every digit sequence) in the answer comes from the selected memory.
    let ans_tokens = content_tokens(answer);
    if ans_tokens.len() >= 2 {
        let mut mem_tokens = content_tokens(&mem.content);
        if !mem.title.is_empty() {
            mem_tokens.extend(content_tokens(&mem.title));
        }
        let all_tokens_present = ans_tokens.iter().all(|t| mem_tokens.contains(t));
        if all_tokens_present {
            return true;
        }
    }
    false
}

fn is_synthesis_grounded(answer: &str, gated: &[&ScoredMemory]) -> bool {
    let mut corpus_tokens: HashSet<String> = HashSet::new();
    let mut corpus_text = String::new();
    for c in gated {
        corpus_tokens.extend(content_tokens(&c.memory.content));
        corpus_tokens.extend(content_tokens(&c.memory.title));
        corpus_tokens.insert(c.memory.category.as_str().to_lowercase());
        corpus_text.push_str(&c.memory.title);
        corpus_text.push(' ');
        corpus_text.push_str(&c.memory.content);
        corpus_text.push('\n');
    }

    let ans_tokens = content_tokens(answer);
    if ans_tokens.is_empty() {
        return false;
    }

    // 1. Strict numeric/code anti-hallucination guard: every digit-bearing token in the answer
    // MUST exist in the union of retrieved memories (content or title).
    for tok in &ans_tokens {
        if tok.chars().any(|c| c.is_ascii_digit())
            && !corpus_tokens.contains(tok)
            && !grounded(tok, &corpus_text)
        {
            return false;
        }
    }

    // 2. Substantive lexical overlap guard (ignoring harmless summary/connector framing words).
    const FRAMING_WORDS: &[&str] = &[
        "also",
        "additionally",
        "furthermore",
        "moreover",
        "including",
        "include",
        "includes",
        "summary",
        "summarize",
        "overall",
        "based",
        "mentioned",
        "noted",
        "across",
        "earlier",
        "currently",
        "several",
        "multiple",
        "various",
        "primarily",
        "key",
        "details",
        "history",
        "records",
        "memories",
        "goals",
        "goal",
        "plan",
        "planned",
        "ranging",
        "between",
        "through",
        "having",
    ];
    let substantive: Vec<&String> = ans_tokens
        .iter()
        .filter(|t| !FRAMING_WORDS.contains(&t.as_str()))
        .collect();

    if substantive.is_empty() {
        return false;
    }

    let matched = substantive
        .iter()
        .filter(|t| corpus_tokens.contains(t.as_str()) || grounded(t, &corpus_text))
        .count();

    let min_required = if substantive.len() == 1 { 1 } else { 2 };
    matched >= min_required && matched * 100 >= substantive.len() * 65
}

fn clean_synthesis_answer(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`'))
        .trim()
        .to_string()
}

fn extract_memory_index(val: Option<&serde_json::Value>) -> Option<usize> {
    let v = val?;
    if let Some(n) = v.as_u64() {
        return Some(n as usize);
    }
    if let Some(arr) = v.as_array() {
        return arr
            .first()
            .and_then(|item| item.as_u64())
            .map(|n| n as usize);
    }
    if let Some(s) = v.as_str() {
        return s.trim().parse::<usize>().ok();
    }
    None
}

#[cfg_attr(not(test), allow(dead_code))]
fn parse_and_ground_ollama_answer(
    raw_json: &str,
    gated: &[&ScoredMemory],
) -> Option<(String, usize, f32)> {
    parse_and_ground_ollama_answer_with_mode(raw_json, gated, false)
}

fn parse_and_ground_ollama_answer_with_mode(
    raw_json: &str,
    gated: &[&ScoredMemory],
    synthesis: bool,
) -> Option<(String, usize, f32)> {
    let json_slice = extract_json_slice(raw_json)?;
    let parsed: serde_json::Value = serde_json::from_str(json_slice).ok()?;
    let raw_ans = parsed.get("answer")?.as_str()?.trim();
    if raw_ans.is_empty() {
        return None;
    }

    let cleaned = if synthesis {
        clean_synthesis_answer(raw_ans)
    } else {
        clean_extracted_span(raw_ans)
    };
    let lower = cleaned.to_lowercase();
    if cleaned.is_empty()
        || matches!(
            lower.as_str(),
            "null" | "none" | "n/a" | "unknown" | "not recorded" | "insufficient evidence"
        )
        || lower.contains("not been recorded")
        || lower.contains("insufficient evidence")
        || lower.contains("no matching memory")
        || lower.contains("no relevant memory")
        || lower.contains("not mentioned in")
        || lower.contains("cannot determine")
    {
        return None;
    }

    let raw_conf = parsed
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.85) as f32;
    let confidence = if raw_conf.is_finite() && raw_conf > 0.0 {
        raw_conf.clamp(0.05, 1.0)
    } else {
        0.85
    };

    let explicit_idx = extract_memory_index(parsed.get("memory_index"));

    if synthesis {
        if !is_synthesis_grounded(&cleaned, gated) {
            return None;
        }
        let ans_tokens = content_tokens(&cleaned);
        let best_idx = explicit_idx
            .filter(|&i| {
                if i >= gated.len() {
                    return false;
                }
                let mut m_toks = content_tokens(&gated[i].memory.content);
                m_toks.extend(content_tokens(&gated[i].memory.title));
                ans_tokens.iter().any(|t| m_toks.contains(t))
            })
            .unwrap_or_else(|| {
                gated
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, c)| {
                        let mut m_toks = content_tokens(&c.memory.content);
                        m_toks.extend(content_tokens(&c.memory.title));
                        ans_tokens
                            .iter()
                            .filter(|t| m_toks.contains(t.as_str()))
                            .count()
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        return Some((cleaned, best_idx, confidence));
    }

    if let Some(idx) = explicit_idx {
        if idx < gated.len() && is_answer_grounded_in(&cleaned, &gated[idx].memory) {
            return Some((cleaned, idx, confidence));
        }
    }

    for (idx, cand) in gated.iter().enumerate() {
        if is_answer_grounded_in(&cleaned, &cand.memory) {
            return Some((cleaned, idx, confidence));
        }
    }

    None
}

pub type PrecisionReranker = OnnxQaReranker;

fn no_evidence(provider: &str) -> AnswerResult {
    AnswerResult {
        answer: "Insufficient evidence in the retrieved memories to answer this question.".into(),
        selected_memory: None,
        confidence: 0.0,
        reranker_used: provider.into(),
    }
}

fn margin_to_confidence(margin: f32) -> f32 {
    (1.0 / (1.0 + (-margin / 5.0).exp())).clamp(0.05, 0.99)
}

fn clean_extracted_span(raw: &str) -> String {
    let mut s = raw
        .trim()
        .trim_matches(|c: char| {
            c.is_whitespace() || matches!(c, ',' | ';' | '.' | '\'' | '"' | '`')
        })
        .trim();

    if s.starts_with('(') && s.ends_with(')') && s.len() > 2 {
        s = s[1..s.len() - 1].trim();
    } else if s.starts_with('(') && !s.contains(')') {
        s = s.trim_start_matches('(').trim();
    } else if s.ends_with(')') && !s.contains('(') {
        s = s.trim_end_matches(')').trim();
    }

    // If the model included a short "Label: Value" prefix (not a URL like https://), strip the label.
    // Guard against stripping numeric timestamps or time intervals (e.g. "18:00 to 19:00" or "from 18:00 to 19:00").
    if !s.contains("://") {
        if let Some((lhs, rhs)) = s.split_once(':') {
            let rhs_clean = rhs.trim();
            let lhs_trimmed = lhs.trim();
            let is_time_or_numeric = lhs_trimmed.chars().all(|c| c.is_ascii_digit())
                || (lhs_trimmed.ends_with(|c: char| c.is_ascii_digit())
                    && rhs.starts_with(|c: char| c.is_ascii_digit()));
            if !rhs_clean.is_empty()
                && !lhs_trimmed.is_empty()
                && lhs.split_whitespace().count() <= 4
                && !is_time_or_numeric
            {
                s = rhs_clean;
            }
        }
    }

    s.trim_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '.' | '\'' | '"'))
        .to_string()
}

fn clean_extracted_span_for_question(raw: &str, q_tokens: &HashSet<String>) -> String {
    let cleaned = clean_extracted_span(raw);
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    if (2..=3).contains(&words.len()) {
        let last = words[words.len() - 1];
        let prefix_in_question = words[..words.len() - 1]
            .iter()
            .all(|w| q_tokens.contains(&w.to_lowercase()));
        if prefix_in_question && last.chars().any(|c| c.is_ascii_digit()) {
            return last.to_string();
        }
    }
    cleaned
}

fn content_tokens(text: &str) -> HashSet<String> {
    const FUNCTION_WORDS: &[&str] = &[
        "what", "which", "who", "where", "when", "why", "how", "is", "are", "was", "were", "does",
        "do", "did", "has", "have", "had", "the", "a", "an", "in", "on", "at", "to", "for", "of",
        "with", "by", "from", "my", "me", "our", "your", "user", "users", "use", "uses", "used",
        "run", "runs", "running", "did", "i",
    ];
    text.split(|c: char| !c.is_alphanumeric())
        .map(|w| w.to_lowercase())
        .filter(|w| {
            (w.chars().any(|c| c.is_ascii_digit()) || w.len() > 1)
                && !FUNCTION_WORDS.contains(&w.as_str())
        })
        .collect()
}

fn is_valid_qa_span(
    span: &str,
    margin: f32,
    q_tokens: &HashSet<String>,
    has_token_type_ids: bool,
) -> bool {
    let is_alnum_code = !span.contains(' ')
        && span.len() >= 6
        && span.chars().any(|c| c.is_ascii_alphabetic())
        && span.chars().any(|c| c.is_ascii_digit());
    let base_min = if has_token_type_ids { 4.5 } else { 3.0 };
    let min_margin = if is_alnum_code { 1.5 } else { base_min };
    if margin < min_margin {
        return false;
    }

    let lower = span.to_lowercase();
    if lower.contains("not been ")
        || lower.contains("not recorded")
        || lower.contains("not specified")
        || lower.contains("not provided")
        || lower.contains("x0000x")
        || lower.contains("xxxx")
        || lower.contains("***")
    {
        return false;
    }

    let s_tokens = content_tokens(span);
    // Reject spans that contain zero new tokens beyond the question itself
    if !s_tokens.is_empty() && s_tokens.iter().all(|t| q_tokens.contains(t)) {
        return false;
    }

    // Reject medium-margin spans (< 10.0) that echo question words
    // (e.g. "4 passport-sized photos" echoing "passport" on "what is my passport number")
    let echo_limit = if has_token_type_ids { 10.0 } else { 6.0 };
    if margin < echo_limit && s_tokens.iter().any(|t| q_tokens.contains(t)) {
        return false;
    }

    true
}

fn grounded(answer: &str, content: &str) -> bool {
    let pattern = format!(
        r"(?i)(?:^|[^\p{{L}}\p{{N}}_]){}(?:$|[^\p{{L}}\p{{N}}_])",
        regex::escape(answer)
    );
    Regex::new(&pattern)
        .map(|r| r.is_match(content))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_candidate(id: &str, content: &str, score: f32) -> ScoredMemory {
        let mut memory = MemoryRecord::new(
            crate::models::MemoryType::Fact,
            "Test fact".to_string(),
            content.to_string(),
            vec![],
            0.9,
            None,
        );
        memory.id = id.to_string();
        ScoredMemory {
            memory,
            score,
            bm25_rank: Some(1),
            vector_rank: Some(1),
        }
    }

    #[test]
    fn test_clean_extracted_span_and_grounding() {
        assert!(!grounded("80", "server runs on 8080"));
        assert!(grounded("8080", "server runs on 8080."));
        assert_eq!(
            clean_extracted_span("Registered No: 2203226828."),
            "2203226828"
        );
        assert_eq!(
            clean_extracted_span("https://atlas.example.test/api"),
            "https://atlas.example.test/api"
        );
        assert_eq!(clean_extracted_span("(22A31A05I7)"), "22A31A05I7");
        assert_eq!(clean_extracted_span("18:00 to 19:00"), "18:00 to 19:00");
        assert_eq!(clean_extracted_span("10:30 AM"), "10:30 AM");
        assert_eq!(
            clean_extracted_span("Time: 18:00 to 19:00"),
            "18:00 to 19:00"
        );
        assert_eq!(
            clean_extracted_span("from 18:00 to 19:00"),
            "from 18:00 to 19:00"
        );
        assert_eq!(
            clean_extracted_span("between 18:00 and 19:00"),
            "between 18:00 and 19:00"
        );
        assert_eq!(
            clean_extracted_span("Daily routine: from 18:00 to 19:00"),
            "from 18:00 to 19:00"
        );
        assert_eq!(clean_extracted_span("Port: 5432"), "5432");
    }

    #[test]
    fn test_is_valid_qa_span_filters_echoes_and_negations() {
        let q_passport = content_tokens("what is my passport number");
        assert!(
            !is_valid_qa_span("4 passport-sized photos", 3.30, &q_passport, true),
            "Low-margin span echoing 'passport' must be rejected"
        );

        let q_pan = content_tokens("what is my pan card no");
        assert!(
            is_valid_qa_span("HAQPP8118D", 1.97, &q_pan, true),
            "Distinct identifier with zero question echo must be accepted"
        );

        let q_father = content_tokens("what is my father name");
        assert!(
            !is_valid_qa_span(
                "My fathers name has not been recorded",
                13.12,
                &q_father,
                true
            ),
            "Negation/absence span must be rejected"
        );
        assert!(is_valid_qa_span("Pathi Srinivas", 15.29, &q_father, true));
    }

    #[test]
    fn test_ollama_json_response_grounding() {
        let c1 = make_candidate("1", "Dad's PAN card is stored at dad_PAN.pdf", 0.48);
        let c2 = make_candidate("2", "User's PAN ID is HAQPP8118D.", 0.44);
        let gated = vec![&c1, &c2];

        let valid_resp = r#"{"answer": "HAQPP8118D", "memory_index": 1, "confidence": 0.92}"#;
        let (ans, idx, conf) = parse_and_ground_ollama_answer(valid_resp, &gated).unwrap();
        assert_eq!(ans, "HAQPP8118D");
        assert_eq!(idx, 1);
        assert!((conf - 0.92).abs() < 1e-4);

        let hallucinated = r#"{"answer": "ZZZZ9999Z", "memory_index": 1, "confidence": 0.95}"#;
        assert!(
            parse_and_ground_ollama_answer(hallucinated, &gated).is_none(),
            "Ungrounded Ollama answer must be rejected"
        );

        let abstained = r#"{"answer": null, "memory_index": null, "confidence": 0.0}"#;
        assert!(parse_and_ground_ollama_answer(abstained, &gated).is_none());

        let markdown_fenced =
            "```json\n{\"answer\": \"HAQPP8118D\", \"memory_index\": 1, \"confidence\": 1.0}\n```";
        let (ans_f, idx_f, _) = parse_and_ground_ollama_answer(markdown_fenced, &gated).unwrap();
        assert_eq!(ans_f, "HAQPP8118D");
        assert_eq!(idx_f, 1);
    }

    #[test]
    fn test_onnx_qa_reranker_end_to_end_when_enabled() {
        if std::env::var_os("LIGHTMEM_QA_DISABLE").is_some() {
            return;
        }

        let reranker = OnnxQaReranker::new(None);

        // 1. Multi-entity clause disambiguation with zero custom rules
        let multi = make_candidate(
            "1",
            "haproxy runs on port 8404 on Linux and envoy runs on port 9901 on Linux.",
            0.85,
        );
        let res_envoy = reranker
            .answer("What port does envoy use?", std::slice::from_ref(&multi))
            .unwrap();
        assert_eq!(res_envoy.answer, "9901");

        let res_haproxy = reranker
            .answer("What port does haproxy use?", std::slice::from_ref(&multi))
            .unwrap();
        assert_eq!(res_haproxy.answer, "8404");

        // 2. Father name fallback across insufficient candidate
        let c1_insufficient = make_candidate("1", "My fathers name has not been recorded.", 0.90);
        let c2_valid = make_candidate("2", "Pathi Srinivas is my fathers name", 0.85);
        let res_father = reranker
            .answer("what is my father name", &[c1_insufficient, c2_valid])
            .unwrap();
        assert_eq!(res_father.answer, "Pathi Srinivas");
        assert_eq!(res_father.selected_memory.unwrap().id, "2");

        // 3. PAN card no & College ID without keyword hacks
        let dad_pan = make_candidate(
            "1",
            "Dad's PAN card is stored at /home/krishnakanth/Documents/Family Vault/files/dad/dad_PAN.pdf",
            0.48,
        );
        let user_pan = make_candidate("2", "User's PAN ID is HAQPP8118D.", 0.44);
        let res_pan = reranker
            .answer("what is my pan card no", &[dad_pan, user_pan])
            .unwrap();
        assert_eq!(res_pan.answer, "HAQPP8118D");
        assert_eq!(res_pan.selected_memory.unwrap().id, "2");
    }

    #[test]
    fn test_content_tokens_preserves_single_digit_numeric_tokens() {
        let t1 = content_tokens("Workers: 4");
        assert!(t1.contains("workers"));
        assert!(t1.contains("4"));

        let t2 = content_tokens("Workers: 8");
        assert!(t2.contains("workers"));
        assert!(t2.contains("8"));

        assert_ne!(t1, t2);

        // Single letters that are not digits or function words
        let t3 = content_tokens("Plan A vs Plan B");
        // 'a' is a function word, but digits are kept
        assert!(t3.contains("plan"));
    }

    #[test]
    fn test_onnx_qa_reranker_returns_no_evidence_early_when_candidates_empty() {
        let reranker = OnnxQaReranker::new(Some("minilm-squad2".to_string()));
        let result = reranker
            .answer("What is the meaning of life?", &[])
            .unwrap();
        assert!(result.answer.contains("Insufficient evidence"));
        assert_eq!(result.confidence, 0.0);
        assert_eq!(result.reranker_used, "minilm-squad2");
    }

    #[test]
    fn test_extract_json_slice_handles_think_tags_and_duplicates() {
        let with_think = "<think>Considering {\"answer\": \"WRONG\"}</think>\n{\"answer\": \"HAQPP8118D\", \"memory_index\": 1, \"confidence\": 0.95}";
        assert_eq!(
            extract_json_slice(with_think).unwrap(),
            "{\"answer\": \"HAQPP8118D\", \"memory_index\": 1, \"confidence\": 0.95}"
        );

        let with_duplicate_close = "{\"answer\": \"HAQPP8118D\", \"memory_index\": 1}\n</think>\n\n{\"answer\": \"HAQPP8118D\", \"memory_index\": 1, \"confidence\": 0.9}";
        assert_eq!(
            extract_json_slice(with_duplicate_close).unwrap(),
            "{\"answer\": \"HAQPP8118D\", \"memory_index\": 1, \"confidence\": 0.9}"
        );

        let c1 = make_candidate("1", "Unrelated", 0.5);
        let c2 = make_candidate("2", "PAN is HAQPP8118D", 0.5);
        let c3 = make_candidate("3", "10th SSC Roll No. 2008102776", 0.5);
        let gated = vec![&c1, &c2, &c3];
        let array_idx =
            r#"{"answer": "2008102776", "memory_index": [2], "confidence": 0.95}</think>"#;
        let (ans, idx, _) = parse_and_ground_ollama_answer(array_idx, &gated).unwrap();
        assert_eq!(ans, "2008102776");
        assert_eq!(idx, 2);
    }

    #[test]
    fn test_synthesis_query_detection_and_grounding() {
        assert!(is_synthesis_query("summarize my goals"));
        assert!(is_synthesis_query("Summarize sprint goals"));
        assert!(is_synthesis_query("what are my study goals?"));
        assert!(is_synthesis_query("list all recent decisions"));
        assert!(!is_synthesis_query("what is my pan"));
        assert!(!is_synthesis_query("what port does redis use"));

        let g1 = make_candidate(
            "g1",
            "Weight loss target: 80 kg from 91 kg via 500 kcal daily deficit.",
            0.65,
        );
        let g2 = make_candidate(
            "g2",
            "Studying Fundamentals of Operating Systems via Udemy Business.",
            0.55,
        );
        let gated = vec![&g1, &g2];

        let grounded_sum = r#"{"answer": "Weight loss target: 80 kg from 91 kg with 500 kcal deficit. Also studying Fundamentals of Operating Systems via Udemy Business.", "memory_index": 0, "confidence": 0.96}"#;
        let (ans, idx, conf) =
            parse_and_ground_ollama_answer_with_mode(grounded_sum, &gated, true).unwrap();
        assert!(ans.starts_with("Weight loss target: 80 kg"));
        assert_eq!(idx, 0);
        assert!(conf > 0.9);

        let fabricated_digit = r#"{"answer": "Weight loss target: 9999 kg by next month.", "memory_index": 0, "confidence": 0.95}"#;
        assert!(
            parse_and_ground_ollama_answer_with_mode(fabricated_digit, &gated, true).is_none(),
            "Summary containing ungrounded numbers must be rejected"
        );

        let unrelated_hallucination = r#"{"answer": "Complete SpaceX Mars orbital telemetry milestone.", "memory_index": 0, "confidence": 0.95}"#;
        assert!(
            parse_and_ground_ollama_answer_with_mode(unrelated_hallucination, &gated, true)
                .is_none(),
            "Summary with ungrounded terms must be rejected"
        );

        let abstained = r#"{"answer": null, "memory_index": null, "confidence": 0.0}"#;
        assert!(parse_and_ground_ollama_answer_with_mode(abstained, &gated, true).is_none());
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        use std::io::{BufRead, BufReader};
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut reader = BufReader::new(stream);
        let mut headers = String::new();
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    headers.push_str(&line);
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("content-length") {
                            content_length = v.trim().parse::<usize>().unwrap_or(0);
                        }
                    }
                }
            }
        }
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            let _ = reader.read_exact(&mut body);
        }
        format!("{}{}", headers, String::from_utf8_lossy(&body))
    }

    #[test]
    fn test_ollama_resolve_model_prefers_bonsai_over_cloud_on_localhost() {
        use std::io::Write;
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let _ = read_http_request(&mut stream);
                let body = r#"{"object":"list","data":[{"id":"gemma4:31b-cloud","object":"model"},{"id":"embeddinggemma-2:latest","object":"model"},{"id":"qwen2.5:3b","object":"model"},{"id":"bonsai-2:2b","object":"model"}]}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });

        let reranker = OllamaReranker::new(format!("http://127.0.0.1:{}", port), None);
        let chosen = reranker.resolve_model();
        let _ = handle.join();
        assert_eq!(chosen, "bonsai-2:2b");
    }

    #[test]
    fn test_ollama_answer_reaches_rank_10_and_sends_reasoning_effort_none() {
        use std::io::Write;
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let saw_reasoning_none = Arc::new(AtomicBool::new(false));
        let saw_rank_9 = Arc::new(AtomicBool::new(false));
        let saw_reasoning_clone = Arc::clone(&saw_reasoning_none);
        let saw_rank9_clone = Arc::clone(&saw_rank_9);

        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let req = read_http_request(&mut stream);
                if req.contains("\"reasoning_effort\":\"none\"") {
                    saw_reasoning_clone.store(true, Ordering::SeqCst);
                }
                if req.contains("[9] ") && req.contains("2008102776") {
                    saw_rank9_clone.store(true, Ordering::SeqCst);
                }
                let body = r#"{"choices":[{"message":{"content":"{\"answer\":\"2008102776\",\"memory_index\":9,\"confidence\":0.95}"}}]}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });

        let mut cands: Vec<ScoredMemory> = (0..9)
            .map(|i| make_candidate(&format!("d{}", i), &format!("Distractor note {}", i), 0.5))
            .collect();
        cands.push(make_candidate(
            "ssc10",
            "10th SSC Regular March 2020 (Roll No. 2008102776)",
            0.38,
        ));

        let reranker = OllamaReranker::new(
            format!("http://127.0.0.1:{}", port),
            Some("bonsai-2:2b".to_string()),
        );
        let res = reranker
            .answer("what is my 10th roll number", &cands)
            .unwrap();
        let _ = handle.join();

        assert!(
            saw_reasoning_none.load(Ordering::SeqCst),
            "Must send reasoning_effort: none to prevent thinking-token exhaustion"
        );
        assert!(
            saw_rank_9.load(Ordering::SeqCst),
            "Must include candidate index 9 (top-10) instead of truncating at 5"
        );
        assert_eq!(res.answer, "2008102776");
        assert_eq!(res.selected_memory.unwrap().id, "ssc10");
    }
}
