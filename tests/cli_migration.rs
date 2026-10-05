use lightmem::{MemoryRecord, MemoryType, Storage};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;
fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lmem"))
        .env("LIGHTMEM_CONFIG_DIR", root.join("config"))
        .env("LIGHTMEM_QA_DISABLE", "1")
        .env_remove("LIGHTMEM_DB")
        .arg("--db")
        .arg(root.join("memory.db"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}
#[test]
fn malformed_filters_fail_before_opening_database() {
    let dir = tempdir().unwrap();
    for args in [
        vec!["recall", "test", "--as-of", "not-a-date"],
        vec!["list", "--status", "typo"],
        vec!["remember", "test", "--type", "unknown"],
    ] {
        let out = cli(dir.path(), &args);
        assert!(!out.status.success());
        assert!(!dir.path().join("memory.db").exists());
    }
}
#[test]
fn noninteractive_model_change_requires_approval_and_failed_migration_keeps_config() {
    let dir = tempdir().unwrap();
    assert!(cli(dir.path(), &["config", "--backend", "hash"])
        .status
        .success());
    assert!(cli(dir.path(), &["remember", "Redis port 6379"])
        .status
        .success());
    let before = std::fs::read(dir.path().join("config/config.json")).unwrap();
    let out = cli(
        dir.path(),
        &[
            "config",
            "--backend",
            "ollama",
            "--url",
            "http://127.0.0.1:1",
        ],
    );
    assert!(!out.status.success());
    assert!(text(&out).contains("Migration needs approval"));
    assert_eq!(
        std::fs::read(dir.path().join("config/config.json")).unwrap(),
        before
    );
    let out = cli(
        dir.path(),
        &[
            "config",
            "--backend",
            "ollama",
            "--url",
            "http://127.0.0.1:1",
            "--yes",
        ],
    );
    assert!(!out.status.success());
    assert!(text(&out).contains("Embedding progress: 0/1"));
    assert_eq!(
        std::fs::read(dir.path().join("config/config.json")).unwrap(),
        before
    );
    assert!(cli(dir.path(), &["recall", "Redis", "--json"])
        .status
        .success());
}
#[test]
fn legacy_json_recall_does_not_prompt_and_explicit_reindex_reports_progress() {
    let dir = tempdir().unwrap();
    assert!(cli(dir.path(), &["config", "--backend", "hash"])
        .status
        .success());
    let storage = Storage::open(dir.path().join("memory.db")).unwrap();
    let m = MemoryRecord::new(
        MemoryType::Fact,
        "Redis".into(),
        "Redis port 6379".into(),
        vec![],
        0.9,
        None,
    );
    storage.insert_memory(&m, Some(&[1.0, 0.0])).unwrap();
    drop(storage);
    let out = cli(dir.path(), &["recall", "Redis", "--json"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("Migration needs approval"));
    assert!(out.stdout.is_empty());
    let out = cli(dir.path(), &["reindex", "--yes"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("Embedding progress: 0/1 (0%)"));
    assert!(text(&out).contains("Embedding progress: 1/1 (100%)"));
    assert!(text(&out).contains("Migration complete"));
    let out = cli(dir.path(), &["recall", "Redis", "--json"]);
    assert!(out.status.success());
    assert!(serde_json::from_slice::<serde_json::Value>(&out.stdout).is_ok());
}

#[test]
fn approved_model_change_commits_new_vectors_then_saves_config() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};
    let dir = tempdir().unwrap();
    assert!(cli(dir.path(), &["config", "--backend", "hash"])
        .status
        .success());
    assert!(cli(dir.path(), &["remember", "Redis port 6379"])
        .status
        .success());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = [0; 4096];
                    let _ = stream.read(&mut bytes).unwrap();
                    let body = r#"{"embedding":[1.0,0.0]}"#;
                    write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
                    break;
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(e) => panic!("Mock Ollama server did not receive request: {e}"),
            }
        }
    });
    let out = cli(
        dir.path(),
        &[
            "config",
            "--backend",
            "ollama",
            "--url",
            &url,
            "--model",
            "fixture-model",
            "--yes",
        ],
    );
    server.join().unwrap();
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("Migration complete: 1 embeddings committed"));
    let cfg: lightmem::LightMemConfig =
        serde_json::from_slice(&std::fs::read(dir.path().join("config/config.json")).unwrap())
            .unwrap();
    assert_eq!(cfg.backend, "ollama");
    let storage = Storage::open(dir.path().join("memory.db")).unwrap();
    assert_eq!(
        storage.embedding_identity().unwrap(),
        Some(cfg.embedding_identity().unwrap())
    );
    assert_eq!(
        storage.get_candidate_vectors(None, None, None).unwrap()[0].1,
        vec![1.0, 0.0]
    );
}
