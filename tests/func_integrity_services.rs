use lightmem::models::{
    AttachmentInput, GraphEdge, GraphNode, GraphSnapshot, MemoryLink, MemoryRecord, MemoryStatus,
    MemoryType, ScoredMemory,
};
use lightmem::services::embeddings::{
    cosine_similarity, validate_vector, EmbeddingProvider, HashEmbeddingProvider,
    OllamaEmbeddingProvider, OnnxEmbeddingProvider,
};
use lightmem::services::graph_view::render_terminal;
use lightmem::services::importer::{
    extract_imported_links, extract_raw_json_values, parse_single_json_value, ImportCandidate,
    JsonMemoryImporter, MemoryImporter, OkfMemoryImporter,
};
use lightmem::services::{
    ConnectService, Exporter, HybridSearchEngine, ObservedCandidate, ObserverService,
    OllamaReranker, OnnxQaReranker, Reranker, Top1Reranker,
};
use lightmem::Storage;
use std::io::{Read, Write};
use std::net::TcpListener;
use tempfile::tempdir;

fn make_mem(id: &str, title: &str, content: &str, cat: MemoryType) -> MemoryRecord {
    let mut m = MemoryRecord::new(
        cat,
        title.to_string(),
        content.to_string(),
        vec![],
        0.9,
        None,
    );
    m.id = id.to_string();
    m
}

// ============================================================================
// 1. services::embeddings — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_hash_embedding_cosine_and_validate_vector() {
    let emb = HashEmbeddingProvider;
    assert_eq!(emb.name(), "hash-fallback");

    let v1 = emb.embed("PostgreSQL runs on port 5432").unwrap();
    let v2 = emb.embed("PostgreSQL runs on port 5432").unwrap();
    assert_eq!(v1.len(), 384);
    assert_eq!(v1, v2);
    validate_vector(&v1).unwrap();

    let sim = cosine_similarity(&v1, &v2);
    assert!((sim - 1.0).abs() < 1e-5);

    let batch = emb
        .embed_batch(&["first".to_string(), "second".to_string()])
        .unwrap();
    assert_eq!(batch.len(), 2);
}

#[test]
fn integrity_embeddings_validate_vector_and_zero_mismatched_cosine() {
    assert!(validate_vector(&[]).is_err(), "Empty vector must fail");
    assert!(validate_vector(&[1.0, f32::NAN]).is_err(), "NaN must fail");
    assert!(
        validate_vector(&[1.0, f32::INFINITY]).is_err(),
        "Inf must fail"
    );

    // Mismatched dimensions or zero vectors must return 0.0 (never NaN)
    assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
    assert_eq!(cosine_similarity(&[], &[]), 0.0);
    assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 2.0]), 0.0);

    // Invalid ONNX model name or missing custom dir must fail cleanly
    assert!(OnnxEmbeddingProvider::new(Some("nonexistent-onnx-model")).is_err());
    let dir = tempdir().unwrap();
    assert!(OnnxEmbeddingProvider::new_custom_dir(dir.path()).is_err());
}

