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
    for command in ["edit", "tui", "ui"] {
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
