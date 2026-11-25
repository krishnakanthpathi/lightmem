use anyhow::{Context, Result};
use colored::*;
use lightmem::{CliView, LightMem, LightMemConfig, ResolvedConnection};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use super::context::{open_controller, remove_db_files};
use crate::terminal;

pub fn parse_reranker_setting(raw: &str) -> Result<String> {
    let lower = raw.trim().to_lowercase();
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
        "Invalid answer engine '{}'. Use minilm-squad2, tinyroberta-squad2, top1, or a model spec like ollama:<model>, openai:<model>, openrouter:<model>, gemini:<model>, groq:<model>, custom:<model>.",
        raw
    );
    let normalized = LightMemConfig::normalize_reranker(raw, None);
    anyhow::ensure!(
        !normalized.ends_with(':'),
        "Answer engine needs a model after ':'"
    );
    Ok(normalized)
}

/// Read the selected vault without creating it or applying schema migrations.
pub fn configuration_plan(
    previous: &LightMemConfig,
    proposed: &LightMemConfig,
    db_path: &Path,
) -> Result<serde_json::Value> {
    let identity = proposed.embedding_identity()?;
    let (count, stored) = lightmem::Storage::inspect_vault_state(db_path)?;
    let before = serde_json::to_value(previous)?;
    let after = serde_json::to_value(proposed)?;
    let keys: std::collections::BTreeSet<_> = before
        .as_object()
        .context("Invalid configuration")?
        .keys()
        .chain(after.as_object().context("Invalid configuration")?.keys())
        .collect();
    let mut changes: Vec<_> = keys
        .into_iter()
        .filter(|key| before.get(*key) != after.get(*key))
        .cloned()
        .collect();
    if changes.iter().any(|k| k == "openai_api_key")
        && !changes.iter().any(|k| k == "ollama_api_key")
    {
        changes.push("ollama_api_key".to_string());
    }
    if changes.iter().any(|k| k == "openai_endpoint") && !changes.iter().any(|k| k == "ollama_url")
    {
        changes.push("ollama_url".to_string());
    }
    if changes.iter().any(|k| k == "openai_model") && !changes.iter().any(|k| k == "reranker") {
        changes.push("reranker".to_string());
    }
    if changes.iter().any(|k| k == "reranker") && !changes.iter().any(|k| k == "openai_model") {
        changes.push("openai_model".to_string());
    }
    changes.sort();
    Ok(serde_json::json!({
        "dry_run": true,
        "config_scope": "shared across vaults in this configuration root",
        "config_file": LightMemConfig::config_file(),
        "selected_db": db_path,
        "vault_exists": db_path.exists(),
        "total_memories": count,
        "changed_fields": changes,
        "stored_embedding_identity": stored,
        "requested_embedding_identity": identity,
        "migration_required": count > 0 && stored.as_deref() != Some(identity.as_str()),
        "embedding_change": previous.embedding_identity()? != identity,
        "other_vaults": "Not reindexed by this command; check each before using changed embeddings",
        "configuration": proposed.redacted_json()?,
    }))
}

pub fn apply_configuration(
    previous: &LightMemConfig,
    proposed: &LightMemConfig,
    db_path: &Path,
    yes: bool,
    json: bool,
) -> Result<bool> {
    let identity = proposed.embedding_identity()?;
    if previous.embedding_identity()? != identity && db_path.exists() {
        let lm = LightMem::open_at(db_path, proposed.clone())?;
        if !migrate_embeddings(&lm, yes, json, false)? {
            return Ok(false);
        }
    }
    proposed.save()?;
    Ok(true)
}

