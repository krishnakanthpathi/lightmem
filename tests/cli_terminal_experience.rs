use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use tempfile::{tempdir, TempDir};

struct Fixture {
    root: TempDir,
}
impl Fixture {
    fn new() -> Self {
        Self {
            root: tempdir().unwrap(),
        }
    }
    fn config_root(&self) -> PathBuf {
        self.root.path().join("config")
    }
    fn config(&self) -> PathBuf {
        self.config_root().join("config.json")
    }
    fn db(&self) -> PathBuf {
        self.root.path().join("vault.db")
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lmem"));
        for key in [
            "LIGHTMEM_DB",
            "LIGHTMEM_CONNECTION",
            "LIGHTMEM_REMOTE_URL",
            "LIGHTMEM_URL",
            "LIGHTMEM_API_KEY",
            "LIGHTMEM_KEY",
            "OLLAMA_API_KEY",
            "LMEM_OLLAMA_API_KEY",
        ] {
            cmd.env_remove(key);
        }
        cmd.env("LIGHTMEM_CONFIG_DIR", self.config_root())
            .env("LIGHTMEM_OFFLINE", "1")
            .env("NO_COLOR", "1")
            .current_dir(self.root.path())
            .stdin(Stdio::null());
        cmd
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command()
            .arg("--db")
            .arg(self.db())
            .args(args)
            .output()
            .unwrap()
    }
    fn json(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn demo(&self) {
        self.json(&["setup", "--preset", "demo", "--yes", "--json"]);
    }
}

#[test]
fn doctor_and_completions_do_not_initialize_any_storage() {
    let f = Fixture::new();
    let report = f.json(&["doctor", "--json"]);
    assert_eq!(report["read_only"], true);
    assert_eq!(report["network_tested"], false);
    assert_eq!(report["target_source"], "--db");
    assert_eq!(report["checks"][1]["status"], "not_created");
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        assert!(f.run(&["completions", shell]).status.success());
    }
    assert!(f.run(&["__connections"]).status.success());
    assert!(!f.config_root().exists());
    assert!(!f.db().exists());
}

#[test]
fn doctor_reports_bad_config_as_json_without_replacing_it() {
    let f = Fixture::new();
    std::fs::create_dir(f.config_root()).unwrap();
    std::fs::write(f.config(), "{bad").unwrap();
    let out = f.run(&["doctor", "--json"]);
    assert!(!out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["ok"], false);
    assert_eq!(report["checks"][0]["status"], "error");
    assert_eq!(std::fs::read_to_string(f.config()).unwrap(), "{bad");
    assert!(!f.db().exists());
    // Completion still works when ordinary operations cannot parse settings.
    assert!(f.run(&["completions", "bash"]).status.success());
}

#[test]
fn defaults_and_demo_preview_are_local_and_side_effect_free() {
    let f = Fixture::new();
    let default = f.json(&["config", "--json"]);
    assert_eq!(default["backend"], "onnx");
    assert_eq!(default["reranker"], "minilm-squad2");
    assert_eq!(default["entity_extraction"], "off");
    let plan = f.json(&["setup", "--preset", "demo", "--dry-run", "--json"]);
    assert_eq!(plan["configuration"]["backend"], "hash");
    assert_eq!(plan["configuration"]["reranker"], "top1");
    assert!(!f.config_root().exists());
    assert!(!f.db().exists());
}

#[test]
fn scripts_never_enter_prompts_or_accept_invalid_combinations() {
    let f = Fixture::new();
    f.demo();
    let before = std::fs::read(f.config()).unwrap();
    for args in [
        vec!["config", "--edit"],
        vec!["config", "--edit", "--json"],
        vec!["config", "--edit", "--dry-run"],
        vec!["config", "--edit", "--yes"],
        vec!["setup"],
        vec!["setup", "--yes"],
        vec!["setup", "--preset", "local", "--json"],
    ] {
        assert!(!f.run(&args).status.success(), "{args:?}");
        assert_eq!(std::fs::read(f.config()).unwrap(), before);
        assert!(!f.db().exists());
    }
}

