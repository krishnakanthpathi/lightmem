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
    "needle".to_string()
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
    pub fn active_embedding_summary(&self) -> String {
        match self.backend.as_str() {
            "onnx" => match self.onnx_model.as_deref() {
                Some("minilm") | Some("all-minilm-l6-v2") | Some("Xenova/all-MiniLM-L6-v2") => {
                    "Xenova/all-MiniLM-L6-v2 (384-dim local ONNX)".to_string()
                }
                Some("nomic") | Some("nomic-embed-text") | Some("nomic-ai/nomic-embed-text-v1.5") => {
                    "nomic-ai/nomic-embed-text-v1.5 (768-dim local ONNX)".to_string()
                }
                Some("bge-small") | Some("bge-small-en-v1.5") | Some("Xenova/bge-small-en-v1.5") | None => {
                    "Xenova/bge-small-en-v1.5 (384-dim local ONNX)".to_string()
                }
                Some(custom) => format!("{} (custom local ONNX)", custom),
            },
            "ollama" => format!("{} (via Ollama @ {})", self.embedding_model, self.ollama_url),
            "hash" => "deterministic-trigram-hash (384-dim offline)".to_string(),
            other => other.to_string(),
        }
    }

    pub fn active_reranker_summary(&self) -> String {
        match self.reranker.as_str() {
            "needle" | "precision" => "needle-3 (Native C-FFI · needle3.cact)".to_string(),
            "top1" => "top1 (0ms vector rank-1)".to_string(),
            other => other.to_string(),
        }
    }

    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".lightmem")
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn global_db_path() -> PathBuf {
        Self::config_dir().join("memories.db")
    }

    pub fn load() -> Self {
        let path = Self::config_file();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(mut cfg) = serde_json::from_str::<LightMemConfig>(&content) {
                    let mut migrated = false;
                    if cfg.onnx_model.as_deref() == Some("bge-small") {
                        cfg.onnx_model = Some("Xenova/bge-small-en-v1.5".to_string());
                        migrated = true;
                    }
                    if cfg.ollama_url == "http://100.75.149.115:7777" {
                        cfg.ollama_url = "http://localhost:11434".to_string();
                        migrated = true;
                    }
                    if migrated {
                        let _ = cfg.save();
                    }
                    return cfg;
                }
            }
        }
        let cfg = Self::default();
        let _ = cfg.save();
        cfg
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config dir {:?}", dir))?;

        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::config_file(), json).with_context(|| "Failed to write config file")?;
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
