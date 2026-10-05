use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightMemConfig {
    pub backend: String,
    pub onnx_model: Option<String>,
    pub ollama_url: String,
    pub embedding_model: String,
    #[serde(default = "default_reranker")]
    pub reranker: String,
}

fn default_reranker() -> String {
    "minilm-squad2".to_string()
}

impl Default for LightMemConfig {
    fn default() -> Self {
        Self {
            backend: "onnx".to_string(),
            onnx_model: Some("Xenova/bge-small-en-v1.5".to_string()),
            ollama_url: "http://localhost:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            reranker: default_reranker(),
        }
    }
}

impl LightMemConfig {
    /// Normalize any reranker string ('onnx', 'minilm-squad2', 'tinyroberta-squad2', 'ollama', 'ollama:<model>', 'top1')
    pub fn normalize_reranker(raw: &str, legacy_qa: Option<&str>) -> String {
        let trimmed = raw.trim();
        let lower = trimmed.to_lowercase();
        if lower == "top1" {
            return "top1".to_string();
        }
        if let Some(ollama_m) = trimmed
            .strip_prefix("ollama:")
            .or_else(|| trimmed.strip_prefix("OLLAMA:"))
        {
            return format!("ollama:{}", ollama_m.trim());
        }
        if let Some(onnx_m) = trimmed
            .strip_prefix("onnx:")
            .or_else(|| trimmed.strip_prefix("ONNX:"))
        {
            return Self::normalize_reranker(onnx_m, None);
        }
        match lower.as_str() {
            "ollama" => {
                if let Some(qm) = legacy_qa.map(str::trim).filter(|m| {
                    !m.is_empty() && !matches!(*m, "minilm-squad2" | "tinyroberta-squad2")
                }) {
                    format!("ollama:{}", qm)
                } else {
                    "ollama".to_string()
                }
            }
            "tinyroberta-squad2"
            | "tinyroberta"
            | "deepset/tinyroberta-squad2"
            | "onnx-community/tinyroberta-squad2-onnx" => "tinyroberta-squad2".to_string(),
            "minilm-squad2"
            | "deepset/minilm-uncased-squad2"
            | "lquint/minilm-uncased-squad2-onnx" => "minilm-squad2".to_string(),
            "onnx" | "qa" | "precision" => {
                if let Some(qm) = legacy_qa.map(str::trim).filter(|m| !m.is_empty()) {
                    Self::normalize_reranker(qm, None)
                } else {
                    "minilm-squad2".to_string()
                }
            }
            other if other.contains(':') => format!("ollama:{}", trimmed),
            _ => "minilm-squad2".to_string(),
        }
    }

    /// Stable embedding-space identity, including card preprocessing and engine version.
    pub fn embedding_identity(&self) -> Result<String> {
        let model = match self.backend.as_str() {
            "hash" => "hash:fnv-bigram-v1".to_string(),
            "ollama" => format!(
                "ollama:{}:{}",
                self.ollama_url.trim_end_matches('/'),
                self.embedding_model
            ),
            "onnx" => {
                let name = self.onnx_model.as_deref().unwrap_or("bge-small");
                let canonical = match name {
                    "minilm" | "all-minilm-l6-v2" | "Xenova/all-MiniLM-L6-v2" => {
                        "Xenova/all-MiniLM-L6-v2"
                    }
                    "nomic" | "nomic-embed-text" | "nomic-ai/nomic-embed-text-v1.5" => {
                        "nomic-ai/nomic-embed-text-v1.5"
                    }
                    "bge-small" | "bge-small-en-v1.5" | "Xenova/bge-small-en-v1.5" => {
                        "Xenova/bge-small-en-v1.5"
                    }
                    other => {
                        let path = Path::new(other);
                        anyhow::ensure!(
                            path.is_dir(),
                            "Unknown ONNX model or missing model directory: {}",
                            other
                        );
                        // Fingerprint file contents, not just the path: replacing a custom model requires migration.
                        use std::io::Read;
                        let mut hash = 0xcbf29ce484222325u64;
                        for file in [
                            "model.onnx",
                            "model_quantized.onnx",
                            "tokenizer.json",
                            "config.json",
                            "special_tokens_map.json",
                            "tokenizer_config.json",
                        ] {
                            let file_path = path.join(file);
                            if !file_path.exists() {
                                continue;
                            }
                            let mut source = std::fs::File::open(file_path)?;
                            let mut buf = [0u8; 65536];
                            loop {
                                let n = source.read(&mut buf)?;
                                if n == 0 {
                                    break;
                                }
                                for byte in &buf[..n] {
                                    hash ^= *byte as u64;
                                    hash = hash.wrapping_mul(0x100000001b3);
                                }
                            }
                        }
                        return Ok(format!("card-v1:fastembed-7.1.0:custom:{:016x}", hash));
                    }
                };
                format!("fastembed-7.1.0:{}", canonical)
            }
            other => anyhow::bail!(
                "Unknown embedding backend: {}. Use onnx, ollama, or hash",
                other
            ),
        };
        Ok(format!("card-v1:{}", model))
    }