#[test]
fn func_and_integrity_ollama_embedding_provider_fallback_and_empty_error() {
    // Spawn mock server: 1st request to /api/embeddings returns 404, 2nd to /api/embed returns vector, 3rd returns empty
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = std::thread::spawn(move || {
        for step in 0..3 {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let resp = match step {
                    0 => {
                        assert!(req.contains("POST /api/embeddings"));
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_string()
                    }
                    1 => {
                        assert!(req.contains("POST /api/embed"));
                        let body = r#"{"embeddings":[[0.6,0.8]]}"#;
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                    }
                    _ => {
                        let body = r#"{"embeddings":[]}"#;
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                    }
                };
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
    });

    let provider = OllamaEmbeddingProvider::with_api_key(
        format!("http://{}/", addr),
        "nomic-embed-text".to_string(),
        Some("test-token".to_string()),
    );
    assert_eq!(provider.name(), "ollama");
    let vec = provider.embed("hello world").unwrap();
    assert_eq!(vec, vec![0.6, 0.8]);

    // 3rd call returns empty embeddings array -> integrity check must fail
    assert!(provider.embed("empty test").is_err());
    let _ = handle.join();
}

// ============================================================================
// 2. services::search — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_hybrid_search_rrf_and_acronym_boost() {
    let storage = Storage::open_in_memory().unwrap();
    let emb = HashEmbeddingProvider;

    let m1 = make_mem(
        "m-kk",
        "Lead Architect",
        "Krishna Kanth designed the hybrid search engine",
        MemoryType::Fact,
    );
    let m2 = make_mem(
        "m-pg",
        "Database Port",
        "PostgreSQL runs on port 5432",
        MemoryType::Fact,
    );
    storage
        .insert_indexed_batch(&[m1.clone(), m2], &emb, "hash-id", true)
        .unwrap();

    let q_tokens = HybridSearchEngine::extract_query_tokens("Who is KK?");
    assert_eq!(q_tokens, vec!["kk".to_string()]);
    let boost = HybridSearchEngine::compute_acronym_and_lexical_boost(&q_tokens, &m1);
    assert!(
        boost >= 0.25,
        "Proper noun initials KK -> Krishna Kanth must receive >=0.26 boost"
    );

    let hits = HybridSearchEngine::search_with_identity(
        &storage,
        &emb,
        "PostgreSQL 5432",
        None,
        Some(MemoryStatus::Active),
        None,
        5,
        None,
        Some("hash-id"),
    )
    .unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].memory.id, "m-pg");
}

#[test]
fn integrity_hybrid_search_rejects_blank_query_and_invalid_min_similarity() {
    let storage = Storage::open_in_memory().unwrap();
    let emb = HashEmbeddingProvider;

    assert!(HybridSearchEngine::search(&storage, &emb, "   ", None, None, None, 5, None).is_err());
    assert!(
        HybridSearchEngine::search(&storage, &emb, "test", None, None, None, 5, Some(1.5)).is_err()
    );
    assert!(
        HybridSearchEngine::search(&storage, &emb, "test", None, None, None, 5, Some(-0.1))
            .is_err()
    );
    assert!(
        HybridSearchEngine::search(&storage, &emb, "test", None, None, None, 0, None)
            .unwrap()
            .is_empty()
    );
}

// ============================================================================
// 3. services::reranker — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_top1_and_ollama_reranker_answer_and_merge() {
    let m1 = make_mem(
        "m1",
        "Redis Port",
        "Redis runs on port 6379",
        MemoryType::Fact,
    );
    let m2 = make_mem(
        "m2",
        "Redis Port New",
        "Redis runs on port 6380",
        MemoryType::Fact,
    );
    let candidate = ScoredMemory {
        memory: m1.clone(),
        score: 0.85,
        bm25_rank: Some(1),
        vector_rank: Some(1),
    };

    let top1 = Top1Reranker;
    assert_eq!(top1.name(), "top1");
    let ans = top1
        .answer("What port does Redis run on?", &[candidate])
        .unwrap();
    assert_eq!(ans.answer, "Redis runs on port 6379");

    let merged = top1.merge_conflict(&m1, &m2).unwrap();
    assert_eq!(merged.content, "Redis runs on port 6380");
}

#[test]
fn integrity_reranker_low_score_gating_and_hallucination_rejection() {
    let low_mem = make_mem("m-low", "Unrelated", " Unrelated note", MemoryType::Fact);
    let low_cand = ScoredMemory {
        memory: low_mem,
        score: 0.15,
        bm25_rank: None,
        vector_rank: Some(10),
    };

    // Top1Reranker must reject ungrounded low-score candidate (< 0.35 and no BM25 rank)
    let ans = Top1Reranker
        .answer("What is the secret code?", std::slice::from_ref(&low_cand))
        .unwrap();
    assert!(ans.selected_memory.is_none());
    assert_eq!(ans.confidence, 0.0);

    // OllamaReranker with mock server returning an ungrounded hallucinated answer must reject it!
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = r#"{"choices":[{"message":{"content":"{\"answer\":\"hallucinated_9999\",\"memory_index\":0,\"confidence\":0.99}"}}],"response":"{\"answer\":\"hallucinated_9999\",\"memory_index\":0,\"confidence\":0.99}"}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });

    let valid_cand = ScoredMemory {
        memory: make_mem("m1", "Redis", "Redis runs on port 6379", MemoryType::Fact),
        score: 0.9,
        bm25_rank: Some(1),
        vector_rank: Some(1),
    };
    let ollama = OllamaReranker::new(
        format!("http://127.0.0.1:{}", port),
        Some("qwen2.5:1.5b".to_string()),
    );
    let res = ollama.answer("What port?", &[valid_cand]).unwrap();
    let _ = handle.join();
    assert!(
        res.selected_memory.is_none(),
        "Hallucinated answer not grounded in candidate memory must be rejected"
    );

    // Invalid ONNX QA model preset must return error
    assert!(OnnxQaReranker::ensure_model_downloaded(Some("unknown-qa-model")).is_err());
}