pub fn migrate_embeddings(
    lm: &LightMem,
    yes: bool,
    machine_output: bool,
    force: bool,
) -> Result<bool> {
    if !force && !lm.embedding_migration_needed()? {
        return Ok(true);
    }
    let count = lm.count(None, None, None)?;
    eprintln!(
        "Embedding migration for {} ({} memories)",
        lm.db_path().display(),
        count
    );
    eprintln!(
        "  From: {}",
        lm.stored_embedding_identity()?
            .as_deref()
            .unwrap_or("legacy / unknown")
    );
    eprintln!("  To:   {}", lm.requested_embedding_identity());
    if !yes {
        anyhow::ensure!(!machine_output && io::stdin().is_terminal() && io::stderr().is_terminal(),
            "Migration needs approval. For a model change, repeat the same config command with --yes. Otherwise run lmem --db '{}' reindex --yes. No embeddings were changed.", lm.db_path().display());
        eprint!("Rebuild embeddings now? The old index stays intact until completion. [y/N] ");
        io::stderr().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if !matches!(response.trim().to_lowercase().as_str(), "y" | "yes") {
            eprintln!("Migration cancelled. Configuration and embeddings were not changed.");
            return Ok(false);
        }
    }
    let count = lm.reindex(|done, total| {
        eprintln!(
            "Embedding progress: {}/{} ({}%)",
            done,
            total,
            done.saturating_mul(100).checked_div(total).unwrap_or(100)
        );
    })?;
    eprintln!("Migration complete: {} embeddings committed.", count);
    Ok(true)
}

pub fn handle_doctor(
    db: &Option<PathBuf>,
    global: bool,
    connection: Option<&str>,
    scope: Option<lightmem::MemoryScope>,
    session: Option<&str>,
    remote: Option<&str>,
    json: bool,
) -> Result<()> {
    terminal::doctor(db, global, connection, scope, session, remote, json)
}

pub fn handle_local(global: bool, json: bool) -> Result<()> {
    let mut cfg = LightMemConfig::load();
    let was_remote = cfg.remote_url.is_some() || cfg.api_key.is_some();
    cfg.remote_url = None;
    cfg.api_key = None;
    cfg.save()?;

    let default_vault = LightMemConfig::resolve_db_path(global);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "mode": "local",
                "vault": default_vault.display().to_string(),
                "previous_remote_cleared": was_remote,
            })
        );
    } else {
        println!(
            "{} {}",
            "✓".green().bold(),
            "Switched to local mode.".white().bold()
        );
        println!(
            "  {} {:<10} {}",
            "◫".dimmed(),
            "Vault".dimmed(),
            CliView::format_path(&default_vault).bold()
        );
        if std::env::var("LIGHTMEM_REMOTE_URL").is_ok() || std::env::var("LIGHTMEM_URL").is_ok() {
            println!(
                "  {} Note: Environment variable LIGHTMEM_REMOTE_URL/LIGHTMEM_URL is active.",
                "⚠️".yellow()
            );
            println!(
                "    Unset the environment variable or use --db to target local vaults explicitly."
            );
        }
    }
    Ok(())
}

