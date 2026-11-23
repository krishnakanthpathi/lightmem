//! Short, line-oriented terminal interactions. Never open a controller for inspection.
use anyhow::{bail, ensure, Context, Result};
use colored::*;
use lightmem::{CliView, LightMemConfig, MemoryContext, MemoryScope};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::json;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

fn require_terminal() -> Result<()> {
    ensure!(io::stdin().is_terminal() && io::stderr().is_terminal(),
        "This prompt requires a terminal. Use explicit config/setup flags for scripts; see lmem config --help.");
    Ok(())
}

fn line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut answer = String::new();
    ensure!(
        io::stdin().read_line(&mut answer)? > 0,
        "Input closed. No settings saved."
    );
    Ok(answer.trim().to_string())
}

fn value(label: &str, current: &str) -> Result<String> {
    let input = line(&format!("{label} [{current}]: "))?;
    Ok(if input.is_empty() {
        current.into()
    } else {
        input
    })
}

pub fn confirm(prompt: &str, default: bool) -> Result<bool> {
    loop {
        let answer = line(&format!(
            "{prompt} {}: ",
            if default { "[Y/n]" } else { "[y/N]" }
        ))?;
        match answer.to_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => eprintln!("Enter y or n."),
        }
    }
}

pub fn validate(cfg: &LightMemConfig) -> Result<()> {
    cfg.embedding_identity()?;
    super::parse_reranker_setting(&cfg.reranker)?;
    let entity_clean = cfg.entity_extraction.trim().to_lowercase();
    ensure!(
        matches!(
            entity_clean.as_str(),
            "off" | "on" | "llm" | "true" | "false"
        ) || entity_clean.starts_with("llm:")
            || entity_clean.starts_with("ollama:")
            || entity_clean.starts_with("openai:")
            || entity_clean.starts_with("openrouter:")
            || entity_clean.starts_with("gemini:")
            || entity_clean.starts_with("groq:")
            || entity_clean.starts_with("deepseek:")
            || entity_clean.starts_with("mistral:"),
        "Entity extraction must be on, off, or <provider>:<model>."
    );
    if !cfg.ollama_url.trim().is_empty() {
        http_url(&cfg.ollama_url)?;
    }
    if let Some(url) = &cfg.embedding_endpoint {
        http_url(url)?;
    }
    if let Some(url) = &cfg.remote_url {
        http_url(url)?;
    }
    ensure!(
        cfg.backend != "ollama" || cfg.effective_embedding_url() != "https://ollama.com",
        "This endpoint provides generative models, not embeddings. Use --backend onnx for search."
    );
    ensure!(
        cfg.backend != "ollama" || !cfg.embedding_model.trim().is_empty(),
        "An Ollama embedding model is required."
    );
    Ok(())
}

fn http_url(url: &str) -> Result<()> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    ensure!(
        rest.is_some_and(|rest| !rest.split('/').next().unwrap_or("").is_empty())
            && !url.chars().any(char::is_whitespace),
        "An http:// or https:// endpoint with a host is required."
    );
    Ok(())
}

pub fn normalize_http_url(url: &str) -> Result<String> {
    let raw = url.trim();
    let with_scheme = if !raw.starts_with("http://") && !raw.starts_with("https://") {
        format!("http://{raw}")
    } else {
        raw.to_string()
    };
    http_url(&with_scheme)?;
    Ok(with_scheme.trim_end_matches('/').to_string())
}

pub fn prompt_remote(
    current_url: Option<&str>,
    current_key: Option<&str>,
) -> Result<(String, Option<String>)> {
    require_terminal()?;
    let default_url = current_url.unwrap_or("http://localhost:8080");
    let input = match line(&format!("Remote URL [{default_url}]: ")) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("\nOperation cancelled. No settings changed.");
            std::process::exit(130);
        }
    };
    let target = if input.is_empty() {
        default_url.to_string()
    } else {
        input
    };
    let clean_url = normalize_http_url(&target)?;

    let key_prompt = if current_key.is_some() {
        "API Key (press Enter to keep, or paste new key): "
    } else {
        "API Key (optional, press Enter to skip): "
    };
    let key_input = match line(key_prompt) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("\nOperation cancelled. No settings changed.");
            std::process::exit(130);
        }
    };
    let final_key = if key_input.is_empty() {
        current_key.map(String::from)
    } else if key_input.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(key_input)
    };

    Ok((clean_url, final_key))
}

pub fn ensure_unchanged(previous: &LightMemConfig) -> Result<()> {
    ensure!(
        serde_json::to_value(LightMemConfig::try_load()?)? == serde_json::to_value(previous)?,
        "Settings changed in another process. Run the command again; this draft was not saved."
    );
    Ok(())
}

fn prompt_api_key(cfg: &mut LightMemConfig, provider: &str) -> Result<()> {
    eprintln!();
    let has_key = cfg
        .ollama_api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some();
    if has_key {
        eprintln!(
            "  {} {} API key: {}",
            CliView::crimson("◈"),
            provider,
            CliView::emerald("configured (secret)")
        );
        if confirm(&format!("Update {provider} API key?"), false)? {
            eprint!("  Enter new {provider} API key (hidden): ");
            io::stderr().flush()?;
            let key = rpassword::read_password()?;
            if !key.trim().is_empty() {
                cfg.ollama_api_key = Some(key.trim().into());
            }
        }
    } else if confirm(&format!("Configure {provider} API key now?"), true)? {
        eprint!("  Enter {provider} API key (hidden): ");
        io::stderr().flush()?;
        let key = rpassword::read_password()?;
        if !key.trim().is_empty() {
            cfg.ollama_api_key = Some(key.trim().into());
        }
    }
    Ok(())
}

