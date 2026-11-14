use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::{tempdir, TempDir};

struct CliFixture {
    root: TempDir,
}
impl CliFixture {
    fn new() -> Self {
        Self {
            root: tempdir().unwrap(),
        }
    }
    fn db(&self) -> std::path::PathBuf {
        self.root.path().join("vault.db")
    }
    fn config(&self) -> std::path::PathBuf {
        self.root.path().join("config/config.json")
    }
    fn run(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lmem"));
        for key in [
            "LIGHTMEM_CONNECTION",
            "LIGHTMEM_DB",
            "LIGHTMEM_REMOTE_URL",
            "LIGHTMEM_URL",
            "LIGHTMEM_API_KEY",
            "LIGHTMEM_KEY",
            "OLLAMA_API_KEY",
            "LMEM_OLLAMA_API_KEY",
            "LIGHTMEM_BINARY_PATH",
            "LMEM_BINARY_PATH",
        ] {
            cmd.env_remove(key);
        }
        cmd.env("LIGHTMEM_CONFIG_DIR", self.root.path().join("config"))
            .env("LIGHTMEM_OFFLINE", "1")
            .env("NO_COLOR", "1")
            .current_dir(self.root.path())
            .stdin(Stdio::null())
            .arg("--db")
            .arg(self.db())
            .args(args)
            .output()
            .unwrap()
    }
    fn json(&self, args: &[&str]) -> Value {
        let result = self.run(args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    }
    fn demo(&self) {
        self.json(&["setup", "--preset", "demo", "--yes", "--json"]);
    }
}
fn bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[test]
fn dry_run_does_not_create_vault_config_or_download_models() {
    let f = CliFixture::new();
    let plan = f.json(&["setup", "--preset", "demo", "--dry-run", "--json"]);
    assert_eq!(plan["configuration"]["backend"], "hash");
    assert_eq!(plan["configuration"]["reranker"], "top1");
    assert_eq!(plan["configuration"]["entity_extraction"], "off");
    assert_eq!(plan["vault_exists"], false);
    assert!(!f.db().exists());
    assert!(!f.config().exists());
    assert!(!f.root.path().join("config/models").exists());
    f.demo();
    assert!(f.config().exists());
    assert!(!f.db().exists());
}

#[test]
fn preview_reports_real_index_migration_and_apply_requires_approval() {
    let f = CliFixture::new();
    f.demo();
    f.json(&["remember", "Auth uses port 8180", "--json"]);
    let cfg_before = bytes(&f.config());
    let db_before = bytes(&f.db());
    let plan = f.json(&["config", "--preset", "local", "--dry-run", "--json"]);
    assert_eq!(plan["total_memories"], 1);
    assert_eq!(plan["migration_required"], true);
    assert!(plan["stored_embedding_identity"]
        .as_str()
        .unwrap()
        .contains("hash"));
    assert_eq!(bytes(&f.config()), cfg_before);
    assert_eq!(bytes(&f.db()), db_before);
    let rejected = f.run(&["config", "--preset", "local", "--json"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Migration needs approval"));
    assert_eq!(bytes(&f.config()), cfg_before);
    assert_eq!(f.json(&["stats", "--json"])["active_memories"], 1);
}

#[test]
fn noninteractive_setup_and_invalid_reset_preserve_configuration_and_memories() {
    let f = CliFixture::new();
    f.demo();
    f.json(&["remember", "Disposable decision", "--json"]);
    let before = bytes(&f.config());
    for args in [
        vec!["setup", "--yes"],
        vec!["setup", "--preset", "local"],
        vec!["setup", "--model", "typo", "--yes"],
        vec!["config", "--reset-db"],
        vec!["config", "--reset-db", "--yes", "--backend", "bad"],
        vec!["config", "--backend", "bad"],
        vec!["config", "--reranker", "onnx:unknown"],
        vec!["config", "--dry-run", "--download", "all"],
    ] {
        let output = f.run(&args);
        assert!(!output.status.success(), "unexpected success: {args:?}");
        assert_eq!(bytes(&f.config()), before, "{args:?}");
        assert_eq!(
            f.json(&["stats", "--json"])["active_memories"],
            1,
            "{args:?}"
        );
    }
}

#[test]
fn provider_test_does_not_create_selected_vault_or_change_existing_records() {
    let f = CliFixture::new();
    f.demo();
    let result = f.json(&["config", "--test", "--json"]);
    assert_eq!(result["dimensions"], 384);
    assert_eq!(result["memory_saved"], false);
    assert!(!f.db().exists());
    f.json(&["remember", "Auth port 8180", "--json"]);
    let before = bytes(&f.db());
    f.json(&["config", "--test", "--json"]);
    assert_eq!(bytes(&f.db()), before);
    assert_eq!(f.json(&["stats", "--json"])["total_memories"], 1);
}

#[test]
fn secrets_are_configurable_and_remain_redacted_in_view_and_preview() {
    let f = CliFixture::new();
    f.demo();
    let output = f.run(&[
        "config",
        "--ollama-api-key",
        "synthetic-llm-secret",
        "--api-key",
        "synthetic-remote-secret",
        "--json",
    ]);
    assert!(output.status.success());
    let result = String::from_utf8(output.stdout).unwrap();
    assert!(!result.contains("synthetic-"));
    let cfg: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(cfg["ollama_api_key_configured"], true);
    assert_eq!(cfg["api_key_configured"], true);
    let preview = f.json(&[
        "config",
        "--ollama-api-key",
        "none",
        "--clear-remote",
        "--dry-run",
        "--json",
    ]);
    assert!(preview["changed_fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "ollama_api_key"));
    assert!(preview["changed_fields"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "api_key"));
    assert_eq!(
        f.json(&["config", "--json"])["ollama_api_key_configured"],
        true
    );
    assert!(!f.db().exists());
}

#[test]
fn cli_restores_same_archived_record_and_file_and_rejects_remote_restore() {
    let f = CliFixture::new();
    f.demo();
    let attachment = f.root.path().join("sample.txt");
    std::fs::write(&attachment, "verified bytes").unwrap();
    let memory = f.json(&[
        "remember",
        "Auth uses 8180",
        "--title",
        "Auth café & API",
        "--ttl",
        "1h",
        "--file",
        attachment.to_str().unwrap(),
        "--json",
    ]);
    let id = memory["id"].as_str().unwrap();
    f.run(&["forget", id]);
    let restored = f.json(&["restore", "Auth café & API", "--json"]);
    assert_eq!(restored["memory"]["id"], id);
    assert_eq!(restored["memory"]["status"], "active");
    assert!(restored["memory"]["expired_at"].is_null());
    assert_eq!(
        restored["memory"]["attachments"].as_array().unwrap().len(),
        1
    );
    let dest = f.root.path().join("download.txt");
    f.json(&["inspect", id, "--save-to", dest.to_str().unwrap(), "--json"]);
    assert_eq!(bytes(&dest), bytes(&attachment));
    let out = Command::new(env!("CARGO_BIN_EXE_lmem"))
        .env("LIGHTMEM_CONFIG_DIR", f.root.path().join("config"))
        .env_remove("LIGHTMEM_OFFLINE")
        .env_remove("LIGHTMEM_DB")
        .env_remove("LIGHTMEM_CONNECTION")
        .args(["--remote", "http://127.0.0.1:9", "restore", id, "--json"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not supported in remote mode"));
    assert_eq!(f.json(&["stats", "--json"])["active_memories"], 1);
}

#[test]
fn ambiguous_archived_titles_do_not_restore_an_arbitrary_record() {
    let f = CliFixture::new();
    f.demo();
    let a = f.json(&[
        "remember",
        "First unique decision",
        "--title",
        "Repeated title",
        "--json",
    ]);
    let a = a["id"].as_str().unwrap();
    assert!(f.run(&["forget", a]).status.success());
    let b = f.json(&[
        "remember",
        "Second different decision",
        "--title",
        "Repeated title",
        "--json",
    ]);
    let b = b["id"].as_str().unwrap();
    assert!(f.run(&["forget", b]).status.success());
    let rejected = f.run(&["restore", "Repeated title", "--json"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Multiple memories"));
    assert_eq!(f.json(&["stats", "--json"])["active_memories"], 0);
    assert_eq!(f.json(&["restore", a, "--json"])["memory"]["id"], a);
    assert_eq!(f.json(&["stats", "--json"])["active_memories"], 1);
}