#[test]
fn explicit_model_flags_keep_legacy_meanings_and_reject_conflicting_aliases() {
    let f = Fixture::new();
    f.demo();
    let plan = f.json(&[
        "config",
        "--embedding-model",
        "custom-embedding",
        "--answer-model",
        "top1",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(plan["configuration"]["embedding_model"], "custom-embedding");
    assert_eq!(plan["configuration"]["reranker"], "top1");
    let setup = f.json(&[
        "setup",
        "--answer-model",
        "tinyroberta-squad2",
        "--dry-run",
        "--json",
    ]);
    assert_eq!(setup["configuration"]["reranker"], "tinyroberta-squad2");
    assert_eq!(
        setup["configuration"]["embedding_model"],
        "nomic-embed-text"
    );
    assert!(!f
        .run(&["config", "--model", "a", "--embedding-model", "b"])
        .status
        .success());
}

#[test]
fn model_change_requires_approval_and_failure_keeps_original_memory_and_config() {
    let f = Fixture::new();
    f.demo();
    let memory = f.json(&["remember", "API uses port 8180", "--json"]);
    let before = std::fs::read(f.config()).unwrap();
    let plan = f.json(&["config", "--preset", "local", "--dry-run", "--json"]);
    assert_eq!(plan["migration_required"], true);
    assert!(!f
        .run(&["config", "--preset", "local", "--json"])
        .status
        .success());
    assert_eq!(std::fs::read(f.config()).unwrap(), before);
    let report = f.json(&["doctor", "--json"]);
    assert_eq!(report["ok"], true);
    assert_eq!(
        f.json(&["inspect", memory["id"].as_str().unwrap(), "--json"])["memory"]["content"],
        "API uses port 8180"
    );
}

#[test]
fn all_config_and_preview_credentials_are_redacted() {
    let f = Fixture::new();
    f.demo();
    for args in [
        vec![
            "config",
            "--api-key",
            "remote-private",
            "--ollama-api-key",
            "model-private",
            "--json",
        ],
        vec![
            "config",
            "--api-key",
            "remote-private",
            "--ollama-api-key",
            "model-private",
            "--dry-run",
            "--json",
        ],
    ] {
        let out = f.run(&args);
        assert!(out.status.success());
        let output = String::from_utf8(out.stdout).unwrap();
        assert!(!output.contains("remote-private"));
        assert!(!output.contains("model-private"));
        assert!(output.contains("[REDACTED]"));
    }
}

#[test]
fn doctor_remote_is_offline_and_explicit_global_selects_local() {
    let f = Fixture::new();
    f.demo();
    let mut cmd = f.command();
    let out = cmd
        .env("LIGHTMEM_REMOTE_URL", "http://127.0.0.1:1")
        .args(["doctor", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["mode"], "remote");
    assert_eq!(report["network_tested"], false);
    let out = f
        .command()
        .env("LIGHTMEM_REMOTE_URL", "http://127.0.0.1:1")
        .args(["--global", "doctor", "--json"])
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["mode"], "local");
    assert_eq!(report["target_source"], "--global");
    assert!(!f.db().exists());
}

#[test]
fn live_connection_completion_is_readonly_and_filters_invalid_values() {
    let f = Fixture::new();
    std::fs::create_dir(f.config_root()).unwrap();
    let registry = rusqlite::Connection::open(f.config_root().join("identities.db")).unwrap();
    registry.execute_batch("CREATE TABLE connections(id TEXT, context TEXT); INSERT INTO connections VALUES ('codex-dev','{}'), ('claude','{}'), ('bad shell value','{}');").unwrap();
    let out = f.run(&["__connections"]);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "claude\ncodex-dev\n"
    );
    let out = f.run(&["completions", "zsh"]);
    let script = String::from_utf8(out.stdout).unwrap();
    assert!(script.contains(":CONNECTION:_lmem_connections"));
    assert!(script.contains("lmem __connections"));
    let out = f.run(&["completions", "bash"]);
    let script = String::from_utf8(out.stdout).unwrap();
    assert!(script.contains("_lmem_base()"));
    assert!(script.contains("_lmem()"));
    assert!(!f.config().exists());
    assert!(!f.db().exists());
}

#[test]
fn no_memory_editor_or_tui_is_added() {
    let f = Fixture::new();
    for command in ["edit", "tui", "ui", "docs"] {
        assert!(!f.run(&[command]).status.success());
    }
}

#[cfg(unix)]
#[test]
fn bash_completes_new_connections_and_setup_presets_at_runtime() {
    let f = Fixture::new();
    let bin_dir = f.root.path().join("bin");
    std::fs::create_dir(&bin_dir).unwrap();
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_lmem"), bin_dir.join("lmem")).unwrap();
    let script = f.root.path().join("completion.bash");
    std::fs::write(&script, f.run(&["completions", "bash"]).stdout).unwrap();
    // The registry is created after the script: connection suggestions are live.
    std::fs::create_dir(f.config_root()).unwrap();
    let registry = rusqlite::Connection::open(f.config_root().join("identities.db")).unwrap();
    registry.execute_batch("CREATE TABLE connections(id TEXT,context TEXT); INSERT INTO connections VALUES ('codex-new','{}'), ('cursor','{}');").unwrap();
    let shell = |words: &str, index: &str, current: &str, previous: &str| {
        let code = format!("source \"$1\"; COMP_WORDS=({words}); COMP_CWORD={index}; _lmem lmem {current} {previous}; printf '%s\\n' \"${{COMPREPLY[@]}}\"");
        let output = Command::new("bash")
            .args(["-c", &code, "_", script.to_str().unwrap()])
            .env("LIGHTMEM_CONFIG_DIR", f.config_root())
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    bin_dir.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    assert_eq!(
        shell("lmem --connection co", "2", "co", "--connection"),
        "codex-new\n"
    );
    assert_eq!(
        shell("lmem setup --preset d", "3", "d", "--preset"),
        "demo\ndeepseek\n"
    );
    assert_eq!(
        shell("lmem setup --preset dem", "3", "dem", "--preset"),
        "demo\n"
    );
    assert!(!f.config().exists());
    assert!(!f.db().exists());
}

#[test]
fn local_and_remote_commands_workflow() {
    let f = Fixture::new();
    f.demo();

    // Configure remote with lmem remote
    let out = f.json(&[
        "remote",
        "http://127.0.0.1:8080",
        "--api-key",
        "test-key",
        "--json",
    ]);
    assert_eq!(out["mode"], "remote");
    assert_eq!(out["remote_url"], "http://127.0.0.1:8080");
    assert_eq!(out["has_api_key"], true);

    // Verify config reflects remote
    let cfg = f.json(&["config", "--json"]);
    assert_eq!(cfg["remote_url"], "http://127.0.0.1:8080");

    // Revert to local using lmem local
    let out = f.json(&["local", "--json"]);
    assert_eq!(out["mode"], "local");
    assert_eq!(out["previous_remote_cleared"], true);

    // Verify config is back to local
    let cfg = f.json(&["config", "--json"]);
    assert!(cfg["remote_url"].is_null());
}

#[test]
fn ui_fixes_banner_no_color_narrow_width_and_groq_slice() {
    let f = Fixture::new();

    let ver = f.run(&["--version"]);
    assert!(ver.status.success());
    assert!(String::from_utf8_lossy(&ver.stdout).contains("0.12.1"));

    f.demo();
    let groq_cfg = f.run(&[
        "config",
        "--backend",
        "hash",
        "--answer-model",
        "groq:llama-3.3-70b-versatile",
        "--ollama-api-key",
        "gsk-dummy",
        "--json",
    ]);
    assert!(groq_cfg.status.success());

    let out = f.run(&[]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains('\x1b'),
        "NO_COLOR=1 must suppress ANSI escapes: {stdout}"
    );
    assert!(
        stdout.contains("groq (llama-3.3-70b-versatile)"),
        "expected full groq model name in banner: {stdout}"
    );
    assert!(!stdout.contains("groq (ama-3.3-70b-versatile)"));

    let narrow = f.command().env("COLUMNS", "80").output().unwrap();
    assert!(narrow.status.success());
    let narrow_out = String::from_utf8_lossy(&narrow.stdout);
    assert!(narrow_out.contains("L I G H T M E M"));
    for line in narrow_out.lines().take(12) {
        assert!(
            line.chars().count() <= 80,
            "narrow header line exceeded 80 cols ({}): {line}",
            line.chars().count()
        );
    }

    let custom_cfg = f.run(&[
        "config",
        "--answer-model",
        "custom:x",
        "--endpoint",
        "http://localhost:8000/v1",
        "--json",
    ]);
    assert!(custom_cfg.status.success());
    let custom_out = f.run(&[]);
    assert!(custom_out.status.success());
    assert!(String::from_utf8_lossy(&custom_out.stdout).contains("custom (x)"));
}

#[test]
fn ui_fixes_config_local_endpoint_and_json_errors() {
    let f = Fixture::new();
    let openai = f.run(&[
        "config",
        "--backend",
        "hash",
        "--preset",
        "openai",
        "--ollama-api-key",
        "none",
        "--json",
    ]);
    assert!(openai.status.success());

    let back_local = f.run(&[
        "config",
        "--backend",
        "hash",
        "--answer-model",
        "top1",
        "--json",
    ]);
    assert!(back_local.status.success());

    let cfg_out = f.run(&["config"]);
    assert!(cfg_out.status.success());
    let cfg_str = String::from_utf8_lossy(&cfg_out.stdout);
    assert!(
        !cfg_str.contains("https://api.openai.com"),
        "dormant cloud endpoint should not render for Top-1: {cfg_str}"
    );
    assert!(cfg_str.contains("local (offline, built-in)"));

    let err_out = f.run(&["config", "--preset", "nonexistent-preset", "--json"]);
    assert!(!err_out.status.success());
    let stderr_str = String::from_utf8_lossy(&err_out.stderr);
    let err_json: Value = serde_json::from_str(stderr_str.trim())
        .unwrap_or_else(|e| panic!("expected JSON error on stderr, got '{stderr_str}': {e}"));
    assert_eq!(err_json["status"], "error");
}

#[test]
fn ui_fixes_inspect_incoming_links_multiline_and_list_dedup() {
    let f = Fixture::new();
    f.demo();
    let a = f.json(&["remember", "Line one alpha\nLine two alpha", "--json"]);
    let b = f.json(&["remember", "Target summary line", "--json"]);
    let a_id = a["id"].as_str().unwrap();
    let b_id = b["id"].as_str().unwrap();

    assert!(f
        .run(&["link", a_id, b_id, "--relation", "depends_on", "--json"])
        .status
        .success());

    let insp_b = f.run(&["inspect", b_id]);
    assert!(insp_b.status.success());
    let insp_b_str = String::from_utf8_lossy(&insp_b.stdout);
    assert!(
        insp_b_str.contains("◀──depends_on──"),
        "incoming link arrow missing: {insp_b_str}"
    );
    assert!(
        insp_b_str.contains(&format!("╰─ ◀──depends_on── ({})", &a_id[..8])),
        "last link should close tree with ╰─: {insp_b_str}"
    );

    let insp_a = f.run(&["inspect", a_id]);
    assert!(insp_a.status.success());
    let insp_a_str = String::from_utf8_lossy(&insp_a.stdout);
    assert!(
        insp_a_str.contains("  Line one alpha\n  Line two alpha"),
        "multiline content lines should stay aligned: {insp_a_str}"
    );

    let list_out = f.run(&["list"]);
    assert!(list_out.status.success());
    let list_str = String::from_utf8_lossy(&list_out.stdout);
    assert!(
        !list_str.contains("Target summary line ──")
            && !list_str.contains("Target summary line ─ Target summary line"),
        "list should not repeat identical title and snippet: {list_str}"
    );
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for code in chars.by_ref() {
                if ('@'..='~').contains(&code) {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn ui_fixes_ansi_alignment_matches_no_color_and_list_shows_expired_badge() {
    let f = Fixture::new();
    f.demo();
    let active = f.json(&["remember", "Active architecture decision", "--json"]);
    let retired = f.json(&["remember", "Old retired note", "--json"]);
    let retired_id = retired["id"].as_str().unwrap();
    assert!(f.run(&["forget", retired_id, "--json"]).status.success());
    assert!(active["id"].is_string());

    for args in [
        vec![],
        vec!["config"],
        vec!["stats"],
        vec!["context"],
        vec!["doctor"],
        vec!["connect", "--list"],
        vec!["list", "--status", "all"],
    ] {
        let plain = f
            .command()
            .arg("--db")
            .arg(f.db())
            .args(&args)
            .output()
            .unwrap();
        assert!(plain.status.success(), "{args:?}");
        let colored = f
            .command()
            .env_remove("NO_COLOR")
            .env("CLICOLOR_FORCE", "1")
            .arg("--db")
            .arg(f.db())
            .args(&args)
            .output()
            .unwrap();
        assert!(colored.status.success(), "{args:?}");
        let colored_str = String::from_utf8_lossy(&colored.stdout);
        let plain_str = String::from_utf8_lossy(&plain.stdout);
        assert!(
            colored_str.contains('\x1b'),
            "expected ANSI output under CLICOLOR_FORCE for {args:?}"
        );
        assert_eq!(
            strip_ansi(&colored_str),
            plain_str,
            "visible column alignment diverged under color for {args:?}"
        );
    }

    let list_all = f.run(&["list", "--status", "all"]);
    assert!(list_all.status.success());
    let list_str = String::from_utf8_lossy(&list_all.stdout);
    assert!(
        list_str.contains("[expired]"),
        "expected [expired] badge in list --status all: {list_str}"
    );
    assert!(list_str.contains("○ [fact] Old retired note [expired]"));
    assert!(list_str.contains("▪ [fact] Active architecture decision"));
}