    pub fn active_embedding_summary(&self) -> String {
        match self.backend.as_str() {
            "onnx" => match self.onnx_model.as_deref() {
                Some("minilm") | Some("all-minilm-l6-v2") | Some("Xenova/all-MiniLM-L6-v2") => {
                    "Xenova/all-MiniLM-L6-v2 (384-dim local ONNX)".to_string()
                }
                Some("nomic")
                | Some("nomic-embed-text")
                | Some("nomic-ai/nomic-embed-text-v1.5") => {
                    "nomic-ai/nomic-embed-text-v1.5 (768-dim local ONNX)".to_string()
                }
                Some("bge-small")
                | Some("bge-small-en-v1.5")
                | Some("Xenova/bge-small-en-v1.5")
                | None => "Xenova/bge-small-en-v1.5 (384-dim local ONNX)".to_string(),
                Some(custom) => format!("{} (custom local ONNX)", custom),
            },
            "ollama" => format!(
                "{} (via Ollama @ {})",
                self.embedding_model, self.ollama_url
            ),
            "hash" => "deterministic-trigram-hash (384-dim offline)".to_string(),
            other => other.to_string(),
        }
    }

    pub fn active_reranker_summary(&self) -> String {
        let norm = Self::normalize_reranker(&self.reranker, None);
        if let Some(ollama_model) = norm.strip_prefix("ollama:") {
            return format!("ollama:{} (@ {})", ollama_model, self.ollama_url);
        }
        match norm.as_str() {
            "tinyroberta-squad2" => "tinyroberta-squad2 (local ONNX Extractive QA)".to_string(),
            "minilm-squad2" => "minilm-squad2 (local ONNX Extractive QA)".to_string(),
            "ollama" => format!("ollama:auto (@ {})", self.ollama_url),
            "top1" => "top1 (0ms vector rank-1)".to_string(),
            other => format!("{} (local ONNX Extractive QA)", other),
        }
    }

    pub fn config_dir() -> PathBuf {
        std::env::var_os("LIGHTMEM_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".lightmem")
            })
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn global_db_path() -> PathBuf {
        Self::config_dir().join("memories.db")
    }

    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    pub fn try_load() -> Result<Self> {
        let path = Self::config_file();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let raw_val: serde_json::Value = serde_json::from_str(&content)
            .with_context(|| format!("Invalid configuration at {}", path.display()))?;
        let legacy_qa = raw_val.get("qa_model").and_then(|v| v.as_str());
        let mut cfg: Self = serde_json::from_value(raw_val.clone())
            .with_context(|| format!("Invalid configuration at {}", path.display()))?;
        cfg.reranker = Self::normalize_reranker(&cfg.reranker, legacy_qa);
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config dir {:?}", dir))?;

        let json = serde_json::to_string_pretty(self)?;
        let temp = dir.join(format!("config-{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temp, json).with_context(|| "Failed to write config file")?;
        if let Err(error) = std::fs::rename(&temp, Self::config_file()) {
            let _ = std::fs::remove_file(temp);
            return Err(error.into());
        }
        Ok(())
    }

    /// Resolve active database path:
    /// 1. If force_global: always ~/.lightmem/memories.db
    /// 2. If ./.lightmem.db exists in current directory or git root: return local project db
    /// 3. Otherwise: ~/.lightmem/memories.db
    pub fn resolve_db_path(force_global: bool) -> PathBuf {
        if let Ok(custom_path) = std::env::var("LIGHTMEM_DB") {
            let trimmed = custom_path.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }

        if force_global {
            return Self::global_db_path();
        }

        // Check current directory
        let local_cwd = Path::new(".lightmem.db");
        if local_cwd.exists() {
            return local_cwd.to_path_buf();
        }

        // Check git root if available
        if let Ok(current_dir) = std::env::current_dir() {
            let mut curr = current_dir.as_path();
            loop {
                let candidate = curr.join(".lightmem.db");
                if candidate.exists() {
                    return candidate;
                }
                let git_dir = curr.join(".git");
                if git_dir.exists() {
                    break;
                }
                match curr.parent() {
                    Some(parent) => curr = parent,
                    None => break,
                }
            }
        }

        Self::global_db_path()
    }
}