// ============================================================================
// 4. services::observer — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_observer_extract_local_and_candidate_conversion() {
    let dialogue = r#"
User: Hey, just to let you know, I prefer dark mode in all desktop apps.
Assistant: Sounds good!
User: Also we decided to standardize on PostgreSQL 16 for production.
"#;
    let candidates = ObserverService::extract(
        dialogue,
        None,
        Some(&["session-1".to_string()]),
        Some(0.80),
        Some("local"),
        None,
    )
    .unwrap();

    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .any(|c| c.category == MemoryType::Preference
            && c.content.contains("User prefers dark mode")));
    assert!(candidates
        .iter()
        .any(|c| c.category == MemoryType::Decision && c.tags.contains(&"postgresql".to_string())));

    let rec = candidates[0].to_memory_record();
    assert_eq!(rec.provenance, "observed");
}

#[test]
fn integrity_observer_filters_questions_filler_and_deduplicates() {
    let noise = r#"
User: What port does Redis run on?
Assistant: Thank you
User: Sounds good
User: I prefer dark mode in editors.
User: I prefer dark mode in editors.
"#;
    let candidates =
        ObserverService::extract(noise, None, None, Some(0.85), Some("local"), None).unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "Questions, trivial filler, and duplicate observations must be filtered out"
    );

    let clamped = ObservedCandidate::new("T", "C", MemoryType::Fact, vec![], 5.0);
    assert_eq!(clamped.confidence, 1.0);
}

// ============================================================================
// 5. services::exporter & services::importer — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_exporter_and_importer_json_and_okf_roundtrip() {
    let dir = tempdir().unwrap();
    let storage = Storage::open_in_memory().unwrap();
    let emb = HashEmbeddingProvider;

    let att = AttachmentInput::from_bytes("config.json", b"{\"port\":8080}".to_vec());
    let m1 = make_mem("id-1", "App Config", "Port is 8080", MemoryType::Artifact);
    let m2 = make_mem(
        "id-2",
        "Auth Rule",
        "Always validate JWT",
        MemoryType::Instruction,
    );

    storage
        .insert_indexed_with_attachments(&m1, &[att], &emb, "hash-id", true)
        .unwrap();
    storage
        .insert_indexed_batch(&[m2], &emb, "hash-id", true)
        .unwrap();
    storage
        .add_link("id-1", "id-2", Some("depends_on"), Some(1.0))
        .unwrap();

    // 1. Export & parse JSON backup
    let json_path = dir.path().join("backup.json");
    Exporter::export_json(&storage, &json_path).unwrap();
    let raw_json = std::fs::read_to_string(&json_path).unwrap();
    let json_candidates = JsonMemoryImporter.parse(&raw_json).unwrap();
    let json_links = extract_imported_links(&raw_json, true);
    assert_eq!(json_candidates.len(), 2);
    assert_eq!(json_links.len(), 1);
    let (restored_rec, restored_atts) = json_candidates
        .into_iter()
        .find(|c| c.id.as_deref() == Some("id-1"))
        .unwrap()
        .into_record_and_attachments()
        .unwrap();
    assert_eq!(restored_rec.title, "App Config");
    assert_eq!(restored_atts.len(), 1);
    assert_eq!(restored_atts[0].data, b"{\"port\":8080}");

    // 2. Export & parse OKF bundle
    let okf_path = dir.path().join("bundle.okf");
    Exporter::export_okf(&storage, Some(&okf_path)).unwrap();
    let raw_okf = std::fs::read_to_string(&okf_path).unwrap();
    let okf_candidates = OkfMemoryImporter.parse(&raw_okf).unwrap();
    let okf_links = extract_imported_links(&raw_okf, false);
    assert_eq!(okf_candidates.len(), 2);
    assert_eq!(okf_links.len(), 1);
}

