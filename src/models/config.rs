use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Helper to format an HTTP Authorization header value as Bearer token
pub fn format_auth_header(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.to_lowercase().starts_with("bearer ") {
        trimmed.to_string()
    } else {
        format!("Bearer {}", trimmed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightMemConfig {
    pub backend: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onnx_model: Option<String>,
    #[serde(
        rename = "openai_endpoint",
        alias = "ollama_url",
        alias = "endpoint",
        alias = "url",
        alias = "openai_url"
    )]
    pub ollama_url: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "openai_api_key",
        alias = "ollama_api_key",
        alias = "model_api_key",
        alias = "openai_key"
    )]
    pub ollama_api_key: Option<String>,
    pub embedding_model: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "embedding_url"
    )]
    pub embedding_endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_api_key: Option<String>,
    #[serde(
        default = "default_reranker",
        rename = "openai_model",
        alias = "reranker",
        alias = "model",
        alias = "answer_model",
        alias = "chat_model"
    )]
    pub reranker: String,
    /// Explicit opt-in for sending remembered text to the configured LLM.
    #[serde(default = "default_entity_extraction")]
    pub entity_extraction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

fn default_reranker() -> String {
    "minilm-squad2".to_string()
}

fn default_entity_extraction() -> String {
    "off".to_string()
}

impl Default for LightMemConfig {
    fn default() -> Self {
        Self {
            backend: "onnx".to_string(),
            onnx_model: Some("Xenova/bge-small-en-v1.5".to_string()),
            ollama_url: "http://localhost:11434".to_string(),
            ollama_api_key: None,
            embedding_model: "nomic-embed-text".to_string(),
            embedding_endpoint: None,
            embedding_api_key: None,
            reranker: default_reranker(),
            entity_extraction: default_entity_extraction(),
            remote_url: None,
            api_key: None,
        }
    }
}

