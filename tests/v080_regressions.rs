use chrono::{DateTime, Utc};
use lightmem::{
    AttachmentPayload, LightMem, LightMemConfig, MemoryStatus, RememberRequest, RemoteClient,
};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::{tempdir, TempDir};

const KEY: &str = "disposable-review-test-key";
fn hash_config() -> LightMemConfig {
    LightMemConfig {
        backend: "hash".into(),
        reranker: "top1".into(),
        ..Default::default()
    }
}
struct Fixture {
    _dir: TempDir,
    controller: Arc<LightMem>,
    client: RemoteClient,
}
fn fixture() -> Fixture {
    let dir = tempdir().unwrap();
    let controller =
        Arc::new(LightMem::open_at(&dir.path().join("test.db"), hash_config()).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client = RemoteClient::new(
        format!("http://{}", listener.local_addr().unwrap()),
        Some(KEY.into()),
    );
    let copy = Arc::clone(&controller);
    std::thread::spawn(move || {
        lightmem::services::api_server::serve_loop(listener, copy, Some(KEY.into())).unwrap()
    });
    Fixture {
        _dir: dir,
        controller,
        client,
    }
}
fn request(base: &str, path: &str, body: Option<&[u8]>) -> (u16, Value) {
    let req = if body.is_some() {
        ureq::post(&format!("{base}{path}"))
    } else {
        ureq::get(&format!("{base}{path}"))
    }
    .set("Authorization", &format!("Bearer {KEY}"))
    .set("Content-Type", "application/json")
    .timeout(Duration::from_secs(5));
    let response = match body {
        Some(bytes) => req.send_bytes(bytes),
        None => req.call(),
    };
    let response = match response {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => panic!("{e}"),
    };
    (response.status(), response.into_json().unwrap())
}
fn post(base: &str, path: &str, body: Value) -> (u16, Value) {
    request(base, path, Some(&serde_json::to_vec(&body).unwrap()))
}
fn remember(client: &RemoteClient, title: &str) -> lightmem::MemoryRecord {
    client
        .remember(&RememberRequest {
            content: Some(format!("{title}: service uses port 8180")),
            title: Some(title.into()),
            ..Default::default()
        })
        .unwrap()
}

#[test]
fn malformed_clear_never_deletes_data_and_explicit_archive_is_recoverable() {
    let f = fixture();
    let m = remember(&f.client, "Disposable clear test");
    for body in [
        b"".as_slice(),
        b"{invalid",
        b"{}",
        b"[]",
        b"[false]",
        b"{\"sof\":false}",
        b"{\"soft\":\"false\"}",
    ] {
        assert_eq!(request(&f.client.base_url, "/v1/clear", Some(body)).0, 400);
        assert_eq!(f.controller.stats().unwrap().total_memories, 1);
    }
    assert_eq!(
        post(&f.client.base_url, "/v1/clear", json!({"soft":true})).0,
        200
    );
    assert_eq!(
        f.controller.get(&m.id).unwrap().unwrap().status,
        MemoryStatus::Expired
    );
    assert_eq!(
        post(&f.client.base_url, "/v1/clear", json!({"soft":false})).0,
        200
    );
    assert_eq!(f.controller.stats().unwrap().total_memories, 0);
}

#[test]
fn explicit_blank_key_is_rejected_and_health_does_not_disclose_vault() {
    let f = fixture();
    let health = f.client.health().unwrap();
    assert!(health.get("vault").is_none());
    let error = lightmem::serve_api(
        Arc::clone(&f.controller),
        lightmem::ServerOptions {
            host: "127.0.0.1".into(),
            port: 0,
            api_key: Some("   ".into()),
            no_auth: false,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot be blank"));
    assert!(RemoteClient::new(&f.client.base_url, None)
        .stats()
        .unwrap_err()
        .to_string()
        .contains("401"));
}

#[test]
fn unicode_reserved_characters_and_timezone_filters_round_trip() {
    let f = fixture();
    remember(&f.client, "Audit A");
    for title in ["Audit A&B", "Audit café", "Audit + # % 東京"] {
        let m = remember(&f.client, title);
        assert_eq!(f.client.get(title).unwrap().unwrap().id, m.id);
    }
    let cutoff = DateTime::parse_from_rfc3339("2000-01-01T00:00:00+05:30")
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(
        f.client
            .list(10, 0, None, None, None, None, None, Some(cutoff))
            .unwrap()
            .total,
        0
    );
    for query in [
        "as_of=invalid",
        "date=bad",
        "category=unknown",
        "status=typo",
        "limit=bad",
        "page=0",
        "page=18446744073709551615&limit=20",
        "media=wrong",
        "limit=1&limit=2",
        "as_of=%ZZ",
    ] {
        assert_eq!(
            request(&f.client.base_url, &format!("/v1/list?{query}"), None).0,
            400,
            "{query}"
        );
    }
}

#[test]
fn media_filter_applies_before_count_and_pagination() {
    let f = fixture();
    remember(&f.client, "Text only");
    for n in 0..3 {
        f.client
            .remember(&RememberRequest {
                content: Some(format!("Image {n}")),
                attachments: Some(vec![AttachmentPayload {
                    name: format!("image{n}.png"),
                    mime: None,
                    data_base64: lightmem::encode_base64(b"\x89PNG\r\n\x1a\n"),
                }]),
                ..Default::default()
            })
            .unwrap();
    }
    let one = f
        .client
        .list(1, 0, Some(1), None, None, Some("image".into()), None, None)
        .unwrap();
    assert_eq!(one.total, 3);
    assert_eq!(one.total_pages, 3);
    assert!(one.has_more);
    assert_eq!(one.items[0].attachments.len(), 1);
    let last = f
        .client
        .list(1, 0, Some(3), None, None, Some("image".into()), None, None)
        .unwrap();
    assert!(!last.has_more);
    assert_ne!(one.items[0].id, last.items[0].id);
    assert_eq!(
        f.client
            .list(10, 0, None, None, None, Some("audio".into()), None, None)
            .unwrap()
            .total,
        0
    );
}

#[test]
fn remote_confidence_subsecond_ttl_and_inspection_links_are_preserved() {
    let f = fixture();
    let a = remember(&f.client, "Related A");
    let b = f
        .client
        .remember(&RememberRequest {
            content: Some("Related B".into()),
            confidence: Some(0.42),
            ttl: Some("500ms".into()),
            ..Default::default()
        })
        .unwrap();
    assert!((b.confidence - 0.42).abs() < 0.001);
    let duration = b.expired_at.unwrap() - b.created_at;
    assert!(duration.num_milliseconds() >= 499 && duration.num_milliseconds() <= 501);
    f.client.link(&a.id, &b.id, None, None).unwrap();
    assert_eq!(f.client.links(&a.id).unwrap().len(), 1);
    assert_eq!(
        post(
            &f.client.base_url,
            "/v1/remember",
            json!({"content":"invalid confidence","confidence":1.5})
        )
        .0,
        400
    );
    assert_eq!(
        post(
            &f.client.base_url,
            "/v1/recall",
            json!({"query":"Related","precision":true})
        )
        .0,
        400
    );
}

#[test]
fn request_limits_authentication_and_browser_origin_are_enforced() {
    let f = fixture();
    let address = f.client.base_url.strip_prefix("http://").unwrap();
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write!(
        stream,
        "POST /v1/remember HTTP/1.1\r\nHost: {address}\r\nContent-Length: 60000000\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 401"));
    let result = ureq::post(&format!("{}/v1/remember", f.client.base_url))
        .set("Authorization", &format!("Bearer {KEY}"))
        .set("Origin", "https://unrelated.example")
        .send_json(json!({"content":"must not save"}));
    assert!(matches!(result, Err(ureq::Error::Status(403, _))));
    assert_eq!(f.controller.stats().unwrap().total_memories, 0);
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    // The server rejects headers > 8192 bytes with 400 and closes the connection immediately,
    // so writing 9000+ bytes may legitimately hit BrokenPipe / ConnectionReset before completion.
    if let Err(err) = write!(
        stream,
        "GET /v1/stats HTTP/1.1\r\nHost: {address}\r\nX-Big: {}\r\n\r\n",
        "x".repeat(9000)
    ) {
        assert!(
            matches!(
                err.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ),
            "Unexpected write error for oversized headers: {err}"
        );
    }
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    assert!(response.starts_with("HTTP/1.1 400"));
}

#[test]
fn default_and_local_preset_do_not_attempt_llm_entity_requests() {
    let dir = tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut cfg = hash_config();
    cfg.ollama_url = format!("http://{}", listener.local_addr().unwrap());
    cfg.apply_preset("local").unwrap();
    assert_eq!(cfg.entity_extraction, "off");
    cfg.backend = "hash".into();
    let lm = LightMem::open_at(&dir.path().join("local.db"), cfg).unwrap();
    let (_, _, warning) = lm
        .remember_with_attachments_and_entities(
            Some("Synthetic private decision"),
            &[],
            false,
            true,
            None,
            None,
            None,
            vec![],
            None,
            None,
            false,
            None,
        )
        .unwrap();
    assert!(warning.is_none());
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn restored_memory_keeps_its_id_entities_and_attachments() {
    let f = fixture();
    let m = f
        .client
        .remember(&RememberRequest {
            content: Some("Restorable attachment".into()),
            attachments: Some(vec![AttachmentPayload {
                name: "note.txt".into(),
                mime: None,
                data_base64: lightmem::encode_base64(b"preserved bytes"),
            }]),
            ..Default::default()
        })
        .unwrap();
    f.controller.forget(&m.id, false).unwrap();
    let restored = f.controller.restore_memory(&m.id).unwrap();
    assert_eq!(restored.id, m.id);
    assert_eq!(restored.status, MemoryStatus::Active);
    assert!(restored.expired_at.is_none());
    assert_eq!(restored.attachments.len(), 1);
    assert_eq!(
        f.controller.get_attachments_data(&m.id).unwrap()[0].1,
        b"preserved bytes"
    );
    assert!(f
        .controller
        .recall("Restorable attachment", None, None, 5, None)
        .unwrap()
        .iter()
        .any(|r| r.memory.id == m.id));
}

fn cli(root: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lmem"));
    isolated(&mut cmd, root);
    cmd.args(args).stdin(Stdio::null()).output().unwrap()
}
fn isolated(cmd: &mut Command, root: &Path) {
    cmd.env("LIGHTMEM_CONFIG_DIR", root)
        .env("LIGHTMEM_QA_DISABLE", "1");
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
}
fn write_config(root: &Path) {
    std::fs::write(
        root.join("config.json"),
        serde_json::to_vec(&hash_config()).unwrap(),
    )
    .unwrap();
}
struct ProcessServer {
    child: Child,
    root: TempDir,
    base: String,
}
impl Drop for ProcessServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn process_server() -> ProcessServer {
    let root = tempdir().unwrap();
    write_config(root.path());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lmem"));
    isolated(&mut cmd, root.path());
    let child = cmd
        .args([
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--api-key",
            KEY,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let server = ProcessServer {
        child,
        root,
        base: format!("http://127.0.0.1:{port}"),
    };
    let deadline = Instant::now() + Duration::from_secs(8);
    while ureq::get(&format!("{}/health", server.base))
        .timeout(Duration::from_millis(100))
        .call()
        .is_err()
    {
        assert!(Instant::now() < deadline, "Server failed to start");
        std::thread::sleep(Duration::from_millis(20));
    }
    server
}
fn wait_job(base: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let (status, value) = request(base, "/v1/workspace/status", None);
        assert_eq!(status, 200);
        if value["busy"] == false {
            return value;
        }
        assert!(Instant::now() < deadline, "Job timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn remote_unsupported_commands_and_options_never_fall_back_locally() {
    let dir = tempdir().unwrap();
    write_config(dir.path());
    let out_path = dir.path().join("must-not-exist.json");
    let out = cli(
        dir.path(),
        &[
            "--remote",
            "http://127.0.0.1:9",
            "export",
            "--json",
            "-o",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(!out.status.success());
    assert!(!out_path.exists());
    assert!(!dir.path().join("memories.db").exists());
    for args in [
        vec!["recall", "test", "--path"],
        vec!["recall", "--image", "does-not-exist.png"],
        vec!["remember", "test", "--extract"],
        vec!["inspect", "test", "--open"],
        vec!["import", "missing.json", "--enrich"],
    ] {
        let mut full = vec!["--remote", "http://127.0.0.1:9"];
        full.extend(args);
        let out = cli(dir.path(), &full);
        assert!(!out.status.success());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("Failed to connect"));
    }
    assert!(!dir.path().join("memories.db").exists());
}

#[test]
fn config_output_redacts_both_secrets_and_enrichment_fails_before_writing() {
    let dir = tempdir().unwrap();
    let mut cfg = hash_config();
    cfg.api_key = Some("synthetic-remote-secret".into());
    cfg.ollama_api_key = Some("synthetic-ollama-secret".into());
    std::fs::write(
        dir.path().join("config.json"),
        serde_json::to_vec(&cfg).unwrap(),
    )
    .unwrap();
    let out = cli(dir.path(), &["config", "--json"]);
    assert!(out.status.success());
    let output = String::from_utf8_lossy(&out.stdout);
    assert!(!output.contains("synthetic-"));
    let value: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(value["api_key"], "[REDACTED]");
    assert_eq!(value["ollama_api_key"], "[REDACTED]");
    let input = dir.path().join("input.json");
    std::fs::write(&input, r#"[{"content":"must not import"}]"#).unwrap();
    let out = cli(dir.path(), &["import", input.to_str().unwrap(), "--enrich"]);
    assert!(!out.status.success());
    assert!(!dir.path().join("memories.db").exists());
}

#[test]
fn remote_precision_cli_returns_an_answer_using_the_server_configuration() {
    let server = process_server();
    let client = RemoteClient::new(&server.base, Some(KEY.into()));
    remember(&client, "Auth port");
    let out = cli(
        server.root.path(),
        &[
            "--remote",
            &server.base,
            "--api-key",
            KEY,
            "recall",
            "Auth port",
            "--precision",
            "--json",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(value["answer"].is_string());
    assert_eq!(value["reranker_used"], "top1");
}

#[test]
fn workspace_config_preview_job_and_model_test_preserve_vault_binding() {
    let server = process_server();
    let client = RemoteClient::new(&server.base, Some(KEY.into()));
    let m = remember(&client, "Config review");
    let (status, preview) = post(
        &server.base,
        "/v1/workspace/config/preview",
        json!({"backend":"onnx","onnx_model":"bge-small"}),
    );
    assert_eq!(status, 200);
    assert_eq!(preview["needs_reindex"], true);
    let before = std::fs::read(server.root.path().join("config.json")).unwrap();
    assert_eq!(
        post(
            &server.base,
            "/v1/workspace/config",
            json!({"backend":"onnx","onnx_model":"bge-small"})
        )
        .0,
        400
    );
    assert_eq!(
        std::fs::read(server.root.path().join("config.json")).unwrap(),
        before
    );
    assert_eq!(
        post(
            &server.base,
            "/v1/workspace/config",
            json!({"entity_extraction":"off","reranker":"top1"})
        )
        .0,
        200
    );
    let status = wait_job(&server.base);
    assert_eq!(status["job"]["status"], "complete");
    assert_eq!(status["stats"]["total_memories"], 1);
    assert_eq!(client.get(&m.id).unwrap().unwrap().id, m.id);
    let (status, test) = post(&server.base, "/v1/workspace/test", json!({}));
    assert_eq!(status, 200);
    assert_eq!(test["ok"], true);
    assert_eq!(client.stats().unwrap().total_memories, 1);
}

#[test]
fn workspace_restore_requires_matching_preview_destination_and_unchanged_vault() {
    let server = process_server();
    let client = RemoteClient::new(&server.base, Some(KEY.into()));
    remember(&client, "Before backup");
    let bytes = ureq::post(&format!("{}/v1/workspace/backup", server.base))
        .set("Authorization", &format!("Bearer {KEY}"))
        .send_json(json!({}))
        .unwrap()
        .into_reader();
    let mut archive = Vec::new();
    bytes.take(10_000_000).read_to_end(&mut archive).unwrap();
    let (_, preview) = post(
        &server.base,
        "/v1/workspace/restore/preview",
        json!({"data_base64":lightmem::encode_base64(&archive)}),
    );
    assert_eq!(preview["manifest"]["stats"]["total_memories"], 1);
    assert_eq!(post(&server.base,"/v1/workspace/restore",json!({"preview_id":preview["preview_id"],"mode":"replace","confirm_vault":"wrong-vault"})).0,400);
    remember(&client, "After preview");
    assert_eq!(post(&server.base,"/v1/workspace/restore",json!({"preview_id":preview["preview_id"],"mode":"replace","confirm_vault":preview["destination"]})).0,400);
    assert_eq!(client.stats().unwrap().total_memories, 2);
    let (_, preview) = post(
        &server.base,
        "/v1/workspace/restore/preview",
        json!({"data_base64":lightmem::encode_base64(&archive)}),
    );
    assert_eq!(post(&server.base,"/v1/workspace/restore",json!({"preview_id":preview["preview_id"],"mode":"replace","confirm_vault":preview["destination"]})).0,200);
    let state = wait_job(&server.base);
    assert_eq!(state["job"]["status"], "complete", "{state}");
    assert_eq!(client.stats().unwrap().total_memories, 1);
}

#[test]
fn named_connections_are_rejected_in_remote_mode_and_daemon_scope_is_forwarded() {
    let root = tempdir().unwrap();
    write_config(root.path());
    for args in [
        vec!["identity", "user-add", "reviewuser"],
        vec![
            "identity",
            "agent-add",
            "reviewagent",
            "--user",
            "reviewuser",
        ],
        vec![
            "identity",
            "connection-add",
            "reviewconn",
            "--user",
            "reviewuser",
            "--agent",
            "reviewagent",
            "--harness",
            "codex",
            "--memory-scope",
            "user",
        ],
    ] {
        assert!(cli(root.path(), &args).status.success());
    }
    let out = cli(
        root.path(),
        &[
            "--connection",
            "reviewconn",
            "--remote",
            "http://127.0.0.1:9",
            "remember",
            "must not store",
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not support local connections"));
    let expected: Value = serde_json::from_slice(
        &cli(
            root.path(),
            &[
                "--connection",
                "reviewconn",
                "--scope",
                "agent",
                "context",
                "--json",
            ],
        )
        .stdout,
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let out = cli(
        root.path(),
        &[
            "--connection",
            "reviewconn",
            "--scope",
            "agent",
            "--session",
            "review-session",
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--api-key",
            KEY,
            "-d",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let base = format!("http://127.0.0.1:{port}");
    let result = request(&base, "/v1/workspace/status", None);
    let stopped = cli(root.path(), &["serve", "--stop"]);
    assert!(stopped.status.success());
    assert_eq!(result.0, 200);
    assert_eq!(result.1["db_path"], expected["db_path"]);
    assert_eq!(result.1["context"]["session_id"], "review-session");
}

#[test]
fn stale_workspace_tabs_cannot_write_to_a_new_vault_and_config_changes_preserve_attribution() {
    let server = process_server();
    let (_, initial) = request(&server.base, "/v1/workspace/status", None);
    let encoded: String = initial["db_path"]
        .as_str()
        .unwrap()
        .bytes()
        .map(|b| format!("%{b:02X}"))
        .collect();
    assert_eq!(post(&server.base, "/v1/workspace/connections", json!({"id":"review", "user":"reviewer", "agent":"coding", "harness":"codex", "project":"demo", "scope":"project"})).0, 200);
    assert_eq!(
        post(
            &server.base,
            "/v1/workspace/vault",
            json!({"connection":"review"})
        )
        .0,
        200
    );
    let attempted = ureq::post(&format!("{}/v1/remember", server.base))
        .set("Authorization", &format!("Bearer {KEY}"))
        .set("X-LightMem-Vault", &encoded)
        .send_json(json!({"content":"must not land in another vault"}));
    assert!(matches!(attempted, Err(ureq::Error::Status(409, _))));
    let client = RemoteClient::new(&server.base, Some(KEY.into()));
    assert_eq!(client.stats().unwrap().total_memories, 0);
    assert_eq!(
        post(
            &server.base,
            "/v1/workspace/config",
            json!({"reranker":"top1","entity_extraction":"off"})
        )
        .0,
        200
    );
    let state = wait_job(&server.base);
    assert_eq!(state["job"]["status"], "complete");
    let saved = remember(&client, "Attributed after configuration");
    assert_eq!(saved.context.unwrap().connection_id, "review");
    assert_eq!(
        post(
            &server.base,
            "/v1/workspace/vault",
            json!({"connection":null})
        )
        .0,
        200
    );
    assert_eq!(client.stats().unwrap().total_memories, 0);
}

#[test]
fn workspace_refuses_to_overwrite_configuration_changed_outside_its_session() {
    let server = process_server();
    let mut changed = hash_config();
    changed.entity_extraction = "llm".into();
    let bytes = serde_json::to_vec(&changed).unwrap();
    std::fs::write(server.root.path().join("config.json"), &bytes).unwrap();
    let (status, error) = post(
        &server.base,
        "/v1/workspace/config/preview",
        json!({"entity_extraction":"off"}),
    );
    assert_eq!(status, 400);
    assert!(error["error"].as_str().unwrap().contains("changed outside"));
    assert_eq!(
        std::fs::read(server.root.path().join("config.json")).unwrap(),
        bytes
    );
}
