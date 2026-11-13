use lightmem::{
    LightMem, LightMemConfig, MemoryType, RecallRequest, RememberRequest, RemoteClient,
};
use std::net::TcpListener;
use std::sync::Arc;
use tempfile::tempdir;

#[test]
fn test_api_server_and_remote_client_full_lifecycle() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("remote_test.db");

    let cfg = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };

    let lm = Arc::new(LightMem::open_at(&db_path, cfg).unwrap());

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let local_addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", local_addr);
    let api_key = "test_secret_key_456".to_string();

    let server_lm = Arc::clone(&lm);
    let server_key = api_key.clone();
    std::thread::spawn(move || {
        let _ = lightmem::services::api_server::serve_loop(listener, server_lm, Some(server_key));
    });

    // 1. Health check works without auth
    let unauthed_client = RemoteClient::new(&base_url, None);
    let health = unauthed_client.health().unwrap();
    assert_eq!(health.get("status").and_then(|v| v.as_str()), Some("ok"));

    // 2. Unauthenticated remember must fail with 401
    let req = RememberRequest {
        content: Some("Unauthorized memory attempt".to_string()),
        ..Default::default()
    };
    let err = unauthed_client.remember(&req);
    assert!(err.is_err(), "Unauthenticated request should fail");
    let err_str = err.unwrap_err().to_string();
    assert!(
        err_str.contains("401") || err_str.contains("Unauthorized"),
        "Error was: {}",
        err_str
    );

    // 3. Authenticated client
    let client = RemoteClient::new(&base_url, Some(api_key));

    // Store first memory
    let req1 = RememberRequest {
        content: Some("LightMem server runs seamlessly on Linux, macOS, and Windows".to_string()),
        category: Some(MemoryType::Fact),
        tags: Some(vec!["server".to_string(), "cross-platform".to_string()]),
        ..Default::default()
    };
    let mem1 = client.remember(&req1).unwrap();
    assert_eq!(mem1.category, MemoryType::Fact);
    assert!(!mem1.id.is_empty());

    // Store second memory
    let req2 = RememberRequest {
        content: Some(
            "Remote clients connect to LightMem server via Bearer token API key".to_string(),
        ),
        category: Some(MemoryType::Decision),
        tags: Some(vec!["auth".to_string(), "server".to_string()]),
        ..Default::default()
    };
    let mem2 = client.remember(&req2).unwrap();

    // Store third memory with attachment
    let fake_png_bytes = b"\x89PNG\r\n\x1a\nfake_image_bytes";
    let b64 = lightmem::encode_base64(fake_png_bytes);
    let req3 = RememberRequest {
        content: Some("System architecture diagram".to_string()),
        category: Some(MemoryType::Artifact),
        attachments: Some(vec![lightmem::AttachmentPayload {
            name: "diagram.png".to_string(),
            mime: Some("image/png".to_string()),
            data_base64: b64,
        }]),
        ..Default::default()
    };
    let mem3 = client.remember(&req3).unwrap();
    assert_eq!(mem3.attachments.len(), 1);
    assert_eq!(mem3.attachments[0].filename, "diagram.png");

    // Check stats
    let stats = client.stats().unwrap();
    assert_eq!(stats.active_memories, 3);

    // Recall
    let recall_req = RecallRequest {
        query: "Linux macOS Windows".to_string(),
        ..Default::default()
    };
    let hits = client.recall(&recall_req).unwrap();
    println!("Recall hits: {:?}", hits);
    assert!(!hits.is_empty());
    assert!(hits.iter().any(|h| h.memory.id == mem1.id));

    // Get
    let fetched = client.get(&mem1.id).unwrap();
    assert!(fetched.is_some());
    assert_eq!(fetched.unwrap().id, mem1.id);

    // List
    let listed = client
        .list(10, 0, None, None, None, None, None, None)
        .unwrap();
    assert_eq!(listed.total, 3);

    // Link
    let link = client
        .link(&mem1.id, &mem2.id, Some("secures"), Some(1.5))
        .unwrap();
    assert_eq!(link.relation, "secures");
    assert_eq!(link.weight, 1.5);

    // Related
    let related = client.related(&mem1.id, 2).unwrap();
    assert!(!related.is_empty());

    // Graph
    let graph = client.graph(Some(&mem1.id), Some(2)).unwrap();
    assert!(!graph.nodes.is_empty());
    assert!(!graph.edges.is_empty());

    // Forget
    let forgot = client.forget(Some(&mem1.id), false, false).unwrap();
    assert!(forgot);

    let stats_after = client.stats().unwrap();
    assert_eq!(stats_after.active_memories, 2);
    assert_eq!(stats_after.expired_memories, 1);

    // Clear
    let cleared = client.clear(false).unwrap();
    assert!(cleared >= 1);
}

