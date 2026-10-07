use lightmem::{
    EmbeddingProvider, MemoryRecord, MemoryType, OllamaEmbeddingProvider, OllamaReranker,
    Reranker, ScoredMemory,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[test]
fn test_ollama_embedding_provider_sends_authorization_header() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth_received = Arc::new(AtomicBool::new(false));
    let auth_received_clone = Arc::clone(&auth_received);

    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.contains("Authorization: Bearer secret-test-token-42") {
                auth_received_clone.store(true, Ordering::SeqCst);
            }
            let body = r#"{"embedding": [0.1, 0.2, 0.3]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });

    let provider = OllamaEmbeddingProvider::with_api_key(
        format!("http://127.0.0.1:{}", port),
        "test-model".to_string(),
        Some("secret-test-token-42".to_string()),
    );

    let res = provider.embed("hello authenticated ollama");
    let _ = handle.join();
    assert!(res.is_ok(), "Embedding error: {:?}", res.err());
    assert!(
        auth_received.load(Ordering::SeqCst),
        "HTTP server should have received Authorization: Bearer secret-test-token-42"
    );
}

#[test]
fn test_ollama_reranker_sends_authorization_header() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let auth_received = Arc::new(AtomicBool::new(false));
    let auth_received_clone = Arc::clone(&auth_received);

    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            if req.contains("Authorization: Bearer reranker-secret-99") {
                auth_received_clone.store(true, Ordering::SeqCst);
            }
            let body = r#"{"response":"{\"answer\":\"5432\",\"memory_index\":0,\"confidence\":0.95}"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });

    let reranker = OllamaReranker::with_api_key(
        format!("http://127.0.0.1:{}", port),
        Some("qwen2.5:3b".to_string()),
        Some("reranker-secret-99".to_string()),
    );

    let mem = MemoryRecord::new(
        MemoryType::Fact,
        "Postgres Port".to_string(),
        "PostgreSQL runs on port 5432 on Linux".to_string(),
        vec!["db".to_string()],
        0.95,
        None,
    );
    let candidate = ScoredMemory {
        memory: mem,
        score: 0.95,
        vector_rank: Some(1),
        bm25_rank: Some(1),
    };

    let result = reranker.answer("What port does PostgreSQL use?", &[candidate]);
    let _ = handle.join();
    assert!(result.is_ok(), "Answer error: {:?}", result.err());
    let ans = result.unwrap();
    assert_eq!(ans.answer, "5432");
    assert!(
        auth_received.load(Ordering::SeqCst),
        "HTTP server should have received Authorization: Bearer reranker-secret-99"
    );
}

#[test]
fn test_config_cli_api_key_flag_and_json_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();

    let binary = env!("CARGO_BIN_EXE_lmem");

    // 1. Set API key (auto-switches URL to https://ollama.com when no explicit url is provided)
    let output = std::process::Command::new(binary)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args(&["config", "--api-key", "sk-supersecret123456"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Ollama API Key"));
    assert!(stdout.contains("sk-s...3456"));
    assert!(stdout.contains("https://ollama.com"));

    // 2. Read with --json flag
    let output_json = std::process::Command::new(binary)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args(&["config", "--json"])
        .output()
        .unwrap();
    assert!(output_json.status.success());
    let json_val: serde_json::Value =
        serde_json::from_slice(&output_json.stdout).expect("Valid JSON");
    assert_eq!(
        json_val.get("ollama_api_key").unwrap().as_str().unwrap(),
        "sk-supersecret123456"
    );
    assert_eq!(
        json_val.get("has_ollama_api_key").unwrap().as_bool().unwrap(),
        true
    );
    assert_eq!(
        json_val.get("ollama_url").unwrap().as_str().unwrap(),
        "https://ollama.com"
    );
    assert!(json_val.get("active_db").is_some());
    assert!(json_val.get("config_file").is_some());
    assert!(json_val.get("embedding_identity").is_some());

    // 3. Set custom URL with trailing /api/chat - verifies normalization
    let output_url = std::process::Command::new(binary)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args(&["config", "--url", "https://custom-proxy.internal/api/chat/"])
        .output()
        .unwrap();
    assert!(output_url.status.success());
    let stdout_url = String::from_utf8_lossy(&output_url.stdout);
    assert!(stdout_url.contains("https://custom-proxy.internal"));

    // 4. Clear API key (resets URL back to localhost if it was cloud)
    let output_clear = std::process::Command::new(binary)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args(&["config", "--clear-api-key"])
        .output()
        .unwrap();
    assert!(output_clear.status.success());
    let stdout_clear = String::from_utf8_lossy(&output_clear.stdout);
    assert!(stdout_clear.contains("none (unauthenticated"));

    let output_json_after = std::process::Command::new(binary)
        .env("LIGHTMEM_CONFIG_DIR", &config_dir)
        .args(&["config", "--json"])
        .output()
        .unwrap();
    assert!(output_json_after.status.success());
    let json_after: serde_json::Value = serde_json::from_slice(&output_json_after.stdout).unwrap();
    assert!(json_after.get("ollama_api_key").is_none() || json_after.get("ollama_api_key").unwrap().is_null());
    assert_eq!(json_after.get("has_ollama_api_key").unwrap().as_bool().unwrap(), false);
}