pub fn setup_configuration(previous: &LightMemConfig) -> Result<LightMemConfig> {
    require_terminal()?;
    eprintln!();
    eprintln!(
        "  {}  {}",
        CliView::crimson_bold("❖"),
        CliView::white_bold("L I G H T M E M  Interactive Setup")
    );
    eprintln!(
        "  {}",
        CliView::crimson("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
    );
    eprintln!(
        "  {} Config Directory: {}",
        CliView::slate("◫"),
        CliView::slate(&LightMemConfig::config_dir().display().to_string())
    );
    eprintln!();
    eprintln!(
        "  {}",
        CliView::slate("Choose an answer generation profile:")
    );
    eprintln!();
    eprintln!(
        "  ╭─ {}",
        CliView::crimson_bold("LOCAL & OFFLINE (No API Key Required)")
    );
    let row = |num: &str, name: &str, desc: &str, color_fn: fn(&str) -> colored::ColoredString| {
        let name_padded = format!("{:<18}", name);
        eprintln!(
            "  │  {} {} {}",
            CliView::slate(num),
            color_fn(&name_padded),
            CliView::slate(desc)
        );
    };

    row(
        " [1]",
        "Local ONNX",
        "100% offline, zero setup, private (bge-small + minilm) [Recommended]",
        CliView::emerald_bold,
    );
    row(
        " [2]",
        "Ollama Local",
        "Run local open-source LLMs via Ollama (localhost:11434)",
        CliView::emerald,
    );
    row(
        " [3]",
        "Offline Demo",
        "Deterministic hash search (testing & CI, no downloads)",
        CliView::emerald,
    );
    eprintln!();
    eprintln!(
        "  ╭─ {}",
        CliView::crimson_bold("CLOUD LLM PROVIDERS (Preconfigured Endpoints)")
    );
    row(
        " [4]",
        "OpenAI",
        "gpt-4o-mini (api.openai.com)",
        CliView::violet_bold,
    );
    row(
        " [5]",
        "Google Gemini",
        "gemini-1.5-flash (generativelanguage.googleapis.com)",
        CliView::violet_bold,
    );
    row(
        " [6]",
        "Groq",
        "llama-3.3-70b-versatile (api.groq.com)",
        CliView::violet_bold,
    );
    row(
        " [7]",
        "OpenRouter",
        "anthropic/claude-3.5-sonnet (openrouter.ai)",
        CliView::violet_bold,
    );
    row(
        " [8]",
        "DeepSeek",
        "deepseek-chat (api.deepseek.com)",
        CliView::violet_bold,
    );
    row(
        " [9]",
        "Mistral",
        "mistral-small-latest (api.mistral.ai)",
        CliView::violet_bold,
    );
    eprintln!();
    eprintln!("  ╭─ {}", CliView::crimson_bold("CUSTOM & NETWORKING"));
    row(
        "[10]",
        "Custom Endpoint",
        "Self-hosted OpenAI-compatible proxy / LLM",
        CliView::gold_bold,
    );
    row(
        "[11]",
        "Remote Vault",
        "Connect to a remote LightMem daemon",
        CliView::gold_bold,
    );
    eprintln!();
    eprintln!("  {} Cancel", CliView::slate("[0]"));
    eprintln!();

    let mut cfg = previous.clone();
    let choice = value(
        "Select profile [1-11] (Use recommended local settings)",
        "1",
    )?
    .to_lowercase();
    eprintln!();

    match choice.as_str() {
        "1" | "local" | "onnx" => cfg.apply_preset("local")?,
        "2" | "ollama" => {
            cfg.apply_preset("ollama")?;
            let model = value("Ollama model name", "qwen2.5:3b")?;
            let clean = model.strip_prefix("ollama:").unwrap_or(&model).trim();
            if !clean.is_empty() && clean != "qwen2.5:3b" {
                cfg.reranker = format!("ollama:{clean}");
            }
            let url = value("Ollama endpoint URL", "http://localhost:11434")?;
            if !url.is_empty() {
                cfg.ollama_url = LightMemConfig::normalize_ollama_url(&url);
            }
            cfg.entity_extraction = "on".into();
        }
        "3" | "demo" | "hash" => cfg.apply_preset("demo")?,
        "4" | "openai" => {
            cfg.apply_preset("openai")?;
            let model = value("Model name", "gpt-4o-mini")?;
            let clean = model.strip_prefix("openai:").unwrap_or(&model).trim();
            if !clean.is_empty() && clean != "gpt-4o-mini" {
                cfg.reranker = format!("openai:{clean}");
            }
            cfg.entity_extraction = "on".into();
            prompt_api_key(&mut cfg, "OpenAI")?;
        }
        "5" | "gemini" => {
            cfg.apply_preset("gemini")?;
            let model = value("Model name", "gemini-1.5-flash")?;
            let clean = model.strip_prefix("gemini:").unwrap_or(&model).trim();
            if !clean.is_empty() && clean != "gemini-1.5-flash" {
                cfg.reranker = format!("gemini:{clean}");
            }
            cfg.entity_extraction = "on".into();
            prompt_api_key(&mut cfg, "Google Gemini")?;
        }
        "6" | "groq" => {
            cfg.apply_preset("groq")?;
            let model = value("Model name", "llama-3.3-70b-versatile")?;
            let clean = model.strip_prefix("groq:").unwrap_or(&model).trim();
            if !clean.is_empty() && clean != "llama-3.3-70b-versatile" {
                cfg.reranker = format!("groq:{clean}");
            }
            cfg.entity_extraction = "on".into();
            prompt_api_key(&mut cfg, "Groq")?;
        }
        "7" | "openrouter" => {
            cfg.apply_preset("openrouter")?;
            let model = value("Model name", "anthropic/claude-3.5-sonnet")?;
            let clean = model.strip_prefix("openrouter:").unwrap_or(&model).trim();
            if !clean.is_empty() && clean != "anthropic/claude-3.5-sonnet" {
                cfg.reranker = format!("openrouter:{clean}");
            }
            cfg.entity_extraction = "on".into();
            prompt_api_key(&mut cfg, "OpenRouter")?;
        }
        "8" | "deepseek" => {
            cfg.apply_preset("deepseek")?;
            let model = value("Model name", "deepseek-chat")?;
            let clean = model.strip_prefix("deepseek:").unwrap_or(&model).trim();
            if !clean.is_empty() && clean != "deepseek-chat" {
                cfg.reranker = format!("deepseek:{clean}");
            }
            cfg.entity_extraction = "on".into();
            prompt_api_key(&mut cfg, "DeepSeek")?;
        }
        "9" | "mistral" => {
            cfg.apply_preset("mistral")?;
            let model = value("Model name", "mistral-small-latest")?;
            let clean = model.strip_prefix("mistral:").unwrap_or(&model).trim();
            if !clean.is_empty() && clean != "mistral-small-latest" {
                cfg.reranker = format!("mistral:{clean}");
            }
            cfg.entity_extraction = "on".into();
            prompt_api_key(&mut cfg, "Mistral")?;
        }
        "10" | "custom" => {
            cfg.apply_preset("custom")?;
            let url = value(
                "Custom OpenAI-compatible endpoint URL (e.g. http://localhost:8000/v1)",
                "http://localhost:8000/v1",
            )?;
            http_url(&url)?;
            cfg.ollama_url = LightMemConfig::normalize_ollama_url(&url);
            let model = value("Model name", "default")?;
            let clean = model.strip_prefix("custom:").unwrap_or(&model).trim();
            cfg.reranker = format!("custom:{clean}");
            cfg.entity_extraction = "on".into();
            prompt_api_key(&mut cfg, "Custom Endpoint")?;
        }
        "11" | "remote" => {
            let url = value(
                "Remote LightMem URL",
                cfg.remote_url.as_deref().unwrap_or("http://localhost:8080"),
            )?;
            http_url(&url)?;
            cfg.remote_url = Some(url.trim_end_matches('/').into());
            let present = cfg.effective_api_key().is_some();
            credential(&mut cfg.api_key, "remote LightMem", present)?;
            eprintln!("Remote client setup does not download local models. Test the server with lmem context.");
            return Ok(cfg);
        }
        "0" | "cancel" => bail!("Setup cancelled. No settings changed."),
        _ => bail!("Invalid choice '{choice}'. No settings changed."),
    }
    if cfg.effective_remote_url().is_some() {
        eprintln!("Remote routing is configured. Local presets do not clear routing.");
        if cfg.remote_url.is_some()
            && confirm(
                "Clear the saved remote URL/key to use local selection by default?",
                true,
            )?
        {
            cfg.remote_url = None;
            cfg.api_key = None;
        }
        if env_present("LIGHTMEM_REMOTE_URL").is_some() || env_present("LIGHTMEM_URL").is_some() {
            eprintln!("Environment routing still applies. Unset LIGHTMEM_REMOTE_URL/LIGHTMEM_URL or select --db/--global for local commands.");
        }
    }
    Ok(cfg)
}

fn credential(stored: &mut Option<String>, label: &str, effective_present: bool) -> Result<()> {
    eprintln!(
        "{label} credential: {}. Environment credentials may apply after removing a stored key.",
        if effective_present {
            "configured"
        } else {
            "not configured"
        }
    );
    match value("Action: keep, replace, remove", "keep")?.as_str() {
        "keep" => {}
        "remove" => *stored = None,
        "replace" => {
            eprint!("New {label} credential (hidden): ");
            io::stderr().flush()?;
            let key = rpassword::read_password()?;
            ensure!(
                !key.trim().is_empty(),
                "Credential cannot be blank. No settings changed."
            );
            *stored = Some(key.trim().into());
        }
        _ => bail!("Choose keep, replace, or remove. No settings changed."),
    }
    Ok(())
}

fn prompt_key_if_missing(cfg: &mut LightMemConfig, provider_name: &str) -> Result<()> {
    if cfg.effective_ollama_api_key().is_none() {
        let (prov, _, key_env) = cfg.active_provider();
        if prov != "onnx" && prov != "top1" && prov != "demo" {
            let env_str = key_env
                .map(|e| format!(" (or export {e})"))
                .unwrap_or_default();
            eprintln!(
                "  {} Note: {} typically requires an API key{}.",
                CliView::gold("💡"),
                provider_name,
                env_str
            );
            if confirm("Configure API key now?", true)? {
                prompt_api_key(cfg, provider_name)?;
            }
        }
    }
    Ok(())
}

pub fn edit_configuration(previous: &LightMemConfig) -> Result<Option<LightMemConfig>> {
    require_terminal()?;
    let mut cfg = previous.clone();

    loop {
        eprintln!();
        eprintln!(
            "  ❖ {}",
            CliView::white_bold("L I G H T M E M  Interactive Configuration")
        );
        eprintln!(
            "  {}",
            CliView::slate("─────────────────────────────────────────────────────────────")
        );
        eprintln!(
            "  {} {:<16} {}",
            CliView::slate("◫"),
            CliView::white_bold("Config File:"),
            CliView::slate(&LightMemConfig::config_file().display().to_string())
        );
        eprintln!();
        eprintln!(
            "  ╭─ {}",
            CliView::crimson_bold("SELECT SETTING TO CONFIGURE")
        );
        eprintln!(
            "  │  {} {} {:<26} {}",
            CliView::slate("▸"),
            CliView::white_bold("1."),
            "Search & Embeddings",
            CliView::slate(&format!("[{}: {}]", cfg.backend, cfg.embedding_model))
        );
        let prov_display = CliView::model_provider_name(&cfg);
        eprintln!(
            "  │  {} {} {:<26} {}",
            CliView::slate("▸"),
            CliView::white_bold("2."),
            "Answer Model & Provider",
            CliView::slate(&format!("[{}: {}]", prov_display, cfg.reranker))
        );
        eprintln!(
            "  │  {} {} {:<26} {}",
            CliView::slate("▸"),
            CliView::white_bold("3."),
            "Entity Extraction",
            CliView::slate(&format!("[{}]", cfg.entity_extraction))
        );
        let remote_disp = cfg.remote_url.as_deref().unwrap_or("none (local vault)");
        eprintln!(
            "  │  {} {} {:<26} {}",
            CliView::slate("▸"),
            CliView::white_bold("4."),
            "Remote Vault Routing",
            CliView::slate(&format!("[{}]", remote_disp))
        );
        let model_key_stat = if cfg.effective_ollama_api_key().is_some() {
            "configured"
        } else {
            "none"
        };
        let remote_key_stat = if cfg.effective_api_key().is_some() {
            "configured"
        } else {
            "none"
        };
        eprintln!(
            "  │  {} {} {:<26} {}",
            CliView::slate("▸"),
            CliView::white_bold("5."),
            "Secrets & API Keys",
            CliView::slate(&format!(
                "[model: {}, remote: {}]",
                model_key_stat, remote_key_stat
            ))
        );
        eprintln!("  │");
        eprintln!(
            "  ╰─ {} {} {:<26}",
            CliView::slate("▸"),
            CliView::slate("0."),
            CliView::slate("Cancel / Done")
        );
        eprintln!();

        match line("Choose [1-5]: ")?.as_str() {
            "0" => return Ok(None),
            "1" => {
                eprintln!();
                eprintln!(
                    "  ╭─ {}",
                    CliView::crimson_bold("SEARCH & VECTOR EMBEDDINGS")
                );
                eprintln!(
                    "  │  Embeddings power semantic search. Changing models may reindex the vault."
                );
                eprintln!("  │");
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 1."),
                    "Local ONNX: bge-small",
                    CliView::slate("[recommended, 384-dim, 100% offline]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 2."),
                    "Local ONNX: minilm",
                    CliView::slate("[lightweight, 384-dim, fast]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 3."),
                    "Local ONNX: nomic",
                    CliView::slate("[high capacity, 768-dim]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 4."),
                    "Ollama Embeddings",
                    CliView::slate("[local ollama endpoint, nomic-embed-text]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 5."),
                    "Custom ONNX Model Directory",
                    CliView::slate("[path to directory with model.onnx]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 6."),
                    "Deterministic Hash",
                    CliView::slate("[demo mode, no neural model]")
                );
                eprintln!("  │");
                eprintln!(
                    "  ╰─ {} {} {:<30}",
                    CliView::slate("▸"),
                    CliView::slate(" 0."),
                    CliView::slate("Back to main menu")
                );
                eprintln!();

                let choice = value("Search embedder (select 1-6)", "1")?;
                match choice.trim() {
                    "0" => continue,
                    "1" | "bge-small" => {
                        cfg.backend = "onnx".to_string();
                        cfg.onnx_model = Some("bge-small".to_string());
                        cfg.embedding_model = "bge-small".to_string();
                        break;
                    }
                    "2" | "minilm" => {
                        cfg.backend = "onnx".to_string();
                        cfg.onnx_model = Some("minilm".to_string());
                        cfg.embedding_model = "minilm".to_string();
                        break;
                    }
                    "3" | "nomic" => {
                        cfg.backend = "onnx".to_string();
                        cfg.onnx_model = Some("nomic".to_string());
                        cfg.embedding_model = "nomic".to_string();
                        break;
                    }
                    "4" | "ollama" => {
                        cfg.backend = "ollama".to_string();
                        let ep = value(
                            "Ollama embedding endpoint URL (e.g. http://localhost:11434)",
                            cfg.embedding_endpoint
                                .as_deref()
                                .unwrap_or("http://localhost:11434"),
                        )?;
                        cfg.embedding_endpoint = Some(LightMemConfig::normalize_ollama_url(&ep));
                        cfg.embedding_model = value(
                            "Ollama embedding model",
                            if cfg.embedding_model.is_empty() || cfg.embedding_model == "bge-small"
                            {
                                "nomic-embed-text"
                            } else {
                                &cfg.embedding_model
                            },
                        )?;
                        break;
                    }
                    "5" | "custom" => {
                        cfg.backend = "onnx".to_string();
                        let dir = value(
                            "Path to custom model directory",
                            cfg.onnx_model.as_deref().unwrap_or("~/.lightmem/models"),
                        )?;
                        cfg.onnx_model = Some(dir);
                        break;
                    }
                    "6" | "hash" => {
                        cfg.backend = "hash".to_string();
                        cfg.embedding_model = "hash".to_string();
                        break;
                    }
                    "onnx" => {
                        cfg.backend = "onnx".to_string();
                        cfg.onnx_model = Some("bge-small".to_string());
                        cfg.embedding_model = "bge-small".to_string();
                        break;
                    }
                    _ => bail!("Choose 1-6, onnx, ollama, or hash. No settings changed."),
                }
            }
            "2" => {
                eprintln!();
                eprintln!(
                    "  ╭─ {}",
                    CliView::crimson_bold("SELECT ANSWER MODEL & PROVIDER")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 1."),
                    "Local ONNX MiniLM",
                    CliView::slate("[100% offline, zero setup]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 2."),
                    "Local Ollama",
                    CliView::slate("[localhost:11434, auto-detect]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 3."),
                    "Groq Ultra-Fast",
                    CliView::slate("[qwen/qwen3.8-27b, llama-3.3-70b]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 4."),
                    "OpenAI Official",
                    CliView::slate("[gpt-4o-mini, gpt-4o]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 5."),
                    "Google Gemini",
                    CliView::slate("[gemini-1.5-flash]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 6."),
                    "DeepSeek Cloud",
                    CliView::slate("[deepseek-chat]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 7."),
                    "OpenRouter Multi-LLM",
                    CliView::slate("[claude-3.5-sonnet]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 8."),
                    "Mistral AI",
                    CliView::slate("[mistral-small-latest]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold(" 9."),
                    "Custom Endpoint",
                    CliView::slate("[custom OpenAI-compatible URL]")
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold("10."),
                    "Top-1 Search Hit",
                    CliView::slate("[no neural QA, return top memory]")
                );
                eprintln!("  │");
                eprintln!(
                    "  ╰─ {} {} {:<30}",
                    CliView::slate("▸"),
                    CliView::slate(" 0."),
                    CliView::slate("Back to main menu")
                );
                eprintln!();

                let ans = value("Answer engine (select 1-10)", "1")?;
                let trimmed = ans.trim();
                match trimmed {
                    "0" => continue,
                    "1" => {
                        cfg.apply_preset("local")?;
                        break;
                    }
                    "2" => {
                        cfg.apply_preset("ollama")?;
                        break;
                    }
                    "3" => {
                        cfg.apply_preset("groq")?;
                        prompt_key_if_missing(&mut cfg, "Groq")?;
                        break;
                    }
                    "4" => {
                        cfg.apply_preset("openai")?;
                        prompt_key_if_missing(&mut cfg, "OpenAI")?;
                        break;
                    }
                    "5" => {
                        cfg.apply_preset("gemini")?;
                        prompt_key_if_missing(&mut cfg, "Gemini")?;
                        break;
                    }
                    "6" => {
                        cfg.apply_preset("deepseek")?;
                        prompt_key_if_missing(&mut cfg, "DeepSeek")?;
                        break;
                    }
                    "7" => {
                        cfg.apply_preset("openrouter")?;
                        prompt_key_if_missing(&mut cfg, "OpenRouter")?;
                        break;
                    }
                    "8" => {
                        cfg.apply_preset("mistral")?;
                        prompt_key_if_missing(&mut cfg, "Mistral")?;
                        break;
                    }
                    "9" => {
                        let ep = value(
                            "Custom OpenAI-compatible endpoint URL",
                            "http://localhost:8000/v1",
                        )?;
                        http_url(&ep)?;
                        cfg.apply_preset("custom")?;
                        cfg.ollama_url = LightMemConfig::normalize_ollama_url(&ep);
                        prompt_key_if_missing(&mut cfg, "Custom Endpoint")?;
                        break;
                    }
                    "10" => {
                        cfg.apply_preset("demo")?;
                        break;
                    }
                    _ => {
                        let lower = trimmed.to_lowercase();
                        match lower.as_str() {
                            "local" | "offline" | "onnx" | "minilm-squad2" => {
                                cfg.apply_preset("local")?;
                                break;
                            }
                            "openai" | "gemini" | "groq" | "openrouter" | "ollama" | "deepseek"
                            | "mistral" | "demo" | "top1" => {
                                cfg.apply_preset(&lower)?;
                                let prov_name = CliView::model_provider_name(&cfg);
                                prompt_key_if_missing(&mut cfg, prov_name)?;
                                break;
                            }
                            "custom" => {
                                let ep = value(
                                    "Custom OpenAI-compatible endpoint URL",
                                    "http://localhost:8000/v1",
                                )?;
                                http_url(&ep)?;
                                cfg.apply_preset("custom")?;
                                cfg.ollama_url = LightMemConfig::normalize_ollama_url(&ep);
                                prompt_key_if_missing(&mut cfg, "Custom Endpoint")?;
                                break;
                            }
                            _ => {
                                if let Some((prov, specific_model)) = trimmed.split_once(':') {
                                    let prov_lower = prov.trim().to_lowercase();
                                    if matches!(
                                        prov_lower.as_str(),
                                        "openai"
                                            | "gemini"
                                            | "groq"
                                            | "openrouter"
                                            | "deepseek"
                                            | "mistral"
                                            | "ollama"
                                    ) {
                                        cfg.apply_preset(&prov_lower)?;
                                        cfg.reranker =
                                            format!("{}:{}", prov_lower, specific_model.trim());
                                        let prov_name = CliView::model_provider_name(&cfg);
                                        prompt_key_if_missing(&mut cfg, prov_name)?;
                                        break;
                                    } else {
                                        cfg.reranker = super::parse_reranker_setting(trimmed)?;
                                        break;
                                    }
                                } else {
                                    cfg.reranker = super::parse_reranker_setting(trimmed)?;
                                    if cfg.reranker.starts_with("ollama")
                                        && !cfg.reranker.contains(':')
                                    {
                                        cfg.ollama_url =
                                            LightMemConfig::normalize_ollama_url(&value(
                                                "Ollama service URL",
                                                &cfg.ollama_url,
                                            )?);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            "3" => {
                eprintln!();
                eprintln!("  ╭─ {}", CliView::crimson_bold("ENTITY EXTRACTION"));
                eprintln!(
                    "  │  Extracts structured entities & relations from newly saved memory text."
                );
                eprintln!(
                    "  │  Generative Model Target: {}",
                    CliView::emerald(&cfg.effective_ollama_url())
                );
                eprintln!("  │");
                eprintln!(
                    "  │  {} {} {:<26} {}",
                    CliView::slate("▸"),
                    CliView::white_bold("1."),
                    "Enabled (on)",
                    CliView::slate("[extracts entities using active answer model]")
                );
                eprintln!(
                    "  │  {} {} {:<26} {}",
                    CliView::slate("▸"),
                    CliView::white_bold("2."),
                    "Disabled (off)",
                    CliView::slate("[store memories as-is, faster ingestion]")
                );
                eprintln!("  │");
                eprintln!(
                    "  ╰─ {} {} {:<26}",
                    CliView::slate("▸"),
                    CliView::slate("0."),
                    CliView::slate("Back to main menu")
                );
                eprintln!();
                let default_ee = if cfg.entity_extraction.eq_ignore_ascii_case("off") {
                    "2"
                } else {
                    "1"
                };
                let mode = value("Entity extraction (select 1-2)", default_ee)?;
                let trimmed = mode.trim();
                match trimmed {
                    "0" => continue,
                    "1" => {
                        cfg.entity_extraction = "on".to_string();
                        break;
                    }
                    "2" => {
                        cfg.entity_extraction = "off".to_string();
                        break;
                    }
                    other => {
                        let lower = other.to_lowercase();
                        if lower == "off" || lower == "false" {
                            cfg.entity_extraction = "off".to_string();
                        } else if lower == "on" || lower == "true" {
                            cfg.entity_extraction = "on".to_string();
                        } else {
                            cfg.entity_extraction = other.to_string();
                        }
                        break;
                    }
                }
            }
            "4" => {
                eprintln!();
                eprintln!(
                    "  ╭─ {}",
                    CliView::crimson_bold("REMOTE LIGHTMEM SERVER ROUTING")
                );
                eprintln!(
                    "  │  Connect this workstation to a remote LightMem daemon vault server."
                );
                eprintln!(
                    "  │  Current routing: {}",
                    CliView::emerald(cfg.remote_url.as_deref().unwrap_or("none (local vault)"))
                );
                eprintln!("  │");
                eprintln!(
                    "  │  {} {} {:<26} {}",
                    CliView::slate("▸"),
                    CliView::white_bold("1."),
                    "Local Vault",
                    CliView::slate("[disconnect remote, use local SQLite database]")
                );
                eprintln!(
                    "  │  {} {} {:<26} {}",
                    CliView::slate("▸"),
                    CliView::white_bold("2."),
                    "Remote Server URL",
                    CliView::slate("[enter http://<server>:<port>]")
                );
                eprintln!("  │");
                eprintln!(
                    "  ╰─ {} {} {:<26}",
                    CliView::slate("▸"),
                    CliView::slate("0."),
                    CliView::slate("Back to main menu")
                );
                eprintln!();
                let default_rem = if cfg.remote_url.is_none() { "1" } else { "2" };
                let url = value("Remote routing (select 1-2)", default_rem)?;
                let trimmed = url.trim();
                match trimmed {
                    "0" => continue,
                    "1" | "local" | "none" => {
                        cfg.remote_url = None;
                        cfg.api_key = None;
                        break;
                    }
                    "2" => {
                        let target = value("Enter remote server URL", "http://localhost:8080")?;
                        http_url(&target)?;
                        cfg.remote_url = Some(target.trim_end_matches('/').into());
                        break;
                    }
                    other => {
                        http_url(other)?;
                        cfg.remote_url = Some(other.trim_end_matches('/').into());
                        break;
                    }
                }
            }
            "5" => {
                eprintln!();
                eprintln!("  ╭─ {}", CliView::crimson_bold("SECRETS & API KEYS"));
                let m_stat = if cfg.effective_ollama_api_key().is_some() {
                    CliView::gold("configured (secret)")
                } else {
                    CliView::slate("none (empty)")
                };
                let r_stat = if cfg.effective_api_key().is_some() {
                    CliView::gold("configured (secret)")
                } else {
                    CliView::slate("none (empty)")
                };
                let e_stat = if cfg.effective_embedding_api_key().is_some() {
                    CliView::gold("configured (secret)")
                } else {
                    CliView::slate("none (empty)")
                };
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold("1."),
                    "Model Service API Key",
                    m_stat
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold("2."),
                    "Remote LightMem Server Key",
                    r_stat
                );
                eprintln!(
                    "  │  {} {} {:<30} {}",
                    CliView::slate("▸"),
                    CliView::white_bold("3."),
                    "Embedding Endpoint Key",
                    e_stat
                );
                eprintln!("  │");
                eprintln!(
                    "  ╰─ {} {} {:<30}",
                    CliView::slate("▸"),
                    CliView::slate("0."),
                    CliView::slate("Back to main menu")
                );
                eprintln!();
                match line(
                    "Credential: 1. Model service, 2. Remote LightMem, 3. Embedding endpoint: ",
                )?
                .as_str()
                {
                    "0" => continue,
                    "1" => {
                        let present = cfg.effective_ollama_api_key().is_some();
                        credential(&mut cfg.ollama_api_key, "model service", present)?;
                        break;
                    }
                    "2" => {
                        let present = cfg.effective_api_key().is_some();
                        credential(&mut cfg.api_key, "remote LightMem", present)?;
                        break;
                    }
                    "3" => {
                        let present = cfg.effective_embedding_api_key().is_some();
                        credential(&mut cfg.embedding_api_key, "embedding endpoint", present)?;
                        break;
                    }
                    _ => bail!("Invalid credential choice. No settings changed."),
                }
            }
            _ => bail!("Invalid choice. No settings changed."),
        }
    }
    if serde_json::to_value(previous)? == serde_json::to_value(&cfg)? {
        eprintln!("No settings changed.");
        return Ok(None);
    }
    Ok(Some(cfg))
}

fn env_present(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

pub fn render_config(
    cfg: &LightMemConfig,
    updated: bool,
    db: Option<&Path>,
    global: bool,
) -> Result<()> {
    CliView::render_config(cfg, updated, false, db, global)
}

pub fn completion_hint() {
    eprintln!("Tab completion: run 'lmem completions <shell>'. For this session:\n  bash: source <(lmem completions bash)\n  zsh:  source <(lmem completions zsh)\n  fish: lmem completions fish | source");
}

pub fn offer_downloads(cfg: &LightMemConfig) -> Result<()> {
    if cfg.backend != "onnx" || cfg.effective_remote_url().is_some() {
        return Ok(());
    }
    if LightMemConfig::offline_requested() {
        eprintln!("Offline mode: model downloads skipped. Prepare assets before enabling LIGHTMEM_OFFLINE.");
        return Ok(());
    }
    if !confirm(
        "Download/verify the selected local search and answer models now?",
        false,
    )? {
        eprintln!(
            "Prepare later: lmem config --download all (requires network). Check with lmem doctor."
        );
        return Ok(());
    }
    let model = cfg.onnx_model.as_deref().unwrap_or("bge-small");
    if Path::new(model).is_dir() {
        let _ = lightmem::OnnxEmbeddingProvider::new_custom_dir(Path::new(model))?;
    } else {
        let _ = lightmem::OnnxEmbeddingProvider::new(Some(model))?;
    }
    if matches!(
        cfg.reranker.as_str(),
        "minilm-squad2" | "tinyroberta-squad2"
    ) {
        lightmem::OnnxQaReranker::ensure_model_downloaded(Some(&cfg.reranker))?;
    }
    eprintln!("Selected local assets prepared. Check search with lmem config --test; test answers with a known memory.");
    Ok(())
}

fn read_only(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_millis(250))?;
    Ok(conn)
}

pub fn connection_ids() -> Result<Vec<String>> {
    let path = LightMemConfig::config_dir().join("identities.db");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let conn = read_only(&path)?;
    let mut statement = conn.prepare("SELECT id FROM connections ORDER BY id")?;
    let values = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(values
        .into_iter()
        .filter(|id| lightmem::models::identity::validate_id(id).is_ok())
        .collect())
}

fn readonly_connection(id: &str, scope: Option<MemoryScope>) -> Result<PathBuf> {
    let conn = read_only(&LightMemConfig::config_dir().join("identities.db"))
        .context("Cannot read connection registry; use lmem identity list")?;
    let raw: String = conn
        .query_row("SELECT context FROM connections WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .context("Unknown connection")?;
    let mut context: MemoryContext = serde_json::from_str(&raw)?;
    if let Some(scope) = scope {
        context.scope = scope;
    }
    let hash = |value: &str| lightmem::compute_sha256_hex(value.as_bytes());
    let root = LightMemConfig::config_dir()
        .join("vaults")
        .join(hash(&context.user_id));
    Ok(match context.scope {
        MemoryScope::User => root.join("user.db"),
        MemoryScope::Project => root.join("projects").join(format!(
            "{}.db",
            hash(
                context
                    .project_id
                    .as_deref()
                    .context("Project scope requires a project ID")?
            )
        )),
        MemoryScope::Agent => {
            let root = root.join("agents").join(hash(&context.agent_id));
            if let Some(project) = context.project_id {
                root.join("projects").join(format!("{}.db", hash(&project)))
            } else {
                root.join("agent.db")
            }
        }
    })
}

#[allow(clippy::too_many_arguments)]
pub fn doctor(
    db: &Option<PathBuf>,
    global: bool,
    connection: Option<&str>,
    scope: Option<MemoryScope>,
    session: Option<&str>,
    remote: Option<&str>,
    machine: bool,
) -> Result<()> {
    let cfg = LightMemConfig::try_load();
    let mut checks = Vec::new();
    let mut add = |name: &str, status: &str, message: String| {
        checks.push(json!({"check":name,"status":status,"message":message}));
    };
    let env_db = env_present("LIGHTMEM_DB").map(PathBuf::from);
    let connection = connection
        .map(String::from)
        .or_else(|| env_present("LIGHTMEM_CONNECTION"));
    ensure!(
        connection.is_some() || (scope.is_none() && session.is_none()),
        "--scope and --session require --connection"
    );
    ensure!(
        connection.is_none() || (db.is_none() && env_db.is_none() && !global),
        "A connection cannot be combined with --db, LIGHTMEM_DB, or --global"
    );
    ensure!(
        remote.is_none() || (db.is_none() && env_db.is_none() && !global),
        "--remote cannot be combined with a local database selector"
    );
    let effective_remote = remote.map(String::from).or_else(|| {
        if db.is_some() || env_db.is_some() || global {
            None
        } else {
            cfg.as_ref().ok().and_then(|c| c.effective_remote_url())
        }
    });
    ensure!(
        connection.is_none() || effective_remote.is_none(),
        "Remote mode does not support local connections or scopes"
    );
    let is_remote = effective_remote.is_some();
    let selected = if let Some(url) = effective_remote {
        http_url(&url)?;
        url
    } else if let Some(id) = &connection {
        readonly_connection(id, scope)?
            .to_string_lossy()
            .into_owned()
    } else {
        db.clone()
            .or(env_db.clone())
            .unwrap_or_else(|| LightMemConfig::resolve_db_path(global))
            .to_string_lossy()
            .into_owned()
    };
    let mode = if is_remote { "remote" } else { "local" };
    let selected_source = if remote.is_some() {
        "--remote"
    } else if connection.is_some() {
        "connection"
    } else if db.is_some() {
        "--db"
    } else if env_db.is_some() {
        "LIGHTMEM_DB"
    } else if global {
        "--global"
    } else if mode == "remote" {
        "remote environment/configuration"
    } else {
        "project discovery/global fallback"
    };
    match cfg {
        Err(error) => add("configuration", "error", format!("{error}")),
        Ok(cfg) => {
            match validate(&cfg) {
                Ok(()) => add(
                    "configuration",
                    "ok",
                    format!(
                        "{} · entity extraction {}",
                        cfg.backend, cfg.entity_extraction
                    ),
                ),
                Err(e) => add("configuration", "error", e.to_string()),
            }
            if mode == "remote" {
                add(
                    "remote",
                    "not_tested",
                    "No network request made. Use lmem context to check remote access.".into(),
                );
            } else if !Path::new(&selected).exists() {
                add(
                    "vault",
                    "not_created",
                    "Created when needed; no vault was opened or written.".into(),
                );
            } else {
                match inspect_index(Path::new(&selected), &cfg) {
                    Ok((count, identity)) => {
                        add(
                            "vault",
                            "ok",
                            format!("{count} stored memories; opened read-only"),
                        );
                        if count == 0
                            || identity.as_deref() == Some(cfg.embedding_identity()?.as_str())
                        {
                            add(
                                "index",
                                "ok",
                                "Matches selected search model (or empty vault).".into(),
                            );
                        } else {
                            add(
                                "index",
                                "needs_reindex",
                                "Stored vectors differ. Run lmem reindex for this selected vault."
                                    .into(),
                            );
                        }
                    }
                    Err(e) => add("vault", "error", e.to_string()),
                }
            }
            add(
                "model_cache",
                "info",
                format!(
                    "{} ({})",
                    LightMemConfig::config_dir().join("models").display(),
                    if LightMemConfig::config_dir().join("models").is_dir() {
                        "directory exists; assets not verified"
                    } else {
                        "not present"
                    }
                ),
            );
            add(
                "embeddings",
                "not_tested",
                "No models loaded or downloaded. Explicit test: lmem config --test.".into(),
            );
            add(
                "answers",
                "not_tested",
                "Test answer quality separately with a known memory using lmem answer.".into(),
            );
        }
    }
    add(
        "agent",
        "not_tested",
        "Application detection or an installed binding does not verify a save/recall cycle.".into(),
    );
    let ok = !checks
        .iter()
        .any(|c| matches!(c["status"].as_str(), Some("error" | "needs_reindex")));
    let report = json!({"ok":ok,"mode":mode,"target":selected,"target_source":selected_source,"config_file":LightMemConfig::config_file(),"read_only":true,"network_tested":false,"checks":checks});
    if machine {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!();
        println!(
            "  {} {}",
            CliView::crimson_bold("❖"),
            CliView::white_bold("L I G H T M E M  Doctor Diagnostics")
        );
        println!(
            "  {}",
            CliView::crimson("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
        );
        println!(
            "  {} {:<18} {} ({})",
            CliView::slate("◫"),
            CliView::white_bold("Target Vault:"),
            report["target"].as_str().unwrap(),
            report["mode"].as_str().unwrap()
        );
        println!(
            "  {} {:<18} {}",
            CliView::slate("◈"),
            CliView::white_bold("Discovery:"),
            selected_source
        );
        println!(
            "  {} {:<18} {}",
            CliView::slate("✦"),
            CliView::white_bold("Config File:"),
            LightMemConfig::config_file().display()
        );
        println!();

        println!("  ╭─ SYSTEM HEALTH & READINESS");
        let last_idx = checks.len().saturating_sub(1);
        for (i, check) in checks.iter().enumerate() {
            let status = check["status"].as_str().unwrap_or("unknown");
            let name = check["check"].as_str().unwrap_or("");
            let message = check["message"].as_str().unwrap_or("");
            let (glyph, styled_name) = match status {
                "ok" => (CliView::emerald_bold("✓"), CliView::white_bold(name)),
                "info" => (CliView::violet_bold("ℹ"), CliView::white_bold(name)),
                "error" => (CliView::crimson_bold("✗"), CliView::crimson_bold(name)),
                "needs_reindex" => (CliView::gold_bold("⚠️"), CliView::gold_bold(name)),
                _ => (CliView::slate("○"), CliView::slate(name)),
            };
            let prefix = if i == last_idx { "  ╰─" } else { "  │ " };
            println!("{} {} {:<16} {}", prefix, glyph, styled_name, message);
        }
        println!();
        if ok {
            println!(
                "  {} {}",
                CliView::emerald_bold("✓"),
                CliView::emerald_bold("All core system checks passed and vault is healthy.")
            );
        } else {
            println!(
                "  {} {}",
                "⚠️".yellow().bold(),
                "One or more diagnostic checks require attention."
                    .yellow()
                    .bold()
            );
        }
        println!();
    }
    if !ok {
        std::process::exit(1);
    }
    Ok(())
}

fn inspect_index(path: &Path, cfg: &LightMemConfig) -> Result<(i64, Option<String>)> {
    let conn = read_only(path)?;
    let count: i64 = conn.query_row("SELECT count(*) FROM memories", [], |r| r.get(0))?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='embedding_state' AND type='table')",
        [],
        |r| r.get(0),
    )?;
    let identity = if exists {
        conn.query_row(
            "SELECT identity FROM embedding_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .optional()?
    } else {
        None
    };
    cfg.embedding_identity()?;
    Ok((count, identity))
}

pub fn complete_connections(shell: clap_complete::Shell, script: String) -> String {
    // Add shell-specific dynamic connection lookup to the generated argument completion.
    // Names come from a read-only query and the hidden command never opens a vault.
    match shell {
        clap_complete::Shell::Bash => {
            let script = script.replacen("_lmem()", "_lmem_base()", 1);
            format!(
                "{script}\n{}",
                r#"_lmem() {
    local previous="${COMP_WORDS[COMP_CWORD-1]}" current="${COMP_WORDS[COMP_CWORD]}"
    if [[ "$previous" == --connection ]]; then
        COMPREPLY=()
        local id
        while IFS= read -r id; do
            [[ "$id" == "$current"* ]] && COMPREPLY+=("$id")
        done < <(lmem __connections 2>/dev/null)
        return 0
    else
        _lmem_base "$@"
    fi
}
"#
            )
        }
        clap_complete::Shell::Zsh => {
            let script = script.replace(":CONNECTION:_default", ":CONNECTION:_lmem_connections");
            format!(
                "{script}\n{}",
                r#"_lmem_connections() {
    local -a connections
    connections=("${(@f)$(lmem __connections 2>/dev/null)}")
    _describe 'connection' connections
}
"#
            )
        }
        clap_complete::Shell::Fish => format!(
            "{script}\ncomplete -c lmem -l connection -r -a '(lmem __connections 2>/dev/null)'\n"
        ),
        _ => script,
    }
}

/// Smooth, branded terminal progress bar using LightMem's Unicode block characters (`▰` / `▱`).
pub struct TerminalProgress {
    prefix: String,
    total: usize,
    width: usize,
    is_tty: bool,
    start_time: std::time::Instant,
}

impl TerminalProgress {
    pub fn new(prefix: impl Into<String>, total: usize) -> Self {
        let is_tty = io::stderr().is_terminal();
        Self {
            prefix: prefix.into(),
            total,
            width: 20,
            is_tty,
            start_time: std::time::Instant::now(),
        }
    }

    pub fn set_total(&mut self, total: usize) {
        self.total = total;
    }

    pub fn update(&self, current: usize, message: &str) {
        if !self.is_tty {
            return;
        }
        let pct = (current * 100).checked_div(self.total).unwrap_or(100);
        let filled = (current * self.width)
            .checked_div(self.total)
            .unwrap_or(self.width);
        let unfilled = self.width.saturating_sub(filled);
        let bar_filled: String = "▰".repeat(filled);
        let bar_unfilled: String = "▱".repeat(unfilled);

        let max_msg_len = 32;
        let display_msg = if message.chars().count() > max_msg_len {
            let mut s = message
                .chars()
                .take(max_msg_len.saturating_sub(3))
                .collect::<String>();
            s.push_str("...");
            s
        } else {
            message.to_string()
        };

        eprint!(
            "\r  \x1b[38;2;113;113;122m▸\x1b[0m {:<14} \x1b[38;2;220;38;38m{}\x1b[38;2;63;63;70m{}\x1b[0m {:>3}% ({}/{})  \x1b[38;2;161;161;170m{}\x1b[0m\x1b[K",
            self.prefix,
            bar_filled,
            bar_unfilled,
            pct,
            current,
            self.total,
            display_msg
        );
        let _ = io::stderr().flush();
    }

    pub fn finish(&self, message: &str) {
        if self.is_tty {
            let bar_filled: String = "▰".repeat(self.width);
            let elapsed = self.start_time.elapsed();
            let elapsed_str = if elapsed.as_secs() > 0 {
                format!("{:.1}s", elapsed.as_secs_f32())
            } else {
                format!("{}ms", elapsed.as_millis())
            };
            eprintln!(
                "\r  \x1b[38;2;113;113;122m▸\x1b[0m {:<14} \x1b[38;2;34;197;94m{}\x1b[0m 100% ({}/{})  \x1b[38;2;34;197;94m{} ({})\x1b[0m\x1b[K",
                self.prefix,
                bar_filled,
                self.total,
                self.total,
                message,
                elapsed_str
            );
        }
    }
}
