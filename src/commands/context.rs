use anyhow::{Context, Result};
use colored::*;
use lightmem::{CliView, LightMem, LightMemConfig, ResolvedConnection};
use std::path::{Path, PathBuf};

pub fn open_controller(
    db_path: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<LightMem> {
    open_controller_with_session(db_path, global, connection, None)
}

pub fn open_controller_with_session(
    db_path: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
    session: Option<&str>,
) -> Result<LightMem> {
    if let Some(connection) = connection {
        return LightMem::open_resolved(connection);
    }
    let resolved_path = db_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| LightMemConfig::resolve_db_path(global));
    let config = LightMemConfig::try_load()?;
    if let Some(sid) = session.map(str::trim).filter(|s| !s.is_empty()) {
        lightmem::models::identity::validate_id(sid)?;
        let context = lightmem::MemoryContext {
            connection_id: "default".to_string(),
            user_id: "default".to_string(),
            agent_id: "default".to_string(),
            harness_id: "cli".to_string(),
            project_id: None,
            scope: lightmem::MemoryScope::User,
            session_id: Some(sid.to_string()),
        };
        return LightMem::open_at_with_context(&resolved_path, config, context);
    }
    if db_path.is_some() {
        LightMem::open_at(&resolved_path, config)
    } else {
        LightMem::open_default(global)
    }
}

pub fn remove_db_files(db_path: &Path) {
    let _ = std::fs::remove_file(db_path);
    if let Some(s) = db_path.to_str() {
        let _ = std::fs::remove_file(format!("{}-wal", s));
        let _ = std::fs::remove_file(format!("{}-shm", s));
    }
    let att_dir = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join(".lightmem_attachments");
    if att_dir.is_dir()
        && att_dir
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s == ".lightmem_attachments")
            .unwrap_or(false)
    {
        let _ = std::fs::remove_dir_all(&att_dir);
    }
}

pub fn handle_remote_network_error(
    remote_url: &str,
    err: &anyhow::Error,
    json: bool,
) -> anyhow::Error {
    let err_str = format!("{err:#}");
    let is_auth = err_str.contains("401")
        || err_str.contains("Unauthorized")
        || err_str.contains("Authentication failed");

    if json {
        let payload = if is_auth {
            serde_json::json!({
                "status": "error",
                "error": "unauthorized",
                "code": 401,
                "remote_url": remote_url,
                "message": "Invalid or missing API key for remote server",
                "hint": format!("Run 'lmem remote {remote_url} --api-key <KEY>' or 'lmem remote' to configure an API key.")
            })
        } else {
            serde_json::json!({
                "status": "error",
                "error": "connection_failed",
                "remote_url": remote_url,
                "message": err_str,
                "hint": "Run 'lmem local' to switch back to local vault."
            })
        };
        eprintln!("{payload}");
        return AlreadyHandledError::new(err_str).into();
    }

    if is_auth {
        eprintln!(
            "\n{} {}",
            "⚠️".yellow(),
            format!("Authentication failed (401 Unauthorized) at {remote_url}")
                .red()
                .bold()
        );
        eprintln!(
            "  {} {} The remote server requires a valid API key.",
            "◈".dimmed(),
            CliView::pad_styled("Reason:", "Reason:".white().bold(), 12)
        );
        eprintln!();
        eprintln!("  {} {}", "💡".dimmed(), "How to fix:".white().bold());
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Set API key:",
            format!("lmem remote {remote_url} --api-key \"<YOUR_API_KEY>\"").cyan()
        );
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Interactive:",
            "lmem remote".cyan()
        );
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Local vault:",
            "lmem local".cyan()
        );
        eprintln!();
    } else {
        eprintln!(
            "\n{} {}",
            "⚠️".yellow(),
            format!("Cannot connect to remote LightMem backend at {remote_url}")
                .red()
                .bold()
        );
        eprintln!(
            "  {} {} {}",
            "◈".dimmed(),
            CliView::pad_styled("Details:", "Details:".white().bold(), 12),
            err_str.dimmed()
        );
        eprintln!();
        eprintln!("  {} {}", "💡".dimmed(), "Troubleshooting:".white().bold());
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Start daemon:",
            "lmem serve --daemon".cyan()
        );
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Check status:",
            "lmem serve --status".cyan()
        );
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Switch to local:",
            "lmem local".cyan()
        );
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Explicit vault:",
            "lmem --db ~/.lightmem/memories.db <CMD>".cyan()
        );
        eprintln!();
    }
    AlreadyHandledError::new(err_str).into()
}

pub fn open_file_in_default_app(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .status()
            .with_context(|| format!("Failed to open {} with 'open'", path.display()))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .status()
            .with_context(|| format!("Failed to open {} with 'start'", path.display()))?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .status()
            .with_context(|| format!("Failed to open {} with 'xdg-open'", path.display()))?;
    }
    Ok(())
}

pub fn open_files_in_default_app(paths: &[PathBuf]) {
    for p in paths {
        if let Err(err) = open_file_in_default_app(p) {
            eprintln!("Warning: {}", err);
        }
    }
}

#[derive(Debug)]
pub struct AlreadyHandledError(pub String);

impl AlreadyHandledError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl std::fmt::Display for AlreadyHandledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AlreadyHandledError {}
