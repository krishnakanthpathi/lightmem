use anyhow::{Context, Result};
use colored::*;
use lightmem::{
    CliView, ConnectService, DocsOptions, IdentityRegistry, LightMemConfig, ResolvedConnection,
    ServerOptions,
};
use serde::{Deserialize, Serialize};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::context::{open_controller, AlreadyHandledError};
use crate::{terminal, Cli, IdentityCommand};

#[derive(Serialize, Deserialize, Debug)]
struct ServerDaemonInfo {
    pid: u32,
    host: String,
    port: u16,
    started_at: String,
}

#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn is_pid_alive(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid)])
        .output();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        text.contains(&pid.to_string())
    } else {
        false
    }
}

#[cfg(unix)]
fn terminate_pid(pid: u32) -> bool {
    unsafe {
        let _ = libc::kill(pid as i32, libc::SIGTERM);
    }
    for _ in 0..30 {
        if !is_pid_alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    unsafe {
        let _ = libc::kill(pid as i32, libc::SIGKILL);
    }
    !is_pid_alive(pid)
}

#[cfg(windows)]
fn terminate_pid(pid: u32) -> bool {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status();
    !is_pid_alive(pid)
}

pub fn handle_identity(action: &IdentityCommand, json: bool, default_db: PathBuf) -> Result<()> {
    let registry = IdentityRegistry::open_default()?;
    match action {
        IdentityCommand::Status => {
            let overview = registry.status_overview(default_db)?;
            CliView::render_vault_status_overview(&overview, json)?;
        }
        IdentityCommand::UserAdd { id, name } => {
            let value = serde_json::to_value(registry.add_user(id, name.as_deref())?)?;
            CliView::render_identity_entry("Registered user", &value, json)?;
        }
        IdentityCommand::UserRemove { id } => {
            let removed = registry.remove_user(id)?;
            CliView::render_identity_removed("user", id, removed, json)?;
            if !removed {
                return Err(AlreadyHandledError::new(format!("User '{}' not found", id)).into());
            }
        }
        IdentityCommand::AgentAdd { id, user, name } => {
            let value = serde_json::to_value(registry.add_agent(id, user, name.as_deref())?)?;
            CliView::render_identity_entry("Registered agent", &value, json)?;
        }
        IdentityCommand::AgentRemove { id } => {
            let removed = registry.remove_agent(id)?;
            CliView::render_identity_removed("agent", id, removed, json)?;
            if !removed {
                return Err(AlreadyHandledError::new(format!("Agent '{}' not found", id)).into());
            }
        }
        IdentityCommand::HarnessAdd { id, path, name } => {
            let value =
                serde_json::to_value(registry.add_harness(id, path.clone(), name.as_deref())?)?;
            CliView::render_identity_entry("Registered harness", &value, json)?;
        }
        IdentityCommand::HarnessRemove { id } => {
            let removed = registry.remove_harness(id)?;
            CliView::render_identity_removed("harness", id, removed, json)?;
            if !removed {
                return Err(AlreadyHandledError::new(format!("Harness '{}' not found", id)).into());
            }
        }
        IdentityCommand::ConnectionAdd {
            id,
            user,
            agent,
            harness,
            project,
            memory_scope,
        } => {
            let value = serde_json::to_value(registry.add_connection(
                id,
                user,
                agent,
                harness,
                project.clone(),
                memory_scope.clone(),
            )?)?;
            CliView::render_identity_entry("Registered connection", &value, json)?;
        }
        IdentityCommand::ConnectionUpdate {
            id,
            project,
            clear_project,
            memory_scope,
        } => {
            let proj_arg = if *clear_project {
                Some(None)
            } else {
                project.as_ref().map(|p| Some(p.clone()))
            };
            let value = serde_json::to_value(registry.update_connection(
                id,
                memory_scope.clone(),
                proj_arg,
            )?)?;
            CliView::render_identity_entry("Updated connection", &value, json)?;
        }
        IdentityCommand::ConnectionRemove { id } => {
            let removed = registry.remove_connection(id)?;
            CliView::render_identity_removed("connection", id, removed, json)?;
            if !removed {
                return Err(
                    AlreadyHandledError::new(format!("Connection '{}' not found", id)).into(),
                );
            }
        }
        IdentityCommand::List { kind } => {
            let value = registry.list(kind)?;
            CliView::render_identity_list(kind, &value, json)?;
        }
        IdentityCommand::Show { id } => {
            let value = serde_json::to_value(registry.resolve(id, None, None)?)?;
            CliView::render_identity_entry("Connection", &value, json)?;
        }
    }
    Ok(())
}

pub fn handle_connect(
    platform: Option<String>,
    workspace: bool,
    path: Option<PathBuf>,
    list: bool,
    json: bool,
    mcp: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    if let Some(connection) = connection {
        if list {
            CliView::render_platforms_list(
                &ConnectService::list_platforms(workspace),
                workspace,
                json,
            )?;
        } else {
            let results = ConnectService::connect_bound(
                connection,
                platform.as_deref(),
                workspace,
                path.as_deref(),
                mcp,
            )?;
            CliView::render_connect(&results, json)?;
        }
    } else if mcp {
        let results = ConnectService::connect_mcp(platform.as_deref(), workspace)?;
        CliView::render_connect(&results, json)?;
    } else if list {
        let platforms = ConnectService::list_platforms(workspace);
        CliView::render_platforms_list(&platforms, workspace, json)?;
    } else {
        let results = ConnectService::connect(platform.as_deref(), workspace, path.as_deref())?;
        CliView::render_connect(&results, json)?;
    }
    Ok(())
}

#[cfg(feature = "mcp")]
pub fn handle_mcp(
    global: bool,
    effective_db: Option<&Path>,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let server = lightmem::McpServer::new(global, effective_db.map(Path::to_path_buf))
        .with_connection(connection.cloned());
    server.run_stdio()
}

pub fn handle_docs(port: Option<u16>, landing: bool, no_browser: bool) -> Result<()> {
    let options = DocsOptions {
        port,
        open_landing: landing,
        no_browser,
    };
    lightmem::serve_docs(options)
}

#[allow(clippy::too_many_arguments)]
pub fn handle_serve(
    host: String,
    port: u16,
    api_key: Option<String>,
    no_auth: bool,
    daemon: bool,
    stop: bool,
    status: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
    cli_db: Option<&Path>,
    cli_connection: Option<&str>,
) -> Result<()> {
    let pid_file = LightMemConfig::config_dir().join("server.pid");

    if stop {
        if !pid_file.exists() {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "not_running", "pid": null })
                );
            } else {
                println!(
                    "{}",
                    "LightMem server daemon is not running (no PID file found).".dimmed()
                );
            }
            return Ok(());
        }
        let content = std::fs::read_to_string(&pid_file)?;
        let info: Result<ServerDaemonInfo, _> = serde_json::from_str(&content);
        let pid = match info {
            Ok(ref d) => d.pid,
            Err(_) => content
                .trim()
                .parse::<u32>()
                .context("Invalid PID in server.pid")?,
        };

        if is_pid_alive(pid) {
            if terminate_pid(pid) {
                let _ = std::fs::remove_file(&pid_file);
                if json {
                    println!("{}", serde_json::json!({ "status": "stopped", "pid": pid }));
                } else {
                    println!(
                        "  {} LightMem server daemon stopped (PID: {}).",
                        "✓".green().bold(),
                        pid
                    );
                }
            } else {
                anyhow::bail!(
                    "Failed to terminate LightMem server process with PID {}",
                    pid
                );
            }
        } else {
            let _ = std::fs::remove_file(&pid_file);
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "not_running", "stale_pid": pid })
                );
            } else {
                println!(
                    "{}",
                    format!(
                        "LightMem server daemon was not running (cleaned up stale PID file: {}).",
                        pid
                    )
                    .dimmed()
                );
            }
        }
        return Ok(());
    }

    if status {
        if !pid_file.exists() {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "stopped", "running": false, "pid": null })
                );
                return Ok(());
            }
            println!(
                "  {}  {}",
                "❖".red().bold(),
                "L I G H T M E M  Daemon Status".white().bold()
            );
            println!(
                "  {}",
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".red()
            );
            println!(
                "  {} State:         {}",
                "▸".red().bold(),
                "STOPPED".red().bold()
            );
            println!("  {} Command:       lmem serve -d\n", "▸".dimmed());
            return Ok(());
        }
        let content = std::fs::read_to_string(&pid_file)?;
        let info: Result<ServerDaemonInfo, _> = serde_json::from_str(&content);
        let (pid, host_str, port_num, started) = match info {
            Ok(ref d) => (d.pid, d.host.clone(), d.port, Some(d.started_at.clone())),
            Err(_) => {
                let p = content
                    .trim()
                    .parse::<u32>()
                    .context("Invalid PID in server.pid")?;
                (p, "127.0.0.1".to_string(), 8080, None)
            }
        };

        if is_pid_alive(pid) {
            let probe_url = format!("http://127.0.0.1:{}/v1/health", port_num);
            let responding = ureq::get(&probe_url)
                .timeout(std::time::Duration::from_millis(500))
                .call()
                .is_ok();
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "running",
                        "running": true,
                        "healthy": responding,
                        "pid": pid,
                        "host": host_str,
                        "port": port_num,
                        "started_at": started,
                        "log_file": LightMemConfig::config_dir().join("server.log"),
                    })
                );
                return Ok(());
            }
            println!(
                "\n  {}  {}",
                "❖".green().bold(),
                "L I G H T M E M  Daemon Status".white().bold()
            );
            println!(
                "  {}",
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".green()
            );
            println!(
                "  {} State:         {} {}",
                "▸".green().bold(),
                "RUNNING".green().bold(),
                if responding {
                    "(HTTP 200 OK)".dimmed()
                } else {
                    "(Starting...)".yellow()
                }
            );
            println!("  {} Process PID:       {}", "▸".dimmed(), pid);
            if host_str == "0.0.0.0" {
                println!(
                    "  {} Listening:         0.0.0.0:{} (all interfaces)",
                    "▸".dimmed(),
                    port_num
                );
                println!("  {} Network Endpoints:", "▸".dimmed());
                let endpoints = lightmem::services::api_server::discover_host_endpoints();
                for ep in &endpoints {
                    println!(
                        "    {} {:<12} http://{}:{} ({})",
                        "•".dimmed(),
                        ep.label.bold(),
                        ep.ip,
                        port_num,
                        ep.if_name
                    );
                }
            } else {
                println!(
                    "  {} Address:           http://{}:{}",
                    "▸".dimmed(),
                    host_str,
                    port_num
                );
            }
            if let Some(s) = started {
                println!("  {} Started:       {}", "▸".dimmed(), s);
            }
            println!(
                "  {} Logs:          {}/server.log",
                "▸".dimmed(),
                LightMemConfig::config_dir().display()
            );
            println!("  {} Stop:          lmem serve --stop\n", "▸".dimmed());
        } else {
            let _ = std::fs::remove_file(&pid_file);
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "stopped",
                        "running": false,
                        "stale_pid": pid,
                    })
                );
                return Ok(());
            }
            println!(
                "  {}  {}",
                "❖".red().bold(),
                "L I G H T M E M  Daemon Status".white().bold()
            );
            println!(
                "  {}",
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".red()
            );
            println!(
                "  {} State:         {} {}",
                "▸".red().bold(),
                "STOPPED".red().bold(),
                format!("(stale PID {} cleaned up)", pid).dimmed()
            );
            println!("  {} Command:       lmem serve -d\n", "▸".dimmed());
        }
        return Ok(());
    }

    if daemon {
        anyhow::ensure!(
            no_auth || api_key.as_ref().is_none_or(|k| !k.trim().is_empty()),
            "API key cannot be blank. Omit it to generate one, or explicitly use --no-auth."
        );
        if pid_file.exists() {
            let content = std::fs::read_to_string(&pid_file).unwrap_or_default();
            let pid = serde_json::from_str::<ServerDaemonInfo>(&content)
                .map(|d| d.pid)
                .or_else(|_| content.trim().parse::<u32>())
                .unwrap_or(0);
            if pid > 0 && is_pid_alive(pid) {
                anyhow::bail!(
                    "LightMem server daemon is already running (PID: {}). Run 'lmem serve --stop' first.",
                    pid
                );
            }
            let _ = std::fs::remove_file(&pid_file);
        }

        let key = if no_auth {
            None
        } else {
            Some(api_key.unwrap_or_else(|| {
                let u = uuid::Uuid::new_v4().to_string().replace('-', "");
                format!("lmem_{}", u)
            }))
        };

        let config_dir = LightMemConfig::config_dir();
        std::fs::create_dir_all(&config_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let log_path = config_dir.join("server.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("Failed to open log file {}", log_path.display()))?;
        let err_file = log_file.try_clone()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            log_file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }

        let exe = std::env::current_exe()?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("serve")
            .arg("--host")
            .arg(&host)
            .arg("--port")
            .arg(port.to_string());

        if let Some(ref k) = key {
            cmd.arg("--api-key").arg(k);
        }
        if no_auth {
            cmd.arg("--no-auth");
        }
        if let Some(db) = cli_db {
            cmd.arg("--db").arg(db);
        }
        if global {
            cmd.arg("--global");
        }
        if let Some(conn) = cli_connection {
            cmd.arg("--connection").arg(conn);
        }
        if let Some(resolved) = connection {
            cmd.arg("--scope").arg(match resolved.context.scope {
                lightmem::MemoryScope::User => "user",
                lightmem::MemoryScope::Project => "project",
                lightmem::MemoryScope::Agent => "agent",
            });
            if let Some(session) = &resolved.context.session_id {
                cmd.arg("--session").arg(session);
            }
        }

        cmd.stdin(std::process::Stdio::null())
            .stdout(log_file)
            .stderr(err_file);

        #[cfg(unix)]
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }

        let child = cmd
            .spawn()
            .context("Failed to spawn background daemon process")?;
        let child_pid = child.id();

        std::thread::sleep(std::time::Duration::from_millis(300));

        if !is_pid_alive(child_pid) {
            let err_msg = std::fs::read_to_string(&log_path)
                .ok()
                .and_then(|logs| {
                    logs.lines()
                        .rev()
                        .take(5)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n")
                        .into()
                })
                .unwrap_or_default();
            if err_msg.contains("Address already in use")
                || err_msg.contains("os error 98")
                || err_msg.contains("os error 48")
                || err_msg.contains("os error 10048")
            {
                let alt_port = if port == 8080 { 8089 } else { port + 1 };
                eprintln!(
                    "\n{} {}",
                    "⚠️".yellow(),
                    format!("Cannot start daemon on {}:{}", host, port)
                        .yellow()
                        .bold()
                );
                eprintln!(
                    "  {} {:<12} Port {} is already in use by another process.",
                    "◈".dimmed(),
                    "Reason:".white().bold(),
                    port
                );
                eprintln!();
                eprintln!("  {} {}", "💡".dimmed(), "How to fix:".white().bold());
                eprintln!(
                    "    {} {:<18} {}",
                    "•".dimmed(),
                    "Use another port:",
                    format!("lmem serve -d --port {}", alt_port).cyan()
                );
                eprintln!(
                    "    {} {:<18} {}",
                    "•".dimmed(),
                    "Check active daemon:",
                    "lmem serve --status".yellow()
                );
                eprintln!(
                    "    {} {:<18} {}",
                    "•".dimmed(),
                    "Stop running daemon:",
                    "lmem serve --stop".yellow()
                );
                eprintln!();
                return Err(
                    AlreadyHandledError::new(format!("Port {} is already in use", port)).into(),
                );
            }
            anyhow::bail!(
                "LightMem server daemon failed to start (process exited immediately).\nLogs:\n{}",
                err_msg
            );
        }

        let daemon_info = ServerDaemonInfo {
            pid: child_pid,
            host: host.clone(),
            port,
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        std::fs::write(&pid_file, serde_json::to_string_pretty(&daemon_info)?)?;

        println!(
            "\n  {}  {}",
            "❖".green().bold(),
            "L I G H T M E M  Daemon Started".white().bold()
        );
        println!(
            "  {}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".green()
        );
        println!("  {} PID:               {}", "▸".green().bold(), child_pid);
        if host == "0.0.0.0" {
            println!(
                "  {} Listening:         0.0.0.0:{} (all interfaces)",
                "▸".dimmed(),
                port
            );
            println!("  {} Network Endpoints:", "▸".dimmed());
            let endpoints = lightmem::services::api_server::discover_host_endpoints();
            for ep in &endpoints {
                println!(
                    "    {} {:<12} http://{}:{} ({})",
                    "•".dimmed(),
                    ep.label.bold(),
                    ep.ip,
                    port,
                    ep.if_name
                );
            }
        } else {
            println!(
                "  {} Listening:         http://{}:{}",
                "▸".dimmed(),
                host,
                port
            );
        }
        if let Some(ref k) = key {
            println!(
                "  {} API Key:           {}  (Keep this secret!)",
                "▸".dimmed(),
                k.yellow().bold()
            );
            println!("  {} Quick Connect (Examples):", "▸".dimmed());
            if host == "0.0.0.0" {
                let endpoints = lightmem::services::api_server::discover_host_endpoints();
                for ep in &endpoints {
                    println!(
                        "    {} {:<12} lmem remote http://{}:{} --api-key \"{}\"",
                        "•".dimmed(),
                        ep.label,
                        ep.ip,
                        port,
                        k
                    );
                }
            } else {
                println!(
                    "    {} {:<12} lmem remote http://{}:{} --api-key \"{}\"",
                    "•".dimmed(),
                    "Remote",
                    host,
                    port,
                    k
                );
            }
        } else {
            println!("  {} Authentication:    None (--no-auth)", "▸".dimmed());
        }
        println!(
            "  {} Health Probe:  http://127.0.0.1:{}/v1/health",
            "▸".dimmed(),
            port
        );
        println!("  {} Logs:          {}", "▸".dimmed(), log_path.display());
        println!("  {} Status:        lmem serve --status", "▸".dimmed());
        println!("  {} Stop:          lmem serve --stop\n", "▸".dimmed());

        return Ok(());
    }

    // Foreground mode
    let lm = open_controller(effective_db, global, connection)?;
    let options = ServerOptions {
        host: host.clone(),
        port,
        api_key,
        no_auth,
    };
    if let Err(e) = lightmem::serve_api(Arc::new(lm), options) {
        let err_str = format!("{e:#}");
        if err_str.contains("Address already in use")
            || err_str.contains("os error 98")
            || err_str.contains("os error 48")
            || err_str.contains("os error 10048")
        {
            let alt_port = if port == 8080 { 8089 } else { port + 1 };
            eprintln!(
                "\n{} {}",
                "⚠️".yellow(),
                format!("Cannot start server on {}:{}", host, port)
                    .yellow()
                    .bold()
            );
            eprintln!(
                "  {} {:<12} Port {} is already in use by another process.",
                "◈".dimmed(),
                "Reason:".white().bold(),
                port
            );
            eprintln!();
            eprintln!("  {} {}", "💡".dimmed(), "How to fix:".white().bold());
            eprintln!(
                "    {} {:<18} {}",
                "•".dimmed(),
                "Use another port:",
                format!("lmem serve --port {}", alt_port).cyan()
            );
            eprintln!(
                "    {} {:<18} {}",
                "•".dimmed(),
                "Run as daemon:",
                format!("lmem serve -d --port {}", alt_port).cyan()
            );
            eprintln!(
                "    {} {:<18} {}",
                "•".dimmed(),
                "Check active daemon:",
                "lmem serve --status".yellow()
            );
            eprintln!(
                "    {} {:<18} {}",
                "•".dimmed(),
                "Stop running daemon:",
                "lmem serve --stop".yellow()
            );
            eprintln!();
            std::process::exit(1);
        }
        return Err(e);
    }
    Ok(())
}