pub fn handle_remote(url: Option<String>, api_key: Option<String>, json: bool) -> Result<()> {
    let mut cfg = LightMemConfig::load();
    let (target_url, new_api_key) = match url {
        Some(u) => {
            let clean = terminal::normalize_http_url(&u)?;
            (clean, api_key.clone().or_else(|| cfg.api_key.clone()))
        }
        None => {
            if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
                let (u, k) =
                    terminal::prompt_remote(cfg.remote_url.as_deref(), cfg.api_key.as_deref())?;
                (u, k)
            } else {
                anyhow::bail!("Specify a remote URL: lmem remote <URL> (or run interactively)");
            }
        }
    };

    cfg.remote_url = Some(target_url.clone());
    cfg.api_key = new_api_key;
    cfg.save()?;

    let client = lightmem::RemoteClient::new(&target_url, cfg.effective_api_key())
        .with_timeout(std::time::Duration::from_millis(1200));
    let probe_res = client.stats();
    let is_connected = probe_res.is_ok();

    if json {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "mode": "remote",
                "remote_url": target_url,
                "has_api_key": cfg.api_key.is_some(),
                "connected": is_connected,
            })
        );
    } else {
        println!(
            "{} {}",
            "✓".green().bold(),
            "Switched to remote mode.".white().bold()
        );
        println!(
            "  {} {:<10} {}",
            "◫".dimmed(),
            "Remote".dimmed(),
            target_url.bold()
        );
        if let Ok(stats) = probe_res {
            println!(
                "  {} {:<10} {}",
                "◈".dimmed(),
                "Status".dimmed(),
                format!("connected ({} active memories)", stats.active_memories).green()
            );
        } else if let Err(ref e) = probe_res {
            let err_str = e.to_string();
            if err_str.contains("401") || err_str.contains("Unauthorized") {
                println!(
                    "  {} {:<10} {}",
                    "◈".dimmed(),
                    "Status".dimmed(),
                    "authentication failed (401 Unauthorized)".red().bold()
                );
                println!(
                    "  {} Warning: The remote server requires a valid API key.",
                    "⚠️".yellow()
                );
                println!(
                    "    Update it with: lmem remote {} --api-key \"<YOUR_KEY>\"",
                    target_url
                );
            } else {
                println!(
                    "  {} {:<10} {}",
                    "◈".dimmed(),
                    "Status".dimmed(),
                    "configured (server not responding yet)".yellow()
                );
                println!(
                    "  {} Note: Start server with 'lmem serve --daemon', or switch back with 'lmem local'.",
                    "ℹ".dimmed()
                );
            }
        }
        if cfg.api_key.is_some() {
            println!(
                "  {} {:<10} API key configured",
                "✦".dimmed(),
                "Auth".dimmed()
            );
        }
    }
    Ok(())
}