#[test]
fn integrity_importer_rejects_corrupt_sha256_bad_confidence_and_html_injection() {
    // HTML comment injection in title/content must be escaped in OKF export
    let dir = tempdir().unwrap();
    let storage = Storage::open_in_memory().unwrap();
    let hostile = make_mem(
        "id-hostile",
        "Title --> injection",
        "Content <!-- comment -->",
        MemoryType::Fact,
    );
    storage.insert_memory(&hostile, None).unwrap();
    let okf_path = dir.path().join("hostile.okf");
    Exporter::export_okf(&storage, Some(&okf_path)).unwrap();
    let raw_okf = std::fs::read_to_string(&okf_path).unwrap();
    let parsed = OkfMemoryImporter.parse(&raw_okf).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].title, "Title --> injection");

    // Corrupt attachment SHA-256 in import candidate must be rejected
    let bad_att_json = r#"[{
        "id": "m1",
        "title": "Bad Att",
        "content": "Payload mismatch",
        "attachments": [{
            "id": "a1",
            "memory_id": "m1",
            "filename": "f.txt",
            "mime_type": "text/plain",
            "media_kind": "document",
            "size_bytes": 5,
            "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
            "created_at": "2026-01-01T00:00:00Z",
            "data_base64": "aGVsbG8="
        }]
    }]"#;
    let mut candidates = JsonMemoryImporter.parse(bad_att_json).unwrap();
    assert!(
        candidates.remove(0).into_record_and_attachments().is_err(),
        "Importer must reject attachment whose base64 bytes do not match its SHA-256"
    );

    // Out-of-bounds confidence in JSON import must fail
    let bad_conf_json = r#"[{"content": "test", "confidence": 1.5}]"#;
    assert!(JsonMemoryImporter.parse(bad_conf_json).is_err());

    // Empty import must fail
    assert!(extract_raw_json_values("   ").is_err());
    assert!(parse_single_json_value(&serde_json::json!({})).is_none());
    let _ = ImportCandidate::from(hostile).to_memory_record();
    let _ = MemoryLink::new("a", "b", None, None);
}

// ============================================================================
// 6. services::connect & services::graph_view — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_connect_service_and_graph_view_render_terminal() {
    let platforms = ConnectService::list_platforms(true);
    assert_eq!(platforms.len(), 6);

    let dir = tempdir().unwrap();
    let res = ConnectService::connect(None, false, Some(dir.path())).unwrap();
    assert_eq!(res.len(), 1);
    assert!(res[0].created);

    // Second connect updates existing file
    let res2 = ConnectService::connect(None, false, Some(dir.path())).unwrap();
    assert!(!res2[0].created);

    let snap = GraphSnapshot {
        nodes: vec![
            GraphNode {
                id: "11112222-3333".to_string(),
                label: "Gateway".to_string(),
                category: "artifact".to_string(),
                tags: vec![],
                confidence: 0.9,
                degree: 1,
                snippet: "Gateway".to_string(),
            },
            GraphNode {
                id: "44445555-6666".to_string(),
                label: "Postgres".to_string(),
                category: "fact".to_string(),
                tags: vec![],
                confidence: 0.9,
                degree: 1,
                snippet: "Postgres".to_string(),
            },
        ],
        edges: vec![GraphEdge {
            source: "11112222-3333".to_string(),
            target: "44445555-6666".to_string(),
            relation: "routes_to".to_string(),
            weight: 1.0,
        }],
    };
    let rendered = render_terminal(&snap);
    assert!(rendered.contains("Gateway"));
    assert!(rendered.contains("routes_to"));
    assert!(rendered.contains("Postgres"));
}

#[test]
fn integrity_connect_rejects_unknown_platform_and_graph_view_handles_empty() {
    assert!(ConnectService::connect(Some("unknown-platform"), true, None).is_err());
    assert!(!ConnectService::is_platform_detected("unknown-platform"));

    let empty = GraphSnapshot {
        nodes: vec![],
        edges: vec![],
    };
    let rendered = render_terminal(&empty);
    assert!(rendered.contains("No memories or connections found"));
}
