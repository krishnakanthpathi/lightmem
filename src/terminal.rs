//! Short, line-oriented terminal interactions. Never open a controller for inspection.
use anyhow::{bail, ensure, Context, Result};
use lightmem::{CliView, LightMemConfig, MemoryContext, MemoryScope};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use colored::*;
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
    ensure!(
        matches!(cfg.entity_extraction.as_str(), "off" | "llm"),
        "Entity extraction must be off or llm."
    );
    http_url(&cfg.ollama_url)?;
    if let Some(url) = &cfg.remote_url {
        http_url(url)?;
    }
    ensure!(
        cfg.backend != "ollama" || cfg.effective_ollama_url() != "https://ollama.com",
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

pub fn setup_configuration(previous: &LightMemConfig) -> Result<LightMemConfig> {
    require_terminal()?;
    eprintln!(
        "LightMem setup · Settings shared under {}",
        LightMemConfig::config_dir().display()
    );
    let mut cfg = previous.clone();
    if confirm(
        "Use recommended local settings (BGE search, MiniLM answers, extraction off)?",
        true,
    )? {
        cfg.apply_preset("local")?;
    } else {
        eprintln!("  1. Local ONNX models\n  2. No-download demo (hash retrieval + best memory)\n  3. LLM answers with local search\n  4. Remote LightMem vault\n  0. Cancel");
        match line("Choose [1-4]: ")?.as_str() {
            "1" => cfg.apply_preset("local")?,
            "2" => cfg.apply_preset("demo")?,
            "3" => {
                cfg.apply_preset("local")?;
                cfg.ollama_url = LightMemConfig::normalize_ollama_url(&value(
                    "Model service URL",
                    &cfg.ollama_url,
                )?);
                let model = value("Answer model (Ollama name)", "qwen2.5:3b")?;
                cfg.reranker = format!("ollama:{model}");
                cfg.entity_extraction = if confirm(
                    "Also send new memory text to this endpoint for entity extraction?",
                    false,
                )? {
                    "llm"
                } else {
                    "off"
                }
                .into();
            }
            "4" => {
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
            "0" => bail!("Setup cancelled. No settings changed."),
            _ => bail!("Invalid choice. No settings changed."),
        }
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

pub fn edit_configuration(previous: &LightMemConfig) -> Result<Option<LightMemConfig>> {
    require_terminal()?;
    eprintln!(
        "Change shared settings in {}",
        LightMemConfig::config_file().display()
    );
    eprintln!("  1. Search\n  2. Answers\n  3. Extraction\n  4. Remote LightMem connection\n  5. Credentials\n  0. Cancel");
    let mut cfg = previous.clone();
    match line("Choose [1-5]: ")?.as_str() {
        "0" => return Ok(None),
        "1" => {
            eprintln!(
                "Embeddings help find related meanings. Changing them may require a vault reindex."
            );
            cfg.backend = value("Engine: onnx, ollama, hash", &cfg.backend)?;
            match cfg.backend.as_str() {
                "onnx" => {
                    cfg.onnx_model = Some(value(
                        "Model: bge-small, minilm, nomic, or custom directory",
                        cfg.onnx_model.as_deref().unwrap_or("bge-small"),
                    )?)
                }
                "ollama" => {
                    cfg.ollama_url = LightMemConfig::normalize_ollama_url(&value(
                        "Model service URL (shared with LLM answers/extraction)",
                        &cfg.ollama_url,
                    )?);
                    cfg.embedding_model = value("Ollama embedding model", &cfg.embedding_model)?;
                }
                "hash" => {
                    eprintln!("Demo mode uses deterministic hashes; no neural semantic model.")
                }
                _ => bail!("Choose onnx, ollama, or hash. No settings changed."),
            }
        }
        "2" => {
            eprintln!("Choose minilm-squad2, tinyroberta-squad2, top1, ollama, or ollama:<model>.");
            cfg.reranker = super::parse_reranker_setting(&value("Answer engine", &cfg.reranker)?)?;
            if cfg.reranker.starts_with("ollama") {
                cfg.ollama_url = LightMemConfig::normalize_ollama_url(&value(
                    "Model service URL (shared with search/extraction)",
                    &cfg.ollama_url,
                )?);
            }
        }
        "3" => {
            eprintln!("LLM extraction sends newly saved memory text to {}. Search and answers are separate.", cfg.effective_ollama_url());
            cfg.entity_extraction = value("Entity extraction: off, llm", &cfg.entity_extraction)?;
        }
        "4" => {
            eprintln!("This is the memory server, separate from the model service. Environment routing can override saved values.");
            let url = value(
                "Remote URL (none = clear saved routing/key)",
                cfg.remote_url.as_deref().unwrap_or("none"),
            )?;
            if url.eq_ignore_ascii_case("none") {
                cfg.remote_url = None;
                cfg.api_key = None;
            } else {
                http_url(&url)?;
                cfg.remote_url = Some(url.trim_end_matches('/').into());
            }
        }
        "5" => match line("Credential: 1. Model service, 2. Remote LightMem: ")?.as_str() {
            "1" => {
                let present = cfg.effective_ollama_api_key().is_some();
                credential(&mut cfg.ollama_api_key, "model service", present)?;
            }
            "2" => {
                let present = cfg.effective_api_key().is_some();
                credential(&mut cfg.api_key, "remote LightMem", present)?;
            }
            _ => bail!("Invalid credential choice. No settings changed."),
        },
        _ => bail!("Invalid choice. No settings changed."),
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

pub fn render_config(cfg: &LightMemConfig, db: Option<&Path>, global: bool) -> Result<()> {
    CliView::render_config(cfg, false, false, db, global)
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
        println!("  {}", CliView::crimson("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"));
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
                "ok" => ("✓".green().bold(), name.white().bold()),
                "info" => ("ℹ".cyan().bold(), name.white()),
                "error" => ("✗".red().bold(), name.red().bold()),
                "needs_reindex" => ("⚠️".yellow().bold(), name.yellow().bold()),
                _ => ("○".dimmed(), name.dimmed()),
            };
            let prefix = if i == last_idx { "  ╰─" } else { "  │ " };
            println!(
                "{} {} {:<16} {}",
                prefix,
                glyph,
                styled_name,
                message
            );
        }
        println!();
        if ok {
            println!(
                "  {} {}",
                "✓".green().bold(),
                "All core system checks passed and vault is healthy.".green().bold()
            );
        } else {
            println!(
                "  {} {}",
                "⚠️".yellow().bold(),
                "One or more diagnostic checks require attention.".yellow().bold()
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
        let pct = if self.total == 0 {
            100
        } else {
            (current * 100) / self.total
        };
        let filled = if self.total == 0 {
            self.width
        } else {
            (current * self.width) / self.total
        };
        let unfilled = self.width.saturating_sub(filled);
        let bar_filled: String = "▰".repeat(filled);
        let bar_unfilled: String = "▱".repeat(unfilled);

        let max_msg_len = 32;
        let display_msg = if message.chars().count() > max_msg_len {
            let mut s = message.chars().take(max_msg_len.saturating_sub(3)).collect::<String>();
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