pub fn handle_model(
    provider_or_model: Option<String>,
    api_key: Option<String>,
    endpoint: Option<String>,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
) -> Result<()> {
    let mut cfg = LightMemConfig::try_load()?;
    if provider_or_model.is_none() && api_key.is_none() && endpoint.is_none() {
        if json {
            CliView::render_config(&cfg, false, true, effective_db, global)?;
        } else {
            terminal::render_config(&cfg, false, effective_db, global)?;
        }
        return Ok(());
    }

    let path = effective_db
        .map(Path::to_path_buf)
        .unwrap_or_else(|| LightMemConfig::resolve_db_path(global));
    let previous = cfg.clone();

    if let Some(target) = provider_or_model {
        let lower = target.trim().to_lowercase();
        match lower.as_str() {
            "local" | "offline" | "onnx" => {
                cfg.apply_preset("local")?;
            }
            "openai" | "gemini" | "groq" | "openrouter" | "ollama" | "deepseek" | "mistral"
            | "demo" => {
                cfg.apply_preset(&lower)?;
            }
            "custom" => {
                anyhow::ensure!(
                    endpoint.is_some()
                        || (!cfg.ollama_url.trim().is_empty()
                            && cfg.ollama_url != "http://localhost:11434"),
                    "Custom provider requires an endpoint URL. Provide one with '--endpoint <URL>'"
                );
                cfg.apply_preset("custom")?;
            }
            _ => {
                if let Some((prov, specific_model)) = target.split_once(':') {
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
                        cfg.reranker = format!("{}:{}", prov_lower, specific_model.trim());
                    } else {
                        cfg.reranker = parse_reranker_setting(&target)?;
                    }
                } else {
                    cfg.reranker = parse_reranker_setting(&target)?;
                }
            }
        }
    }

    if let Some(ref ep) = endpoint {
        cfg.ollama_url = LightMemConfig::normalize_ollama_url(ep);
    }
    if let Some(ref key) = api_key {
        let trimmed = key.trim();
        cfg.ollama_api_key = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(trimmed.to_string())
        };
    }

    terminal::validate(&cfg)?;
    if !apply_configuration(&previous, &cfg, &path, true, json)? {
        return Ok(());
    }

    if json {
        CliView::render_config(&cfg, true, true, effective_db, global)?;
    } else {
        println!(
            "\n  {} {}",
            CliView::emerald_bold("✓"),
            CliView::emerald_bold("Answer model and provider updated")
        );
        terminal::render_config(&cfg, true, effective_db, global)?;
        let (prov, _, key_env) = cfg.active_provider();
        if prov != "onnx"
            && prov != "top1"
            && prov != "demo"
            && cfg.effective_ollama_api_key().is_none()
        {
            if let Some(env_name) = key_env {
                println!(
                    "  {} Tip: Configure an API key with 'lmem model --api-key <KEY>' or export {}.\n",
                    CliView::gold("💡"),
                    CliView::violet(env_name)
                );
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn handle_config(
    reload: bool,
    action: Option<String>,
    edit: bool,
    preset: Option<String>,
    backend: Option<String>,
    onnx_model: Option<String>,
    embedding_endpoint: Option<String>,
    embedding_api_key: Option<String>,
    download: Option<String>,
    url: Option<String>,
    model: Option<String>,
    reranker: Option<String>,
    entity_extraction: Option<String>,
    ollama_api_key: Option<String>,
    dry_run: bool,
    test: bool,
    reset_db: bool,
    yes: bool,
    remote: Option<String>,
    api_key: Option<String>,
    clear_remote: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let is_reload = reload
        || action
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case("reload") || s.eq_ignore_ascii_case("refresh"))
            .unwrap_or(false);
    if is_reload {
        let reloaded = LightMemConfig::try_load()?;
        if json {
            CliView::render_config(&reloaded, false, true, effective_db, global)?;
        } else {
            println!(
                "\n  {} Configuration reloaded from disk.",
                CliView::emerald_bold("✓")
            );
            terminal::render_config(&reloaded, false, effective_db, global)?;
        }
        return Ok(());
    }
    if let Some(act) = action.as_deref() {
        anyhow::bail!(
            "Unknown action '{}'. Did you mean 'lmem config reload' or 'lmem config --help'?",
            act
        );
    }
    let path = effective_db
        .map(Path::to_path_buf)
        .unwrap_or_else(|| LightMemConfig::resolve_db_path(global));
    let cfg_loaded = LightMemConfig::try_load()?;
    if edit {
        anyhow::ensure!(
            !json,
            "Use config flags with --json; --edit requires a terminal."
        );
        let Some(proposed) = terminal::edit_configuration(&cfg_loaded)? else {
            return Ok(());
        };
        terminal::validate(&proposed)?;
        let plan = configuration_plan(&cfg_loaded, &proposed, &path)?;
        CliView::render_configuration_plan(&proposed, &plan, false)?;
        if terminal::confirm(
            "Save these shared settings and approve selected-vault migration?",
            false,
        )? {
            terminal::ensure_unchanged(&cfg_loaded)?;
            if apply_configuration(&cfg_loaded, &proposed, &path, true, false)? {
                terminal::render_config(&proposed, true, effective_db, global)?;
            }
        } else {
            eprintln!("Cancelled. No settings changed.");
        }
        return Ok(());
    }
    if test {
        let scratch = tempfile::tempdir()?;
        let lm = LightMem::open_at(&scratch.path().join("diagnostic.db"), cfg_loaded.clone())?;
        let dimensions = lm.test_embedding_provider().context(
            "Embedding test failed. Check 'lmem config', model assets, endpoint, and credentials",
        )?;
        let result = serde_json::json!({
            "status": "ok",
            "backend": cfg_loaded.backend,
            "dimensions": dimensions,
            "memory_saved": false,
            "tests": "local saved embedding configuration; answer quality and remote vault health are separate"
        });
        if json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            println!();
            println!(
                "  {} {} {}",
                CliView::emerald_bold("✓"),
                CliView::white_bold("Embedding Engine Verified:"),
                CliView::violet_bold(&format!(
                    "{} dimensions ({})",
                    dimensions, cfg_loaded.backend
                ))
            );
            println!(
                "    {} Status: Online and operational (disposable test; no memory saved).",
                CliView::crimson("◈")
            );
            println!(
                "    {} Tip:    Test answer reranking with '{}'.",
                CliView::gold("💡"),
                CliView::violet("lmem answer \"<question>\"")
            );
            println!();
        }
        return Ok(());
    }
    if reset_db {
        let lm = open_controller(effective_db, global, connection)?;
        let db_path = lm.db_path().to_path_buf();
        let removed = lm.clear_all(true)?;
        drop(lm);
        remove_db_files(&db_path);
        CliView::render_clear(removed, true, &db_path, json)?;
        return Ok(());
    }

    if let Some(ref dl) = download {
        match dl.to_lowercase().as_str() {
            "all" => {
                for m in ["bge-small", "minilm", "nomic"] {
                    let _ = lightmem::OnnxEmbeddingProvider::new(Some(m))?;
                }
                let _ = lightmem::OnnxQaReranker::ensure_model_downloaded(Some("minilm-squad2"))?;
                let _ = lightmem::OnnxQaReranker::ensure_model_downloaded(Some("tinyroberta-squad2"))?;
            }
            "qa" | "minilm-squad2" | "deepset/minilm-uncased-squad2" => {
                let _ = lightmem::OnnxQaReranker::ensure_model_downloaded(Some("minilm-squad2"))?;
            }
            "tinyroberta" | "tinyroberta-squad2" | "deepset/tinyroberta-squad2" => {
                let _ = lightmem::OnnxQaReranker::ensure_model_downloaded(Some("tinyroberta-squad2"))?;
            }
            "bge-small" | "bge-small-en-v1.5" | "xenova/bge-small-en-v1.5" => {
                let _ = lightmem::OnnxEmbeddingProvider::new(Some("bge-small"))?;
            }
            "minilm" | "all-minilm-l6-v2" | "xenova/all-minilm-l6-v2" => {
                let _ = lightmem::OnnxEmbeddingProvider::new(Some("minilm"))?;
            }
            "nomic" | "nomic-embed-text" | "nomic-ai/nomic-embed-text-v1.5" => {
                let _ = lightmem::OnnxEmbeddingProvider::new(Some("nomic"))?;
            }
            other => anyhow::bail!(
                "Unknown model '{}' for --download. Choose 'bge-small', 'minilm', 'nomic', 'minilm-squad2', 'tinyroberta-squad2', or 'all'.",
                other
            ),
        }
    }

    let mut cfg = cfg_loaded;
    let previous = cfg.clone();
    let mut changed = false;

    if let Some(ref p) = preset {
        if p.eq_ignore_ascii_case("custom") {
            anyhow::ensure!(
                url.is_some()
                    || (!cfg.ollama_url.trim().is_empty()
                        && cfg.ollama_url != "http://localhost:11434"),
                "Custom provider requires an endpoint URL. Provide one with '--endpoint <URL>'"
            );
        }
        cfg.apply_preset(p)?;
        changed = true;
    }
    if let Some(ref eu) = embedding_endpoint {
        let trimmed = eu.trim();
        cfg.embedding_endpoint = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(LightMemConfig::normalize_ollama_url(trimmed))
        };
        changed = true;
    }
    if let Some(ref ek) = embedding_api_key {
        let trimmed = ek.trim();
        cfg.embedding_api_key = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(trimmed.to_string())
        };
        changed = true;
    }
    if let Some(mode) = entity_extraction {
        let trimmed = mode.trim();
        let normalized =
            if trimmed.eq_ignore_ascii_case("off") || trimmed.eq_ignore_ascii_case("false") {
                "off".to_string()
            } else {
                "on".to_string()
            };
        cfg.entity_extraction = normalized;
        changed = true;
    }
    if let Some(ref key) = ollama_api_key {
        let key = key.trim();
        cfg.ollama_api_key = if key.is_empty() || key.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(key.into())
        };
        changed = true;
    }

    if let Some(ref b) = backend {
        let target_url = url
            .as_ref()
            .map(|u| LightMemConfig::normalize_ollama_url(u))
            .unwrap_or_else(|| cfg.effective_ollama_url());
        if b == "ollama" && target_url == "https://ollama.com" {
            anyhow::bail!(
                "Ollama Cloud (https://ollama.com) does not provide an embedding endpoint; it only hosts generative LLMs.\n\
                 ◈ For vector search, keep '--backend onnx' (local ONNX, recommended) or connect to a local Ollama with '--url http://localhost:11434'.\n\
                 ◈ To use Ollama Cloud for question answering and reranking, run:\n\
                   lmem config --reranker ollama"
            );
        }
        cfg.backend = b.clone();
        changed = true;
    }
    if let Some(om) = onnx_model {
        cfg.onnx_model = Some(om);
        changed = true;
    }
    if let Some(ref u) = url {
        cfg.ollama_url = LightMemConfig::normalize_ollama_url(u);
        changed = true;
    }
    if backend.is_none() && cfg.backend == "ollama" && cfg.ollama_url == "https://ollama.com" {
        eprintln!(
            "◈ Notice: Ollama Cloud (https://ollama.com) provides generative models for answering/reranking.\n\
             ◈ Automatically keeping embedding backend on local ONNX ('bge-small') for fast offline search."
        );
        cfg.backend = "onnx".to_string();
        changed = true;
    }
    if let Some(m) = model {
        let lower = m.trim().to_lowercase();
        let is_local_switch = lower == "local" || lower == "offline" || lower == "onnx";
        let has_provider_prefix = lower.starts_with("openai:")
            || lower.starts_with("openrouter:")
            || lower.starts_with("gemini:")
            || lower.starts_with("groq:")
            || lower.starts_with("deepseek:")
            || lower.starts_with("mistral:")
            || lower.starts_with("ollama:")
            || lower.starts_with("llm:")
            || lower.starts_with("onnx:");

        if is_local_switch {
            cfg.apply_preset("local")?;
            changed = true;
        } else if has_provider_prefix
            || (reranker.is_none()
                && cfg.backend != "ollama"
                && backend.as_deref() != Some("ollama"))
        {
            cfg.reranker = parse_reranker_setting(&m)?;
            if cfg.entity_extraction != "off" && !cfg.entity_extraction.is_empty() {
                cfg.entity_extraction = "on".to_string();
            }
            changed = true;
        } else {
            cfg.embedding_model = m;
            changed = true;
        }
    }
    if let Some(r) = reranker {
        if let Some((prov, _)) = r.split_once(':') {
            let prov_lower = prov.trim().to_lowercase();
            if matches!(
                prov_lower.as_str(),
                "openai" | "gemini" | "groq" | "openrouter" | "deepseek" | "mistral"
            ) && url.is_none()
            {
                let _ = cfg.apply_preset(&prov_lower);
            }
        }
        cfg.reranker = parse_reranker_setting(&r)?;
        changed = true;
    }

    if let Some(ref r) = remote {
        let trimmed = r.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            cfg.remote_url = None;
        } else {
            cfg.remote_url = Some(trimmed.trim_end_matches('/').to_string());
        }
        changed = true;
    }
    if let Some(k) = api_key {
        let trimmed = k.trim();
        let is_clear = trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none");
        if ollama_api_key.is_some() || remote.is_some() || cfg.remote_url.is_some() {
            cfg.api_key = if is_clear {
                None
            } else {
                Some(trimmed.to_string())
            };
        } else {
            cfg.ollama_api_key = if is_clear {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        changed = true;
    }
    if clear_remote {
        cfg.remote_url = None;
        cfg.api_key = None;
        changed = true;
    }

    terminal::validate(&cfg)?;
    if dry_run {
        let plan = configuration_plan(&previous, &cfg, &path)?;
        return CliView::render_configuration_plan(&cfg, &plan, json);
    }
    if changed && !apply_configuration(&previous, &cfg, &path, yes, json)? {
        return Ok(());
    }
    if json {
        CliView::render_config(&cfg, changed, true, effective_db, global)?;
    } else {
        terminal::render_config(&cfg, changed, effective_db, global)?;
    }
    Ok(())
}

pub fn handle_setup(
    preset: Option<String>,
    url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    entity_extraction: Option<String>,
    dry_run: bool,
    yes: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
) -> Result<()> {
    let previous = LightMemConfig::try_load()?;
    let mut cfg = previous.clone();
    let interactive = io::stdin().is_terminal() && io::stderr().is_terminal() && !json;
    let explicit = preset.is_some()
        || url.is_some()
        || model.is_some()
        || api_key.is_some()
        || entity_extraction.is_some();
    if let Some(preset) = preset {
        cfg.apply_preset(&preset)?;
    } else if !explicit {
        anyhow::ensure!(
            interactive && !yes,
            "Choose settings explicitly: lmem setup --preset local --dry-run (or --preset demo for no downloads). Add --yes to save."
        );
        cfg = terminal::setup_configuration(&previous)?;
    }
    if let Some(url) = url {
        cfg.ollama_url = LightMemConfig::normalize_ollama_url(&url);
    }
    if let Some(model) = model {
        cfg.reranker = parse_reranker_setting(&model)?;
    }
    if let Some(key) = api_key {
        let trimmed = key.trim();
        cfg.ollama_api_key = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    if let Some(mode) = entity_extraction {
        let trimmed = mode.trim();
        let normalized =
            if trimmed.eq_ignore_ascii_case("off") || trimmed.eq_ignore_ascii_case("false") {
                "off".to_string()
            } else {
                "on".to_string()
            };
        cfg.entity_extraction = normalized;
    }
    let path = effective_db
        .map(Path::to_path_buf)
        .unwrap_or_else(|| LightMemConfig::resolve_db_path(global));
    terminal::validate(&cfg)?;
    let plan = configuration_plan(&previous, &cfg, &path)?;
    if dry_run {
        return CliView::render_configuration_plan(&cfg, &plan, json);
    }
    if !yes {
        anyhow::ensure!(
            interactive,
            "Setup needs confirmation. Preview with --dry-run, then repeat with --yes to save. No settings changed."
        );
        CliView::render_configuration_plan(&cfg, &plan, false)?;
        eprint!("Save these shared settings and approve selected-vault migration if embeddings change? [y/N] ");
        io::stderr().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if !matches!(response.trim().to_lowercase().as_str(), "y" | "yes") {
            eprintln!("Setup cancelled. Configuration and embeddings were not changed.");
            return Ok(());
        }
    }
    terminal::ensure_unchanged(&previous)?;
    if !apply_configuration(&previous, &cfg, &path, true, json)? {
        return Ok(());
    }
    if json {
        CliView::render_config(&cfg, true, true, effective_db, global)?;
    } else {
        terminal::render_config(&cfg, true, effective_db, global)?;
        if interactive {
            terminal::offer_downloads(&cfg)?;
        }
        terminal::completion_hint();
    }
    Ok(())
}

pub fn handle_reindex(
    yes: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let migrated = migrate_embeddings(&lm, yes, json, true)?;
    if json {
        let obj = serde_json::json!({
            "status": if migrated { "ok" } else { "cancelled" },
            "embedding_identity": lm.requested_embedding_identity(),
            "db": lm.db_path().display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
    }
    Ok(())
}