pub fn handle_uninstall(purge_data: bool, yes: bool, json: bool) -> Result<()> {
    if json {
        anyhow::ensure!(
            yes,
            "Non-interactive uninstallation via --json requires --yes"
        );
    } else {
        println!(
            "\n  {}  {}  {}",
            "❖".red().bold(),
            "L I G H T M E M".white().bold(),
            "Uninstaller".dimmed()
        );
        println!(
            "  {}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".red()
        );
        println!(
            "  {} {}",
            "▸".red().bold(),
            "Removing LightMem engine, binaries & shell completions".white()
        );
        println!(
            "  {}\n",
            "────────────────────────────────────────────────".dimmed()
        );

        if !yes && std::io::stdin().is_terminal() {
            print!(
                "  {} ",
                "? Proceed with uninstalling LightMem? [y/N]:"
                    .yellow()
                    .bold()
            );
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let trimmed = input.trim().to_lowercase();
            if trimmed != "y" && trimmed != "yes" {
                println!("\n  {}\n", "Aborted by user.".dimmed());
                return Ok(());
            }
            println!();
        }
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    clean_rc_file(&home.join(".zshrc"), "lightmem/completions")?;
    clean_rc_file(&home.join(".bashrc"), "lightmem/completions")?;

    if let Ok(entries) = std::fs::read_dir(&home) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(".zcompdump") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    let comp_dir = home.join(".lightmem").join("completions");
    if comp_dir.exists() {
        let _ = std::fs::remove_dir_all(&comp_dir);
        println!(
            "  {} Removed shell completions: ~/.lightmem/completions",
            "◈".green().bold()
        );
    }

    let bin_candidates = [
        home.join(".local/bin/lmem"),
        home.join(".local/bin/lmem.exe"),
        home.join(".cargo/bin/lmem"),
        home.join(".cargo/bin/lmem.exe"),
        home.join(".lightmem/bin/lmem"),
        home.join(".lightmem/bin/lmem.exe"),
    ];
    let mut removed_count = 0;
    for bin in &bin_candidates {
        if bin.exists() && std::fs::remove_file(bin).is_ok() {
            println!("  {} Removed binary: {}", "◈".green().bold(), bin.display());
            removed_count += 1;
        }
    }

    let lightmem_bin_dir = home.join(".lightmem").join("bin");
    if lightmem_bin_dir.exists() {
        let _ = std::fs::remove_dir_all(&lightmem_bin_dir);
    }
    if removed_count == 0 {
        println!(
            "  {}",
            "▸ No standalone binaries found in ~/.local/bin or ~/.cargo/bin".dimmed()
        );
    }

    let pip_cmds: &[&[&str]] = &[
        &["pip3", "uninstall", "-y", "lmem"],
        &["pip", "uninstall", "-y", "lmem"],
        &["python3", "-m", "pip", "uninstall", "-y", "lmem"],
        &["python", "-m", "pip", "uninstall", "-y", "lmem"],
    ];
    for cmd in pip_cmds {
        if let Ok(output) = std::process::Command::new(cmd[0]).args(&cmd[1..]).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("Successfully uninstalled") {
                    println!(
                        "  {} Uninstalled lmem Python package ({})",
                        "◈".green().bold(),
                        cmd[0]
                    );
                    break;
                }
            }
        }
    }

    let lightmem_dir = home.join(".lightmem");
    if purge_data {
        if lightmem_dir.exists() {
            let _ = std::fs::remove_dir_all(&lightmem_dir);
            println!(
                "  {} Purged all data & models: {}",
                "◈".red().bold(),
                lightmem_dir.display()
            );
        }
    } else {
        let vault_file = lightmem_dir.join("memories.db");
        if vault_file.exists() {
            println!(
                "\n  {} Persistent vault preserved at: {}",
                "ℹ".cyan().bold(),
                vault_file.display()
            );
            println!("    (To delete your memories vault as well, pass: --purge-data)");
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if current_exe.exists() && !bin_candidates.contains(&current_exe) {
            let _ = std::fs::remove_file(&current_exe);
        }
    }

    if json {
        let obj = serde_json::json!({
            "status": "uninstalled",
            "purged_data": purge_data,
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
        return Ok(());
    }

    println!(
        "\n  {} LightMem has been successfully uninstalled.",
        "✔".green().bold()
    );
    println!("    Restart your shell or run 'rehash' to complete.\n");
    Ok(())
}

fn clean_rc_file(path: &Path, pattern: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    if !content.contains(pattern) {
        return Ok(());
    }
    let mut cleaned_lines = Vec::new();
    let mut skip = false;
    for line in content.lines() {
        if line.contains("# LightMem CLI completions") {
            skip = true;
            continue;
        }
        if skip {
            if line.contains("compinit")
                || line.contains("menu select")
                || line.contains("lmem.bash")
                || line.contains("menu-complete")
                || line.contains("show-all-if-ambiguous")
                || line.contains("lightmem/completions")
            {
                continue;
            }
            if line.trim().is_empty() {
                skip = false;
                continue;
            }
            skip = false;
        }
        if line.contains(pattern) {
            continue;
        }
        cleaned_lines.push(line);
    }
    let new_content = cleaned_lines.join("\n") + "\n";
    std::fs::write(path, new_content)?;
    println!(
        "  {} Cleaned completion hooks from {}",
        "◈".green().bold(),
        path.display()
    );
    Ok(())
}

pub fn print_completions(shell: clap_complete::Shell) -> Result<()> {
    use clap::builder::PossibleValuesParser;
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    cmd = cmd.mut_arg("scope", |a| {
        a.value_parser(PossibleValuesParser::new(["user", "project", "agent"]))
    });

    cmd = cmd.mut_subcommand("remember", |sub| {
        sub.mut_arg("ttl", |a| {
            a.value_parser(PossibleValuesParser::new(["1h", "24h", "7d", "30d"]))
        })
        .mut_arg("provenance", |a| {
            a.value_parser(PossibleValuesParser::new([
                "explicit_statement",
                "validated",
                "observed",
                "corrected",
                "inferred",
                "imported",
                "attachment_metadata",
                "attachment_llm_extracted",
            ]))
        })
    });
    cmd = cmd.mut_subcommand("config", |sub| {
        sub.mut_arg("preset", |a| {
            a.value_parser(PossibleValuesParser::new([
                "demo",
                "local",
                "ollama",
                "ollama-local",
                "ollama-cloud",
                "openai",
                "openrouter",
                "gemini",
                "groq",
                "deepseek",
                "mistral",
                "custom",
            ]))
        })
        .mut_arg("backend", |a| {
            a.value_parser(PossibleValuesParser::new(["onnx", "ollama", "hash"]))
        })
        .mut_arg("onnx_model", |a| {
            a.value_parser(PossibleValuesParser::new(["bge-small", "minilm", "nomic"]))
        })
        .mut_arg("download", |a| {
            a.value_parser(PossibleValuesParser::new([
                "bge-small",
                "minilm",
                "nomic",
                "minilm-squad2",
                "tinyroberta-squad2",
                "qa",
                "all",
            ]))
        })
        .mut_arg("reranker", |a| {
            a.value_parser(PossibleValuesParser::new([
                "minilm-squad2",
                "tinyroberta-squad2",
                "onnx",
                "ollama",
                "ollama:bonsai-2:2b",
                "ollama:gemma4:31b-cloud",
                "ollama:qwen2.5:3b",
                "ollama:qwen2.5:1.5b",
                "ollama:llama3.2:1b",
                "openai:gpt-4o-mini",
                "gemini:gemini-1.5-flash",
                "groq:llama-3.3-70b-versatile",
                "openrouter:anthropic/claude-3.5-sonnet",
                "deepseek:deepseek-chat",
                "mistral:mistral-small-latest",
                "top1",
            ]))
        })
    });
    cmd = cmd.mut_subcommand("setup", |sub| {
        sub.mut_arg("preset", |a| {
            a.value_parser(PossibleValuesParser::new([
                "demo",
                "local",
                "ollama",
                "ollama-local",
                "ollama-cloud",
                "openai",
                "openrouter",
                "gemini",
                "groq",
                "deepseek",
                "mistral",
                "custom",
            ]))
        })
        .mut_arg("model", |a| {
            a.value_parser(PossibleValuesParser::new([
                "minilm-squad2",
                "tinyroberta-squad2",
                "ollama",
                "ollama:qwen2.5:3b",
                "openai:gpt-4o-mini",
                "gemini:gemini-1.5-flash",
                "groq:llama-3.3-70b-versatile",
                "openrouter:anthropic/claude-3.5-sonnet",
                "deepseek:deepseek-chat",
                "mistral:mistral-small-latest",
                "top1",
            ]))
        })
    });
    cmd = cmd.mut_subcommand("answer", |sub| {
        sub.mut_arg("reranker", |a| {
            a.value_parser(PossibleValuesParser::new([
                "onnx",
                "ollama",
                "ollama:bonsai-2:2b",
                "openai:gpt-4o-mini",
                "gemini:gemini-1.5-flash",
                "groq:llama-3.3-70b-versatile",
                "openrouter:anthropic/claude-3.5-sonnet",
                "deepseek:deepseek-chat",
                "mistral:mistral-small-latest",
                "top1",
                "minilm-squad2",
                "tinyroberta-squad2",
            ]))
        })
    });
    cmd = cmd.mut_subcommand("conflicts", |sub| {
        sub.mut_arg("reranker", |a| {
            a.value_parser(PossibleValuesParser::new([
                "onnx",
                "ollama",
                "openai:gpt-4o-mini",
                "gemini:gemini-1.5-flash",
                "top1",
                "minilm-squad2",
                "tinyroberta-squad2",
            ]))
        })
    });
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut cmd, "lmem", &mut script);
    let script = String::from_utf8(script)?;
    print!("{}", terminal::complete_connections(shell, script));
    Ok(())
}