#[test]
fn test_cli_remote_binary_e2e() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("cli_remote.db");

    let cfg = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };

    let lm = Arc::new(LightMem::open_at(&db_path, cfg).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let local_addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", local_addr);
    let api_key = "secret_agent_key_999".to_string();

    let server_lm = Arc::clone(&lm);
    let server_key = api_key.clone();
    std::thread::spawn(move || {
        let _ = lightmem::services::api_server::serve_loop(listener, server_lm, Some(server_key));
    });

    let exe = env!("CARGO_BIN_EXE_lmem");

    // 1. Remember via CLI using --remote and --api-key flags
    let output = std::process::Command::new(exe)
        .args([
            "--remote",
            &base_url,
            "--api-key",
            &api_key,
            "remember",
            "Deployed microservice on port 9090",
            "--type",
            "fact",
            "--json",
        ])
        .output()
        .expect("Failed to execute lmem remember");
    assert!(
        output.status.success(),
        "lmem remember failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let remembered: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(remembered["content"], "Deployed microservice on port 9090");

    // 2. Recall via CLI using environment variables LIGHTMEM_REMOTE_URL and LIGHTMEM_API_KEY
    let output = std::process::Command::new(exe)
        .env("LIGHTMEM_REMOTE_URL", &base_url)
        .env("LIGHTMEM_API_KEY", &api_key)
        .args(["recall", "microservice port", "--json"])
        .output()
        .expect("Failed to execute lmem recall");
    assert!(
        output.status.success(),
        "lmem recall failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let hits: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!hits.is_empty());
    assert!(hits[0]["memory"]["content"]
        .as_str()
        .unwrap()
        .contains("port 9090"));

    // 3. Stats via CLI in remote mode
    let output = std::process::Command::new(exe)
        .env("LIGHTMEM_REMOTE_URL", &base_url)
        .env("LIGHTMEM_API_KEY", &api_key)
        .args(["stats", "--json"])
        .output()
        .expect("Failed to execute lmem stats");
    assert!(output.status.success());
    let stats: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stats["active_memories"], 1);

    // 4. Context via CLI in remote mode
    let output = std::process::Command::new(exe)
        .env("LIGHTMEM_REMOTE_URL", &base_url)
        .env("LIGHTMEM_API_KEY", &api_key)
        .args(["context", "--json"])
        .output()
        .expect("Failed to execute lmem context");
    assert!(output.status.success());
    let ctx: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(ctx["mode"], "remote");
    assert_eq!(ctx["status"], "online");
    assert_eq!(ctx["api_key_configured"], true);
}

#[test]
fn test_cli_serve_daemon_lifecycle() {
    let dir = tempdir().unwrap();
    let config_dir = dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    let db_path = dir.path().join("daemon_vault.db");

    // Reserve an open port
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let exe = env!("CARGO_BIN_EXE_lmem");

    // 1. Start Daemon
    let start_out = std::process::Command::new(exe)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args([
            "serve",
            "-d",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--api-key",
            "test_daemon_key",
            "--db",
            db_path.to_str().unwrap(),
        ])
        .output()
        .expect("Failed to execute serve -d");

    assert!(
        start_out.status.success(),
        "Daemon start failed: {}",
        String::from_utf8_lossy(&start_out.stderr)
    );
    let start_text = String::from_utf8_lossy(&start_out.stdout);
    assert!(start_text.contains("Daemon Started"));

    let pid_file = config_dir.join("server.pid");
    assert!(pid_file.exists(), "PID file should exist");

    // 2. Status Check: Running
    let status_out = std::process::Command::new(exe)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args(["serve", "--status"])
        .output()
        .expect("Failed to execute serve --status");
    assert!(status_out.status.success());
    let status_text = String::from_utf8_lossy(&status_out.stdout);
    assert!(status_text.contains("RUNNING"));

    // 3. HTTP Health Probe
    let health_url = format!("http://127.0.0.1:{}/v1/health", port);
    let mut responding = false;
    for _ in 0..20 {
        if ureq::get(&health_url)
            .timeout(std::time::Duration::from_millis(500))
            .call()
            .is_ok()
        {
            responding = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        responding,
        "Server daemon should respond on HTTP /v1/health"
    );

    // 4. Stop Daemon
    let stop_out = std::process::Command::new(exe)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args(["serve", "--stop"])
        .output()
        .expect("Failed to execute serve --stop");
    assert!(stop_out.status.success());
    let stop_text = String::from_utf8_lossy(&stop_out.stdout);
    assert!(stop_text.contains("stopped"));

    // 5. Status Check: Stopped & PID file cleaned up
    assert!(!pid_file.exists(), "PID file should be removed after stop");
    let status_after = std::process::Command::new(exe)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args(["serve", "--status"])
        .output()
        .expect("Failed to execute serve --status");
    assert!(status_after.status.success());
    let status_after_text = String::from_utf8_lossy(&status_after.stdout);
    assert!(status_after_text.contains("STOPPED"));
}