impl LightMemConfig {
    pub fn offline_requested() -> bool {
        match std::env::var("LIGHTMEM_OFFLINE") {
            Ok(val) => {
                let v = val.trim().to_lowercase();
                !v.is_empty() && v != "0" && v != "false" && v != "no" && v != "off"
            }
            Err(_) => false,
        }
    }
    /// Normalize any reranker string ('onnx', 'minilm-squad2', 'tinyroberta-squad2', 'ollama', 'ollama:<model>', 'top1', 'openai:<model>', etc.)
    pub fn normalize_reranker(raw: &str, legacy_qa: Option<&str>) -> String {
        let trimmed = raw.trim();
        let lower = trimmed.to_lowercase();
        if lower == "top1" {
            return "top1".to_string();
        }
        for prefix in &[
            "openai:",
            "openrouter:",
            "gemini:",
            "groq:",
            "deepseek:",
            "mistral:",
            "ollama:",
            "llm:",
        ] {
            if trimmed.len() > prefix.len() && trimmed[..prefix.len()].eq_ignore_ascii_case(prefix)
            {
                let clean_p = if prefix.eq_ignore_ascii_case("llm:") {
                    "ollama:"
                } else {
                    *prefix
                };
                return format!("{}{}", clean_p, trimmed[prefix.len()..].trim());
            }
        }
        if let Some(onnx_m) = trimmed
            .strip_prefix("onnx:")
            .or_else(|| trimmed.strip_prefix("ONNX:"))
        {
            return Self::normalize_reranker(onnx_m, None);
        }
        match lower.as_str() {
            "ollama" | "openai" | "openrouter" | "gemini" | "groq" | "deepseek" | "mistral"
            | "llm" => {
                if let Some(qm) = legacy_qa.map(str::trim).filter(|m| {
                    !m.is_empty() && !matches!(*m, "minilm-squad2" | "tinyroberta-squad2")
                }) {
                    format!("{}:{}", lower, qm)
                } else {
                    lower
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

    /// Apply a standard setup preset: 'local', 'ollama', 'openai', 'openrouter', 'gemini', 'groq', 'deepseek', 'mistral', 'custom', 'demo'
    pub fn apply_preset(&mut self, preset: &str) -> Result<()> {
        match preset.to_lowercase().as_str() {
            "demo" | "hash" => {
                self.backend = "hash".to_string();
                self.reranker = "top1".to_string();
                self.entity_extraction = "off".to_string();
                Ok(())
            }
            "local" | "offline" | "onnx" => {
                self.entity_extraction = "off".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "minilm-squad2".to_string();
                self.ollama_url = "http://localhost:11434".to_string();
                Ok(())
            }
            "ollama-cloud" | "cloud" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "ollama:gemma4:31b-cloud".to_string();
                self.ollama_url = "http://localhost:11434".to_string();
                Ok(())
            }
            "ollama-local" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "ollama".to_string();
                self.ollama_url = "http://localhost:11434".to_string();
                Ok(())
            }
            "ollama" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "ollama".to_string();
                self.ollama_url = "http://localhost:11434".to_string();
                Ok(())
            }
            "openai" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "openai:gpt-4o-mini".to_string();
                self.ollama_url = "https://api.openai.com".to_string();
                Ok(())
            }
            "openrouter" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "openrouter:anthropic/claude-3.5-sonnet".to_string();
                self.ollama_url = "https://openrouter.ai/api".to_string();
                Ok(())
            }
            "gemini" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "gemini:gemini-1.5-flash".to_string();
                self.ollama_url = "https://generativelanguage.googleapis.com/v1beta/openai".to_string();
                Ok(())
            }
            "groq" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "groq:llama-3.3-70b-versatile".to_string();
                self.ollama_url = "https://api.groq.com/openai".to_string();
                Ok(())
            }
            "deepseek" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "deepseek:deepseek-chat".to_string();
                self.ollama_url = "https://api.deepseek.com".to_string();
                Ok(())
            }
            "mistral" => {
                self.entity_extraction = "on".to_string();
                self.backend = "onnx".to_string();
                self.onnx_model = Some("bge-small".to_string());
                self.embedding_model = "bge-small".to_string();
                self.reranker = "mistral:mistral-small-latest".to_string();
                self.ollama_url = "https://api.mistral.ai".to_string();
                Ok(())
            }
            "custom" => {
                if !self.reranker.starts_with("custom:") {
                    self.reranker = "custom".to_string();
                }
                Ok(())
            }
            other => anyhow::bail!(
                "Unknown preset '{}'. Choose 'local' (offline ONNX), 'ollama' (localhost LLM), 'openai', 'openrouter', 'gemini', 'groq', 'deepseek', 'mistral', 'custom', or 'demo'.",
                other
            ),
        }
    }

    /// Normalizes an Ollama endpoint URL by trimming whitespace, trailing slashes,
    /// and stripping trailing path suffixes like `/api/chat`, `/api/generate`, `/api/embeddings`, `/api/embed`, `/api`, or `/v1`.
    pub fn normalize_ollama_url(raw: &str) -> String {
        let mut u = raw.trim().trim_end_matches('/').to_string();
        for suffix in [
            "/api/chat",
            "/api/generate",
            "/api/embeddings",
            "/api/embed",
            "/api",
            "/v1",
        ] {
            if u.ends_with(suffix) {
                if suffix == "/api" && u.contains("openrouter.ai") {
                    continue;
                }
                u = u[..u.len() - suffix.len()]
                    .trim_end_matches('/')
                    .to_string();
            }
        }
        if u.is_empty() {
            "http://localhost:11434".to_string()
        } else {
            u
        }
    }

    /// Returns (provider_identifier, default_base_url, optional_primary_env_var_for_api_key)
    pub fn active_provider(&self) -> (&'static str, &'static str, Option<&'static str>) {
        let norm = Self::normalize_reranker(&self.reranker, None);
        let lower = norm.to_lowercase();
        if lower.starts_with("openai:") || lower == "openai" {
            ("openai", "https://api.openai.com", Some("OPENAI_API_KEY"))
        } else if lower.starts_with("openrouter:") || lower == "openrouter" {
            (
                "openrouter",
                "https://openrouter.ai/api",
                Some("OPENROUTER_API_KEY"),
            )
        } else if lower.starts_with("gemini:") || lower == "gemini" {
            (
                "gemini",
                "https://generativelanguage.googleapis.com/v1beta/openai",
                Some("GEMINI_API_KEY"),
            )
        } else if lower.starts_with("groq:") || lower == "groq" {
            ("groq", "https://api.groq.com/openai", Some("GROQ_API_KEY"))
        } else if lower.starts_with("deepseek:") || lower == "deepseek" {
            (
                "deepseek",
                "https://api.deepseek.com",
                Some("DEEPSEEK_API_KEY"),
            )
        } else if lower.starts_with("mistral:") || lower == "mistral" {
            ("mistral", "https://api.mistral.ai", Some("MISTRAL_API_KEY"))
        } else if lower == "top1" {
            ("top1", "", None)
        } else if lower == "custom" || lower.starts_with("custom:") {
            ("custom", "", None)
        } else if lower == "minilm-squad2"
            || lower == "tinyroberta-squad2"
            || lower == "onnx"
            || lower == "qa"
        {
            ("onnx", "", None)
        } else {
            ("ollama", "http://localhost:11434", Some("OLLAMA_API_KEY"))
        }
    }

    /// Returns true when `url` matches one of the built-in provider default base URLs.
    pub fn is_known_provider_default_url(url: &str) -> bool {
        let norm = Self::normalize_ollama_url(url);
        matches!(
            norm.as_str(),
            "http://localhost:11434"
                | "https://api.openai.com"
                | "https://openrouter.ai/api"
                | "https://generativelanguage.googleapis.com/v1beta/openai"
                | "https://api.groq.com/openai"
                | "https://api.deepseek.com"
                | "https://api.mistral.ai"
        )
    }

    /// Aligns `ollama_url` when switching reranker/chat providers unless a custom endpoint was set or preserved.
    pub fn align_provider_endpoint(
        &mut self,
        previous_default_url: Option<&str>,
        explicit_url: Option<&str>,
    ) {
        if let Some(url) = explicit_url {
            self.ollama_url = Self::normalize_ollama_url(url);
            return;
        }
        let (_new_prov, new_default_url, _) = self.active_provider();
        if new_default_url.is_empty() {
            return;
        }
        let current = Self::normalize_ollama_url(&self.ollama_url);
        let prev_matches = previous_default_url
            .filter(|p| !p.is_empty())
            .map(|p| current == p)
            .unwrap_or(false);
        if self.ollama_url.trim().is_empty()
            || current == "http://localhost:11434"
            || prev_matches
            || Self::is_known_provider_default_url(&current)
        {
            self.ollama_url = new_default_url.to_string();
        }
    }

    /// Validates and normalizes an answer engine (`reranker`) specification string.
    pub fn parse_reranker_setting(raw: &str) -> Result<String> {
        let trimmed = raw.trim();
        let lower = trimmed.to_lowercase();
        if let Some(model) = lower.strip_prefix("onnx:") {
            anyhow::ensure!(
                matches!(
                    model,
                    "minilm-squad2" | "tinyroberta-squad2" | "minilm" | "tinyroberta"
                ),
                "Unknown ONNX answer model: {}",
                model
            );
        }
        anyhow::ensure!(
            matches!(
                lower.as_str(),
                "onnx"
                    | "qa"
                    | "precision"
                    | "ollama"
                    | "openai"
                    | "openrouter"
                    | "gemini"
                    | "groq"
                    | "deepseek"
                    | "mistral"
                    | "llm"
                    | "top1"
                    | "custom"
                    | "minilm-squad2"
                    | "tinyroberta-squad2"
            ) || lower.starts_with("onnx:")
                || lower.starts_with("custom:")
                || lower.contains(':'),
            "Invalid answer engine '{}'. Use minilm-squad2, tinyroberta-squad2, top1, or a model spec like ollama:<model>, openai:<model>, openrouter:<model>, gemini:<model>, groq:<model>, deepseek:<model>, mistral:<model>, custom:<model>.",
            raw
        );
        let normalized = Self::normalize_reranker(trimmed, None);
        anyhow::ensure!(
            !normalized.ends_with(':'),
            "Answer engine needs a model after ':'"
        );
        Ok(normalized)
    }

    /// Validates an entity extraction toggle value strictly (`on|true|1|yes` or `off|false|0|no`).
    pub fn parse_entity_extraction(raw: &str) -> Result<String> {
        match raw.trim().to_lowercase().as_str() {
            "on" | "true" | "1" | "yes" => Ok("on".to_string()),
            "off" | "false" | "0" | "no" => Ok("off".to_string()),
            _ => anyhow::bail!(
                "Invalid entity_extraction mode '{}'. Use 'on' or 'off'.",
                raw.trim()
            ),
        }
    }

    /// Validates an embedding backend name (`onnx`, `ollama`, or `hash`).
    pub fn parse_embedding_backend(raw: &str) -> Result<String> {
        let lower = raw.trim().to_lowercase();
        anyhow::ensure!(
            matches!(lower.as_str(), "onnx" | "ollama" | "hash"),
            "Invalid backend '{}'. Use 'onnx', 'ollama', or 'hash'.",
            raw.trim()
        );
        Ok(lower)
    }

    fn normalize_optional_value(raw: &str) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    /// Canonical field names accepted by `config get`.
    pub fn valid_field_names() -> &'static [&'static str] {
        &[
            "backend",
            "onnx_model",
            "embedding_model",
            "embedding_endpoint",
            "embedding_api_key",
            "reranker",
            "ollama_url",
            "ollama_api_key",
            "entity_extraction",
            "remote_url",
            "api_key",
            "provider",
            "endpoint",
            "effective_embedding_url",
        ]
    }

    /// Reads a configuration field (redacting secrets) using canonical names or common aliases.
    pub fn get_field(&self, key: &str) -> Result<serde_json::Value> {
        let redacted = self.redacted_json()?;
        let normalized = key.trim().to_lowercase();
        let canonical = match normalized.as_str() {
            "backend" | "embedding_backend" => "backend",
            "onnx_model" => "onnx_model",
            "embedding_model" => "embedding_model",
            "embedding_endpoint" | "embedding_url" => "embedding_endpoint",
            "effective_embedding_url" => "effective_embedding_url",
            "embedding_api_key" => {
                return Ok(redacted
                    .get("embedding_api_key")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null));
            }
            "embedding_api_key_configured" => "embedding_api_key_configured",
            "reranker" | "model" | "answer_model" | "chat_model" | "openai_model" => "reranker",
            "ollama_url" | "url" | "openai_endpoint" | "openai_url" => "ollama_url",
            "endpoint" => "endpoint",
            "ollama_api_key" | "model_api_key" | "openai_api_key" | "openai_key" => {
                return Ok(redacted
                    .get("ollama_api_key")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null));
            }
            "ollama_api_key_configured" | "openai_api_key_configured" => {
                "ollama_api_key_configured"
            }
            "entity_extraction" => "entity_extraction",
            "remote_url" | "remote" => {
                return Ok(redacted
                    .get("remote_url")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null));
            }
            "api_key" | "remote_api_key" => {
                return Ok(redacted
                    .get("api_key")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null));
            }
            "api_key_configured" => "api_key_configured",
            "provider" => "provider",
            other => anyhow::bail!(
                "Unknown config key '{}'. Valid keys: {}",
                other,
                Self::valid_field_names().join(", ")
            ),
        };
        Ok(redacted
            .get(canonical)
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Updates a mutable configuration field using canonical names or common aliases.
    pub fn set_field(&mut self, key: &str, value: &str) -> Result<()> {
        let normalized = key.trim().to_lowercase();
        match normalized.as_str() {
            "preset" => self.apply_preset(value.trim())?,
            "backend" | "embedding_backend" => {
                self.backend = Self::parse_embedding_backend(value)?;
            }
            "onnx_model" => {
                let opt = Self::normalize_optional_value(value);
                if let Some(ref m) = opt {
                    self.embedding_model = m.clone();
                }
                self.onnx_model = opt;
            }
            "embedding_model" => {
                let trimmed = value.trim();
                anyhow::ensure!(!trimmed.is_empty(), "embedding_model cannot be empty");
                self.embedding_model = trimmed.to_string();
                if self.backend == "onnx" {
                    self.onnx_model = Some(trimmed.to_string());
                }
            }
            "embedding_endpoint" | "embedding_url" => {
                self.embedding_endpoint =
                    Self::normalize_optional_value(value).map(|u| Self::normalize_ollama_url(&u));
            }
            "embedding_api_key" => {
                self.embedding_api_key = Self::normalize_optional_value(value);
            }
            "ollama_url" | "url" | "endpoint" | "openai_endpoint" | "openai_url" => {
                self.ollama_url = Self::normalize_ollama_url(value);
            }
            "ollama_api_key" | "model_api_key" | "openai_api_key" | "openai_key" => {
                self.ollama_api_key = Self::normalize_optional_value(value);
            }
            "reranker" | "model" | "answer_model" | "chat_model" | "openai_model" => {
                let (_, prev_default_url, _) = self.active_provider();
                let parsed = Self::parse_reranker_setting(value)?;
                self.reranker = parsed;
                self.align_provider_endpoint(Some(prev_default_url), None);
            }
            "entity_extraction" => {
                self.entity_extraction = Self::parse_entity_extraction(value)?;
            }
            "remote_url" | "remote" => {
                self.remote_url = Self::normalize_optional_value(value)
                    .map(|u| u.trim_end_matches('/').to_string());
            }
            "api_key" | "remote_api_key" => {
                self.api_key = Self::normalize_optional_value(value);
            }
            other => anyhow::bail!(
                "Unknown or read-only config key '{}'. Mutable keys: preset, backend, onnx_model, embedding_model, embedding_endpoint, embedding_api_key, reranker, ollama_url, ollama_api_key, entity_extraction, remote_url, api_key",
                other
            ),
        }
        Ok(())
    }

    /// Resolves the effective base URL for Ollama / OpenAI-compatible endpoints:
    /// Respects custom configured URL; otherwise defaults to active provider's default URL.
    pub fn effective_ollama_url(&self) -> String {
        let raw = self.ollama_url.trim();
        let (_prov, default_url, _key_env) = self.active_provider();
        if !raw.is_empty() && raw != "http://localhost:11434" && raw != default_url {
            return Self::normalize_ollama_url(raw);
        }
        if let Some(env_url) = std::env::var("LIGHTMEM_MODEL_URL")
            .ok()
            .or_else(|| std::env::var("OPENAI_BASE_URL").ok())
            .or_else(|| std::env::var("OPENAI_ENDPOINT").ok())
            .or_else(|| std::env::var("OLLAMA_URL").ok())
            .or_else(|| std::env::var("OLLAMA_HOST").ok())
            .filter(|s| !s.trim().is_empty())
        {
            return Self::normalize_ollama_url(&env_url);
        }
        if !default_url.is_empty() {
            return default_url.to_string();
        }
        Self::normalize_ollama_url(&self.ollama_url)
    }

    /// Resolves effective embedding endpoint URL for Ollama or custom embedding servers.
    pub fn effective_embedding_url(&self) -> String {
        if let Some(ref u) = self.embedding_endpoint {
            let trimmed = u.trim();
            if !trimmed.is_empty() {
                return Self::normalize_ollama_url(trimmed);
            }
        }
        if let Ok(env_url) = std::env::var("LIGHTMEM_EMBEDDING_URL") {
            let trimmed = env_url.trim();
            if !trimmed.is_empty() {
                return Self::normalize_ollama_url(trimmed);
            }
        }
        if self.backend == "ollama" {
            let raw = self.ollama_url.trim();
            let (prov, default_url, _) = self.active_provider();
            let is_cloud_provider_url = matches!(
                prov,
                "openai" | "gemini" | "groq" | "openrouter" | "deepseek" | "mistral"
            ) && raw == default_url;

            if !raw.is_empty() && raw != "http://localhost:11434" && !is_cloud_provider_url {
                return self.effective_ollama_url();
            }
            return "http://localhost:11434".to_string();
        }
        String::new()
    }

    /// Resolves effective embedding API key if using an authenticated remote embedding endpoint.
    pub fn effective_embedding_api_key(&self) -> Option<String> {
        let from_config = self
            .embedding_api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty() && !k.eq_ignore_ascii_case("none"))
            .map(String::from);
        if from_config.is_some() {
            return from_config;
        }
        std::env::var("LIGHTMEM_EMBEDDING_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
    }

    /// Resolves effective API key from config, provider-specific environment variables, or fallback environment variables.
    pub fn effective_ollama_api_key(&self) -> Option<String> {
        let (prov, _default_url, key_env) = self.active_provider();

        // 1. Check provider-specific environment variables first (CLI flag > env var > config > default)
        if let Some(env_name) = key_env {
            if let Ok(key) = std::env::var(env_name) {
                let trimmed = key.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        if prov == "gemini" {
            if let Ok(key) = std::env::var("GOOGLE_API_KEY") {
                let trimmed = key.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }

        // 2. Check config file key
        let from_config = self
            .ollama_api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty() && !k.eq_ignore_ascii_case("none"))
            .map(String::from);
        if from_config.is_some() {
            return from_config;
        }

        // 3. Provider-specific fallbacks only (never leak OPENAI_API_KEY to non-OpenAI providers or remote server keys)
        match prov {
            "openai" => std::env::var("OPENAI_API_KEY").ok(),
            "ollama" => std::env::var("OLLAMA_API_KEY")
                .or_else(|_| std::env::var("LMEM_OLLAMA_API_KEY"))
                .or_else(|_| std::env::var("LLM_API_KEY"))
                .ok(),
            _ => None,
        }
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
    }

    /// Returns true if an LLM is configured as the active answer engine.
    pub fn has_active_llm(&self) -> bool {
        let (prov, _, _) = self.active_provider();
        prov != "onnx" && prov != "top1"
    }

    /// Entity extraction now always uses the unified configured model; returns false.
    pub fn has_explicit_entity_llm(&self) -> bool {
        false
    }

    /// Returns a human-friendly provider descriptor for CLI display
    pub fn active_provider_summary(&self) -> (&'static str, String) {
        let (prov, _def_url, key_env) = self.active_provider();
        let eff_url = self.effective_ollama_url();
        if prov == "onnx"
            && (self.entity_extraction.eq_ignore_ascii_case("off") || !self.has_active_llm())
        {
            return (
                "Local",
                format!(
                    "{} (local ONNX Extractive QA, offline @ {})",
                    self.reranker, eff_url
                ),
            );
        }
        if prov == "top1" && self.entity_extraction.eq_ignore_ascii_case("off") {
            return (
                "Top-1",
                "top1 (direct rank-1 selection; no generative model)".to_string(),
            );
        }
        let eff_url = self.effective_ollama_url();
        let cred_disp = if self
            .ollama_api_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty() && !k.eq_ignore_ascii_case("none"))
            .is_some()
        {
            "configured (secret)".to_string()
        } else if let Some(env_name) = key_env {
            if std::env::var(env_name)
                .ok()
                .filter(|s| !s.trim().is_empty())
                .is_some()
            {
                format!("from {}", env_name)
            } else if prov == "gemini"
                && std::env::var("GOOGLE_API_KEY")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .is_some()
            {
                "from GOOGLE_API_KEY".to_string()
            } else if self.effective_ollama_api_key().is_some() {
                "configured (secret)".to_string()
            } else {
                "none".to_string()
            }
        } else {
            "none".to_string()
        };
        let prov_label = match prov {
            "openai" => "OpenAI",
            "openrouter" => "OpenRouter",
            "gemini" => "Gemini",
            "groq" => "Groq",
            "deepseek" => "DeepSeek",
            "mistral" => "Mistral",
            "onnx" | "local" => "Local ONNX",
            "top1" => "Top-1",
            "custom" => "Custom",
            _ => "Ollama",
        };
        let model_display = if self.reranker == "ollama" {
            format!("ollama (@ {})", eff_url)
        } else {
            format!("{} (@ {})", self.reranker, eff_url)
        };
        (
            prov_label,
            format!("{} · credential: {}", model_display, cred_disp),
        )
    }

    /// Resolves effective remote backend URL from environment variables (LIGHTMEM_REMOTE_URL, LIGHTMEM_URL) or config.
    pub fn effective_remote_url(&self) -> Option<String> {
        let raw = std::env::var("LIGHTMEM_REMOTE_URL")
            .ok()
            .or_else(|| std::env::var("LIGHTMEM_URL").ok())
            .or_else(|| self.remote_url.clone())?;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(trimmed.trim_end_matches('/').to_string())
        }
    }

    /// Resolves effective remote backend API key from environment variables (LIGHTMEM_API_KEY, LIGHTMEM_KEY) or config.
    pub fn effective_api_key(&self) -> Option<String> {
        let raw = std::env::var("LIGHTMEM_API_KEY")
            .ok()
            .or_else(|| std::env::var("LIGHTMEM_KEY").ok())
            .or_else(|| self.api_key.clone())?;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    pub fn effective_onnx_model(&self) -> String {
        self.onnx_model
            .clone()
            .unwrap_or_else(|| self.embedding_model.clone())
    }

    /// Stable embedding-space identity, including card preprocessing and engine version.
    pub fn embedding_identity(&self) -> Result<String> {
        let model = match self.backend.as_str() {
            "hash" => "hash:fnv-bigram-v1".to_string(),
            "ollama" => format!(
                "ollama:{}:{}",
                self.effective_embedding_url().trim_end_matches('/'),
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
                self.embedding_model,
                self.effective_embedding_url()
            ),
            "hash" => "deterministic hash (384-dim keyword/demo mode)".to_string(),
            other => other.to_string(),
        }
    }

    pub fn active_reranker_summary(&self) -> String {
        let norm = Self::normalize_reranker(&self.reranker, None);
        let eff_url = self.effective_ollama_url();
        for prefix in &[
            "openai:",
            "openrouter:",
            "gemini:",
            "groq:",
            "deepseek:",
            "mistral:",
            "ollama:",
        ] {
            if let Some(model) = norm.strip_prefix(prefix) {
                let prov = match *prefix {
                    "openai:" => "OpenAI",
                    "openrouter:" => "OpenRouter",
                    "gemini:" => "Gemini",
                    "groq:" => "Groq",
                    "deepseek:" => "DeepSeek",
                    "mistral:" => "Mistral",
                    _ => "ollama",
                };
                return format!("{}:{} (@ {})", prov, model, eff_url);
            }
        }
        match norm.as_str() {
            "tinyroberta-squad2" => "tinyroberta-squad2 (local ONNX Extractive QA)".to_string(),
            "minilm-squad2" => "minilm-squad2 (local ONNX Extractive QA)".to_string(),
            "ollama" => format!("ollama (@ {})", eff_url),
            "openai" => format!("OpenAI:gpt-4o-mini (@ {})", eff_url),
            "openrouter" => format!("OpenRouter:claude-3.5-sonnet (@ {})", eff_url),
            "gemini" => format!("Gemini:gemini-1.5-flash (@ {})", eff_url),
            "groq" => format!("Groq:llama-3.3-70b (@ {})", eff_url),
            "top1" => "top1 (best retrieved memory; no answer model)".to_string(),
            other => format!("{} (local ONNX Extractive QA)", other),
        }
    }

    pub fn active_entity_extraction_summary(&self) -> String {
        let norm = self.entity_extraction.trim();
        if norm.eq_ignore_ascii_case("off") || norm.eq_ignore_ascii_case("false") || norm.is_empty()
        {
            if !self.has_active_llm() {
                return "skipped (requires LLM)".to_string();
            }
            return "off".to_string();
        }
        if !self.has_active_llm() {
            return "skipped (requires LLM)".to_string();
        }
        "on".to_string()
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
        match Self::try_load() {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!(
                    "Warning: Failed to load configuration: {}. Using default.",
                    e
                );
                Self::default()
            }
        }
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
        if cfg.backend == "onnx" {
            let emb = cfg.embedding_model.trim().to_lowercase();
            let is_chat = emb.contains("qwen")
                || emb.contains("llama")
                || emb.contains("mistral")
                || emb.contains("gemma")
                || emb.contains("coder")
                || emb.contains("chat");
            if is_chat {
                if cfg.reranker == "ollama" || cfg.reranker.is_empty() {
                    cfg.reranker = format!("ollama:{}", cfg.embedding_model);
                }
                cfg.embedding_model = cfg
                    .onnx_model
                    .clone()
                    .unwrap_or_else(|| "bge-small".to_string());
            }
            if cfg.onnx_model.is_none() && !cfg.embedding_model.is_empty() {
                cfg.onnx_model = Some(cfg.embedding_model.clone());
            }
        }
        let mode = cfg.entity_extraction.trim().to_lowercase();
        cfg.entity_extraction = if mode == "off" || mode == "false" || mode.is_empty() {
            "off".to_string()
        } else {
            "on".to_string()
        };
        Ok(cfg)
    }

    pub fn redacted_json(&self) -> Result<serde_json::Value> {
        let mut value = serde_json::to_value(self)?;
        for field in [
            "api_key",
            "ollama_api_key",
            "openai_api_key",
            "embedding_api_key",
        ] {
            if value.get(field).is_some() {
                value[field] = serde_json::json!("[REDACTED]");
            }
        }
        if let Some(ref k) = self.ollama_api_key {
            if !k.trim().is_empty() {
                value["ollama_api_key"] = serde_json::json!("[REDACTED]");
                value["openai_api_key"] = serde_json::json!("[REDACTED]");
            }
        }
        if let Some(ref k) = self.embedding_api_key {
            if !k.trim().is_empty() {
                value["embedding_api_key"] = serde_json::json!("[REDACTED]");
            }
        }
        let (prov, _, _) = self.active_provider();
        value["provider"] = serde_json::json!(prov);
        value["model"] = serde_json::json!(self.reranker);
        value["endpoint"] = serde_json::json!(self.effective_ollama_url());
        value["ollama_url"] = serde_json::json!(self.ollama_url);
        value["openai_endpoint"] = serde_json::json!(self.ollama_url);
        value["reranker"] = serde_json::json!(self.reranker);
        value["openai_model"] = serde_json::json!(self.reranker);
        value["embedding_backend"] = serde_json::json!(self.backend);
        value["embedding_endpoint"] = serde_json::json!(self.embedding_endpoint);
        value["effective_embedding_url"] = serde_json::json!(self.effective_embedding_url());
        let norm_entity = if self.entity_extraction != "off" && !self.entity_extraction.is_empty() {
            "on"
        } else {
            "off"
        };
        value["entity_extraction"] = serde_json::json!(norm_entity);
        if self.backend == "onnx" {
            value["onnx_model"] = serde_json::json!(self.effective_onnx_model());
        }
        value["api_key_configured"] = serde_json::json!(self.effective_api_key().is_some());
        value["ollama_api_key_configured"] =
            serde_json::json!(self.effective_ollama_api_key().is_some());
        value["openai_api_key_configured"] =
            serde_json::json!(self.effective_ollama_api_key().is_some());
        value["embedding_api_key_configured"] =
            serde_json::json!(self.effective_embedding_api_key().is_some());
        Ok(value)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config dir {:?}", dir))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }

        let mut to_save = self.clone();
        let norm_entity =
            if to_save.entity_extraction != "off" && !to_save.entity_extraction.is_empty() {
                "on"
            } else {
                "off"
            };
        to_save.entity_extraction = norm_entity.to_string();

        let json = serde_json::to_string_pretty(&to_save)?;
        let temp = dir.join(format!("config-{}.tmp", uuid::Uuid::new_v4()));
        std::fs::write(&temp, json).with_context(|| "Failed to write config file")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600));
        }
        if let Err(error) = std::fs::rename(&temp, Self::config_file()) {
            let _ = std::fs::remove_file(temp);
            return Err(error.into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                Self::config_file(),
                std::fs::Permissions::from_mode(0o600),
            );
        }
        Ok(())
    }

    /// Resolve active database path:
    /// 1. If LIGHTMEM_DB environment variable is set: return custom database path
    /// 2. If force_global: always ~/.lightmem/memories.db
    /// 3. If ./.lightmem.db exists in current directory or git root: return local project db
    /// 4. Otherwise: ~/.lightmem/memories.db
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_auth_header() {
        assert_eq!(format_auth_header("my-secret-key"), "Bearer my-secret-key");
        assert_eq!(
            format_auth_header("Bearer already-prefixed"),
            "Bearer already-prefixed"
        );
        assert_eq!(format_auth_header("bearer lower-case"), "bearer lower-case");
        assert_eq!(
            format_auth_header("  padded-token  "),
            "Bearer padded-token"
        );
    }

    #[test]
    fn test_effective_ollama_api_key_resolution() {
        let mut cfg = LightMemConfig::default();
        assert_eq!(cfg.effective_ollama_api_key(), None);

        cfg.ollama_api_key = Some("test-key-123".to_string());
        assert_eq!(
            cfg.effective_ollama_api_key(),
            Some("test-key-123".to_string())
        );

        cfg.ollama_api_key = Some("none".to_string());
        assert_eq!(cfg.effective_ollama_api_key(), None);

        cfg.ollama_api_key = Some("  ".to_string());
        assert_eq!(cfg.effective_ollama_api_key(), None);
    }

    #[test]
    fn test_normalize_ollama_url() {
        assert_eq!(
            LightMemConfig::normalize_ollama_url("https://ollama.com/api/chat"),
            "https://ollama.com"
        );
        assert_eq!(
            LightMemConfig::normalize_ollama_url("https://ollama.com/api/generate"),
            "https://ollama.com"
        );
        assert_eq!(
            LightMemConfig::normalize_ollama_url("https://ollama.com/api/"),
            "https://ollama.com"
        );
        assert_eq!(
            LightMemConfig::normalize_ollama_url("http://localhost:11434/api"),
            "http://localhost:11434"
        );
        assert_eq!(
            LightMemConfig::normalize_ollama_url("http://localhost:11434/v1/"),
            "http://localhost:11434"
        );
        assert_eq!(
            LightMemConfig::normalize_ollama_url(""),
            "http://localhost:11434"
        );
    }

    #[test]
    fn test_effective_ollama_url_resolution() {
        let mut cfg = LightMemConfig::default();
        // Default URL is always localhost:11434
        assert_eq!(cfg.effective_ollama_url(), "http://localhost:11434");

        // When API key is provided, default URL remains localhost:11434 unless explicitly changed
        cfg.ollama_api_key = Some("test-api-key-123".to_string());
        assert_eq!(cfg.effective_ollama_url(), "http://localhost:11434");

        // Explicit custom URL is preserved and normalized
        cfg.ollama_url = "https://custom-ollama-proxy.internal/api/chat".to_string();
        assert_eq!(
            cfg.effective_ollama_url(),
            "https://custom-ollama-proxy.internal"
        );
    }

    #[test]
    fn test_provider_prefixes_and_presets() {
        let mut cfg = LightMemConfig::default();

        // Presets
        cfg.apply_preset("openai").unwrap();
        assert_eq!(cfg.reranker, "openai:gpt-4o-mini");
        assert_eq!(cfg.effective_ollama_url(), "https://api.openai.com");

        cfg.apply_preset("openrouter").unwrap();
        assert_eq!(cfg.reranker, "openrouter:anthropic/claude-3.5-sonnet");
        assert_eq!(cfg.effective_ollama_url(), "https://openrouter.ai/api");

        cfg.apply_preset("gemini").unwrap();
        assert_eq!(cfg.reranker, "gemini:gemini-1.5-flash");
        assert_eq!(
            cfg.effective_ollama_url(),
            "https://generativelanguage.googleapis.com/v1beta/openai"
        );

        cfg.apply_preset("groq").unwrap();
        assert_eq!(cfg.reranker, "groq:llama-3.3-70b-versatile");
        assert_eq!(cfg.effective_ollama_url(), "https://api.groq.com/openai");

        cfg.apply_preset("deepseek").unwrap();
        assert_eq!(cfg.reranker, "deepseek:deepseek-chat");
        assert_eq!(cfg.effective_ollama_url(), "https://api.deepseek.com");

        cfg.apply_preset("mistral").unwrap();
        assert_eq!(cfg.reranker, "mistral:mistral-small-latest");
        assert_eq!(cfg.effective_ollama_url(), "https://api.mistral.ai");

        cfg.apply_preset("local").unwrap();
        assert_eq!(cfg.reranker, "minilm-squad2");
        assert_eq!(cfg.entity_extraction, "off");

        // Embedding endpoint separation test
        let mut emb_cfg = LightMemConfig::default();
        emb_cfg.apply_preset("openai").unwrap();
        emb_cfg.backend = "ollama".to_string();
        assert_eq!(emb_cfg.effective_ollama_url(), "https://api.openai.com");
        // Embedding URL does not collide with OpenAI answer endpoint
        assert_eq!(emb_cfg.effective_embedding_url(), "http://localhost:11434");
        emb_cfg.embedding_endpoint = Some("http://remote-ollama:11434".to_string());
        assert_eq!(
            emb_cfg.effective_embedding_url(),
            "http://remote-ollama:11434"
        );

        // Normalization
        assert_eq!(
            LightMemConfig::normalize_reranker("OPENAI:gpt-4o", None),
            "openai:gpt-4o"
        );
        assert_eq!(
            LightMemConfig::normalize_reranker("openrouter:meta-llama/llama-3", None),
            "openrouter:meta-llama/llama-3"
        );
        assert_eq!(
            LightMemConfig::normalize_reranker("gemini:gemini-2.0-flash", None),
            "gemini:gemini-2.0-flash"
        );
        assert_eq!(
            LightMemConfig::normalize_reranker("groq:mixtral", None),
            "groq:mixtral"
        );
    }

    #[test]
    fn test_endpoint_alignment_field_access_and_parsers() {
        let mut cfg = LightMemConfig {
            reranker: "groq:llama-3.3-70b-versatile".to_string(),
            ollama_url: "https://api.openai.com".to_string(),
            ..Default::default()
        };
        cfg.align_provider_endpoint(Some("https://api.openai.com"), None);
        assert_eq!(cfg.ollama_url, "https://api.groq.com/openai");

        // Custom URL must never be overwritten when switching provider
        cfg.ollama_url = "https://custom-proxy.internal".to_string();
        cfg.reranker = "openai:gpt-4o".to_string();
        cfg.align_provider_endpoint(Some("https://api.groq.com/openai"), None);
        assert_eq!(cfg.ollama_url, "https://custom-proxy.internal");

        // Remote api_key must not count as LLM credential in provider summary
        cfg.ollama_url = "https://api.openai.com".to_string();
        cfg.reranker = "openai:gpt-4o-mini".to_string();
        cfg.api_key = Some("remote-server-key".to_string());
        cfg.ollama_api_key = None;
        let summary_without_model_key = cfg.active_provider_summary();
        cfg.ollama_api_key = Some("llm-provider-key".to_string());
        let summary_with_model_key = cfg.active_provider_summary();
        assert_ne!(summary_without_model_key.1, summary_with_model_key.1);
        assert!(
            summary_without_model_key.1.contains("credential: none"),
            "unexpected summary without model key: {:?}",
            summary_without_model_key
        );
        assert!(
            !summary_with_model_key.1.contains("credential: none"),
            "unexpected summary with model key: {:?}",
            summary_with_model_key
        );

        // Strict parsers
        assert_eq!(LightMemConfig::parse_entity_extraction("on").unwrap(), "on");
        assert_eq!(
            LightMemConfig::parse_entity_extraction("true").unwrap(),
            "on"
        );
        assert_eq!(
            LightMemConfig::parse_entity_extraction("off").unwrap(),
            "off"
        );
        assert!(LightMemConfig::parse_entity_extraction("invalid-mode").is_err());

        assert_eq!(
            LightMemConfig::parse_embedding_backend("onnx").unwrap(),
            "onnx"
        );
        assert_eq!(
            LightMemConfig::parse_embedding_backend("HASH").unwrap(),
            "hash"
        );
        assert!(LightMemConfig::parse_embedding_backend("pinecone").is_err());

        assert_eq!(
            LightMemConfig::parse_reranker_setting("tinyroberta-squad2").unwrap(),
            "tinyroberta-squad2"
        );
        assert!(LightMemConfig::parse_reranker_setting("   ").is_err());

        // Dynamic get_field / set_field
        assert!(cfg.set_field("entity_extraction", "on").is_ok());
        assert_eq!(cfg.get_field("entity_extraction").unwrap(), "on");
        assert!(cfg.set_field("backend", "hash").is_ok());
        assert_eq!(cfg.get_field("backend").unwrap(), "hash");
        assert!(cfg.get_field("nonexistent_key").is_err());
        assert!(cfg.set_field("nonexistent_key", "val").is_err());
    }
}
