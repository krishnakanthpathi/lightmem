use chrono::{Duration, Utc};
use lightmem::models::config::format_auth_header;
use lightmem::models::link::{extract_wikilinks, GraphEdge, GraphNode, GraphSnapshot, MemoryLink};
use lightmem::models::memory::{
    compute_sha256_hex, decode_base64, detect_memory_conflict,
    detect_memory_conflict_with_similarity, detect_mime_and_kind, encode_base64,
    extract_meaningful_tokens, parse_audio_duration_secs, parse_image_dimensions,
    parse_mp4_dimensions_and_duration, parse_ttl_duration, AttachmentInput, MemoryRecord,
    MemoryStatus, MemoryType, PaginatedMemories,
};
use lightmem::models::stats::StorageStats;
use lightmem::LightMemConfig;
use std::str::FromStr;
use std::sync::Mutex;
use tempfile::tempdir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

// ============================================================================
// 1. models::config — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_format_auth_header() {
    assert_eq!(format_auth_header("secret-token-123"), "Bearer secret-token-123");
}

#[test]
fn integrity_format_auth_header_idempotency_and_whitespace() {
    assert_eq!(
        format_auth_header("  Bearer   already-prefixed  "),
        "Bearer   already-prefixed"
    );
    assert_eq!(
        format_auth_header("bearer lower-prefixed"),
        "bearer lower-prefixed"
    );
    assert_eq!(format_auth_header("   raw-key   "), "Bearer raw-key");
}

#[test]
fn func_config_default() {
    let cfg = LightMemConfig::default();
    assert_eq!(cfg.backend, "onnx");
    assert_eq!(cfg.onnx_model.as_deref(), Some("Xenova/bge-small-en-v1.5"));
    assert_eq!(cfg.ollama_url, "http://localhost:11434");
    assert_eq!(cfg.ollama_api_key, None);
    assert_eq!(cfg.embedding_model, "nomic-embed-text");
    assert_eq!(cfg.reranker, "minilm-squad2");
}

#[test]
fn integrity_config_default_produces_valid_identity() {
    let cfg = LightMemConfig::default();
    let id = cfg.embedding_identity().expect("default config must have valid identity");
    assert_eq!(id, "card-v1:fastembed-7.1.0:Xenova/bge-small-en-v1.5");
}

#[test]
fn func_normalize_reranker() {
    assert_eq!(LightMemConfig::normalize_reranker("top1", None), "top1");
    assert_eq!(
        LightMemConfig::normalize_reranker("ollama:qwen2.5:3b", None),
        "ollama:qwen2.5:3b"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("openai:gpt-4o-mini", None),
        "ollama:gpt-4o-mini"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("llm:gemma4:31b-cloud", None),
        "ollama:gemma4:31b-cloud"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("tinyroberta", None),
        "tinyroberta-squad2"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("minilm-squad2", None),
        "minilm-squad2"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("onnx:tinyroberta-squad2", None),
        "tinyroberta-squad2"
    );
}

#[test]
fn integrity_normalize_reranker_legacy_qa_and_fallback() {
    assert_eq!(
        LightMemConfig::normalize_reranker("ollama", Some("qwen2.5:7b")),
        "ollama:qwen2.5:7b"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("ollama", Some("minilm-squad2")),
        "ollama"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("onnx", Some("tinyroberta")),
        "tinyroberta-squad2"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("qwen2.5:1.5b", None),
        "ollama:qwen2.5:1.5b"
    );
    assert_eq!(
        LightMemConfig::normalize_reranker("completely-unknown-value", None),
        "minilm-squad2"
    );
}

#[test]
fn func_apply_preset() {
    let mut cfg = LightMemConfig::default();
    cfg.apply_preset("local").unwrap();
    assert_eq!(cfg.backend, "onnx");
    assert_eq!(cfg.onnx_model.as_deref(), Some("bge-small"));
    assert_eq!(cfg.reranker, "minilm-squad2");

    cfg.apply_preset("ollama-cloud").unwrap();
    assert_eq!(cfg.reranker, "ollama:gemma4:31b-cloud");

    cfg.apply_preset("ollama-local").unwrap();
    assert_eq!(cfg.reranker, "ollama");
}

#[test]
fn integrity_apply_preset_rejects_unknown_preset() {
    let mut cfg = LightMemConfig::default();
    let err = cfg.apply_preset("invalid-preset").unwrap_err();
    assert!(err.to_string().contains("Unknown preset 'invalid-preset'"));
}

#[test]
fn func_normalize_and_effective_ollama_url() {
    assert_eq!(
        LightMemConfig::normalize_ollama_url("http://localhost:11434/api/chat/"),
        "http://localhost:11434"
    );
    assert_eq!(
        LightMemConfig::normalize_ollama_url("https://proxy.example.com/v1"),
        "https://proxy.example.com"
    );
    let mut cfg = LightMemConfig::default();
    cfg.ollama_url = "http://127.0.0.1:11434/api/embeddings".to_string();
    assert_eq!(cfg.effective_ollama_url(), "http://127.0.0.1:11434");
}

#[test]
fn integrity_normalize_ollama_url_empty_and_whitespace() {
    assert_eq!(
        LightMemConfig::normalize_ollama_url("   "),
        "http://localhost:11434"
    );
    assert_eq!(
        LightMemConfig::normalize_ollama_url("/api/generate/"),
        "http://localhost:11434"
    );
}

#[test]
fn func_effective_ollama_api_key() {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut cfg = LightMemConfig::default();
    cfg.ollama_api_key = Some(" explicit-key ".to_string());
    assert_eq!(cfg.effective_ollama_api_key().as_deref(), Some("explicit-key"));
}

#[test]
fn integrity_effective_ollama_api_key_ignores_none_and_blank() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::remove_var("OLLAMA_API_KEY");
    std::env::remove_var("LMEM_OLLAMA_API_KEY");

    let mut cfg = LightMemConfig::default();
    cfg.ollama_api_key = Some("NONE".to_string());
    assert_eq!(cfg.effective_ollama_api_key(), None);

    cfg.ollama_api_key = Some("   ".to_string());
    assert_eq!(cfg.effective_ollama_api_key(), None);
}

#[test]
fn func_embedding_identity_and_summaries() {
    let mut cfg = LightMemConfig {
        backend: "hash".to_string(),
        ..Default::default()
    };
    assert_eq!(
        cfg.embedding_identity().unwrap(),
        "card-v1:hash:fnv-bigram-v1"
    );
    assert!(cfg.active_embedding_summary().contains("deterministic-trigram-hash"));

    cfg.backend = "ollama".to_string();
    cfg.ollama_url = "http://localhost:11434/api".to_string();
    cfg.embedding_model = "mxbai-embed-large".to_string();
    assert_eq!(
        cfg.embedding_identity().unwrap(),
        "card-v1:ollama:http://localhost:11434:mxbai-embed-large"
    );
    assert!(cfg.active_embedding_summary().contains("mxbai-embed-large"));

    cfg.reranker = "tinyroberta-squad2".to_string();
    assert!(cfg.active_reranker_summary().contains("tinyroberta-squad2"));
    cfg.reranker = "top1".to_string();
    assert!(cfg.active_reranker_summary().contains("top1"));
}

#[test]
fn integrity_embedding_identity_custom_dir_fingerprints_contents_and_rejects_bad_backend() {
    let dir = tempdir().unwrap();
    let model_dir = dir.path().join("custom_onnx");
    std::fs::create_dir_all(&model_dir).unwrap();
    std::fs::write(model_dir.join("model.onnx"), b"weights-v1").unwrap();
    std::fs::write(model_dir.join("tokenizer.json"), b"{}").unwrap();

    let mut cfg = LightMemConfig {
        backend: "onnx".to_string(),
        onnx_model: Some(model_dir.to_string_lossy().to_string()),
        ..Default::default()
    };
    let id1 = cfg.embedding_identity().unwrap();

    // Mutating model weights in-place must alter the FNV-1a fingerprint
    std::fs::write(model_dir.join("model.onnx"), b"weights-v2").unwrap();
    let id2 = cfg.embedding_identity().unwrap();
    assert_ne!(id1, id2, "Custom ONNX model identity must change when file contents change");

    // Missing directory must error
    cfg.onnx_model = Some(dir.path().join("nonexistent").to_string_lossy().to_string());
    assert!(cfg.embedding_identity().is_err());

    // Unknown backend must error
    cfg.backend = "quantum-flux".to_string();
    assert!(cfg.embedding_identity().is_err());
}

#[test]
fn func_config_save_load_and_resolve_db_path() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_var("LIGHTMEM_CONFIG_DIR", dir.path());
    std::env::remove_var("LIGHTMEM_DB");

    let mut cfg = LightMemConfig::default();
    cfg.backend = "hash".to_string();
    cfg.reranker = "top1".to_string();
    cfg.save().unwrap();

    let loaded = LightMemConfig::try_load().unwrap();
    assert_eq!(loaded.backend, "hash");
    assert_eq!(loaded.reranker, "top1");
    assert_eq!(LightMemConfig::config_file(), dir.path().join("config.json"));
    assert_eq!(LightMemConfig::global_db_path(), dir.path().join("memories.db"));
    assert_eq!(LightMemConfig::resolve_db_path(true), dir.path().join("memories.db"));

    std::env::remove_var("LIGHTMEM_CONFIG_DIR");
}

#[test]
fn integrity_config_corrupt_json_fails_try_load_but_load_falls_back() {
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempdir().unwrap();
    std::env::set_var("LIGHTMEM_CONFIG_DIR", dir.path());

    std::fs::write(dir.path().join("config.json"), "{ invalid json ").unwrap();
    assert!(LightMemConfig::try_load().is_err());
    let fallback = LightMemConfig::load();
    assert_eq!(fallback.backend, "onnx");

    std::env::remove_var("LIGHTMEM_CONFIG_DIR");
}

// ============================================================================
// 2. models::memory — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_memory_type_and_status_roundtrip() {
    for &cat in MemoryType::ALL {
        let s = cat.as_str();
        assert_eq!(cat.to_string(), s);
        assert_eq!(MemoryType::from_str(s).unwrap(), cat);
    }
    assert_eq!(MemoryType::from_str("passwords").unwrap(), MemoryType::Password);
    assert_eq!(MemoryStatus::from_str("active").unwrap(), MemoryStatus::Active);
    assert_eq!(MemoryStatus::from_str("expired").unwrap(), MemoryStatus::Expired);
    assert_eq!(MemoryStatus::Active.as_str(), "active");
    assert_eq!(MemoryStatus::Expired.as_str(), "expired");
}

#[test]
fn integrity_memory_type_and_status_reject_invalid_strings() {
    assert!(MemoryType::from_str("not_a_category").is_err());
    assert!(MemoryStatus::from_str("deleted").is_err());
}

#[test]
fn func_memory_type_infer_all_14_categories() {
    assert_eq!(MemoryType::infer("password: super-secret"), MemoryType::Password);
    assert_eq!(MemoryType::infer("panic: worker thread crashed"), MemoryType::Error);
    assert_eq!(MemoryType::infer("We decided to use SQLite WAL"), MemoryType::Decision);
    assert_eq!(MemoryType::infer("Never commit .env files"), MemoryType::Instruction);
    assert_eq!(MemoryType::infer("User prefers dark mode"), MemoryType::Preference);
    assert_eq!(MemoryType::infer("Our goal is sub-5ms recall"), MemoryType::Goal);
    assert_eq!(MemoryType::infer("TODO: ship release v0.3"), MemoryType::Commitment);
    assert_eq!(MemoryType::infer("Learned that FTS5 requires triggers"), MemoryType::Learning);
    assert_eq!(MemoryType::infer("Deployed v0.2.4 to production"), MemoryType::Event);
    assert_eq!(MemoryType::infer("Alice is the maintainer of auth"), MemoryType::Relationship);
    assert_eq!(MemoryType::infer("Profiling shows 0ms idle CPU"), MemoryType::Observation);
    assert_eq!(MemoryType::infer("OpenAPI schema definition saved"), MemoryType::Artifact);
    assert_eq!(MemoryType::infer("Currently working on test suite"), MemoryType::Context);
    assert_eq!(MemoryType::infer("PostgreSQL runs on port 5432"), MemoryType::Fact);
}

#[test]
fn integrity_memory_type_infer_prefix_precedence_and_url_token_isolation() {
    // Explicit prefix must beat mid-sentence keywords
    assert_eq!(
        MemoryType::infer("Fact: we decided to fix panic in dark mode"),
        MemoryType::Fact
    );
    // Scoped package URL with separate port must not falsely trigger credential URL rule
    assert_eq!(
        MemoryType::infer("Registry is https://registry.npmjs.org/@scope/pkg on port: 443"),
        MemoryType::Fact
    );
    // Real user:pass@host URI must trigger Password
    assert_eq!(
        MemoryType::infer("Connect via redis://default:s3cr3t@10.0.0.1:6379"),
        MemoryType::Password
    );
}

#[test]
fn func_sha256_hex_nist_vectors() {
    assert_eq!(
        compute_sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        compute_sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn integrity_sha256_hex_padding_block_boundaries() {
    // 55 bytes (fits in 1 block), 56 bytes (spills length to 2nd block), 64 bytes (exact block)
    let b55 = vec![b'a'; 55];
    let b56 = vec![b'a'; 56];
    let b64 = vec![b'a'; 64];
    let h55 = compute_sha256_hex(&b55);
    let h56 = compute_sha256_hex(&b56);
    let h64 = compute_sha256_hex(&b64);
    assert_eq!(h55.len(), 64);
    assert_eq!(h56.len(), 64);
    assert_eq!(h64.len(), 64);
    assert_ne!(h55, h56);
    assert_ne!(h56, h64);
}

#[test]
fn func_base64_encode_decode_roundtrip() {
    for len in [0, 1, 2, 3, 4, 5, 31, 64, 255] {
        let data: Vec<u8> = (0..len).map(|i| (i * 37) as u8).collect();
        let encoded = encode_base64(&data);
        let decoded = decode_base64(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
}

#[test]
fn integrity_base64_decode_whitespace_and_invalid_input() {
    let encoded = "SGVs\r\nbG8g\tV29y\nbGQ=";
    assert_eq!(decode_base64(encoded).unwrap(), b"Hello World");
    assert!(decode_base64("abc").is_err(), "Non-multiple of 4 must fail");
    assert!(decode_base64("ab$=").is_err(), "Invalid character must fail");
}

#[test]
fn func_detect_mime_and_kind_and_media_parsers() {
    // PNG 640x480
    let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    png.extend_from_slice(&640u32.to_be_bytes());
    png.extend_from_slice(&480u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    assert_eq!(
        detect_mime_and_kind("diagram.png", &png),
        ("image/png".to_string(), "image".to_string())
    );
    assert_eq!(parse_image_dimensions(&png).as_deref(), Some("640x480"));

    // GIF 320x200
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&320u16.to_le_bytes());
    gif.extend_from_slice(&200u16.to_le_bytes());
    assert_eq!(parse_image_dimensions(&gif).as_deref(), Some("320x200"));

    // WAV 4.0s
    let mut wav = b"RIFF\x24\xfa\x00\x00WAVEfmt ".to_vec();
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&[1, 0, 1, 0]);
    wav.extend_from_slice(&16000u32.to_le_bytes());
    wav.extend_from_slice(&16000u32.to_le_bytes()); // byte_rate = 16000
    wav.extend_from_slice(&[1, 0, 8, 0]);
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&64000u32.to_le_bytes()); // data_bytes = 64000 -> 4.0s
    assert_eq!(parse_audio_duration_secs(&wav), Some(4.0));

    // MP4 with moov -> mvhd (2000 / 1000 = 2.0s) & tkhd (1280x720)
    let mut mvhd = vec![0u8; 20];
    mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
    mvhd[16..20].copy_from_slice(&2000u32.to_be_bytes());
    let mut tkhd = vec![0u8; 84];
    tkhd[76..80].copy_from_slice(&(1280u32 << 16).to_be_bytes());
    tkhd[80..84].copy_from_slice(&(720u32 << 16).to_be_bytes());

    let mut moov_body = Vec::new();
    moov_body.extend_from_slice(&((8 + mvhd.len()) as u32).to_be_bytes());
    moov_body.extend_from_slice(b"mvhd");
    moov_body.extend_from_slice(&mvhd);
    moov_body.extend_from_slice(&((8 + tkhd.len()) as u32).to_be_bytes());
    moov_body.extend_from_slice(b"tkhd");
    moov_body.extend_from_slice(&tkhd);

    let mut mp4 = Vec::new();
    mp4.extend_from_slice(&16u32.to_be_bytes());
    mp4.extend_from_slice(b"ftypisom\x00\x00\x00\x00");
    mp4.extend_from_slice(&((8 + moov_body.len()) as u32).to_be_bytes());
    mp4.extend_from_slice(b"moov");
    mp4.extend_from_slice(&moov_body);

    let (dims, dur) = parse_mp4_dimensions_and_duration(&mp4);
    assert_eq!(dims.as_deref(), Some("1280x720"));
    assert_eq!(dur, Some(2.0));
}

#[test]
fn integrity_media_parsers_and_attachment_input_handle_corrupt_or_spoofed_bytes() {
    // Magic bytes override spoofed extension
    let pdf_bytes = b"%PDF-1.7 fake pdf";
    assert_eq!(
        detect_mime_and_kind("virus.exe", pdf_bytes),
        ("application/pdf".to_string(), "document".to_string())
    );
    // Truncated headers must safely return None without panicking
    assert_eq!(parse_image_dimensions(b"\x89PNG\r\n\x1a\nshort"), None);
    assert_eq!(parse_image_dimensions(b"\xFF\xD8\xFF\xC0\x00"), None);
    assert_eq!(parse_audio_duration_secs(b"RIFF1234WAVEfmt "), None);
    assert_eq!(
        parse_mp4_dimensions_and_duration(b"\x00\x00\xFF\xFFmoov"),
        (None, None)
    );

    // Path traversal filename sanitization in AttachmentInput::from_bytes
    let att = AttachmentInput::from_bytes("../../etc/passwd", b"hello world".to_vec());
    assert_eq!(att.metadata.filename, "passwd");
    assert!(att.metadata.summary_text().contains("passwd"));

    // Empty filename fallback
    let att_empty = AttachmentInput::from_bytes("   ", vec![0xFF, 0x00, 0xFE]);
    assert_eq!(att_empty.metadata.filename, "attachment.bin");
    assert_eq!(att_empty.metadata.media_kind, "binary");
}

#[test]
fn func_memory_record_and_card_text() {
    let mut rec = MemoryRecord::new(
        MemoryType::Decision,
        "Database Engine".to_string(),
        "Use SQLite WAL mode".to_string(),
        vec!["sqlite".to_string(), "wal".to_string()],
        0.95,
        None,
    );
    let att = AttachmentInput::from_bytes("schema.sql", b"CREATE TABLE t(id INT);".to_vec());
    rec.attachments.push(att.metadata);
    let card = rec.to_card_text();
    assert!(card.contains("[DECISION] Database Engine"));
    assert!(card.contains("Use SQLite WAL mode"));
    assert!(card.contains("Attachments: schema.sql"));
    assert!(card.contains("Tags: sqlite, wal"));
}

#[test]
fn integrity_memory_record_clamps_out_of_bounds_confidence() {
    let high = MemoryRecord::new(
        MemoryType::Fact,
        "T".to_string(),
        "C".to_string(),
        vec![],
        42.0,
        None,
    );
    assert_eq!(high.confidence, 1.0);

    let low = MemoryRecord::new(
        MemoryType::Fact,
        "T".to_string(),
        "C".to_string(),
        vec![],
        -5.0,
        None,
    );
    assert_eq!(low.confidence, 0.0);
}

#[test]
fn func_parse_ttl_duration() {
    assert_eq!(parse_ttl_duration("500ms").unwrap(), Duration::milliseconds(500));
    assert_eq!(parse_ttl_duration("30s").unwrap(), Duration::seconds(30));
    assert_eq!(parse_ttl_duration("15m").unwrap(), Duration::minutes(15));
    assert_eq!(parse_ttl_duration("24h").unwrap(), Duration::hours(24));
    assert_eq!(parse_ttl_duration("7d").unwrap(), Duration::days(7));
    assert_eq!(parse_ttl_duration("2w").unwrap(), Duration::weeks(2));
    assert_eq!(parse_ttl_duration("60").unwrap(), Duration::seconds(60));
}

#[test]
fn integrity_parse_ttl_duration_rejects_zero_negative_and_empty() {
    assert!(parse_ttl_duration("").is_err());
    assert!(parse_ttl_duration("   ").is_err());
    assert!(parse_ttl_duration("0s").is_err());
    assert!(parse_ttl_duration("-10m").is_err());
    assert!(parse_ttl_duration("abc").is_err());
}

#[test]
fn func_extract_meaningful_tokens_and_conflict_detection() {
    let (words, all) = extract_meaningful_tokens("PostgreSQL 16.4 runs on port 5432!");
    assert!(words.contains("postgresql"));
    assert!(words.contains("port"));
    assert!(all.contains("5432"));
    assert!(!words.contains("on"));

    let mut m1 = MemoryRecord::new(
        MemoryType::Fact,
        "PG Port".to_string(),
        "PostgreSQL primary database runs on port 5432".to_string(),
        vec![],
        0.9,
        None,
    );
    m1.created_at = Utc::now() - Duration::minutes(10);

    let mut m2 = MemoryRecord::new(
        MemoryType::Fact,
        "PG Port Updated".to_string(),
        "PostgreSQL primary database runs on port 5433".to_string(),
        vec![],
        0.95,
        None,
    );
    m2.created_at = Utc::now();

    let conflict = detect_memory_conflict(&m1, &m2).expect("must detect conflict");
    assert_eq!(conflict.older_memory.id, m1.id);
    assert_eq!(conflict.newer_memory.id, m2.id);
    assert!(conflict.similarity >= 0.72);
}

#[test]
fn integrity_conflict_detection_self_id_and_unrelated_guards() {
    let m1 = MemoryRecord::new(
        MemoryType::Fact,
        "PG".to_string(),
        "PostgreSQL primary database runs on port 5432".to_string(),
        vec![],
        0.9,
        None,
    );
    // Comparing a memory against itself must always return None
    assert!(detect_memory_conflict_with_similarity(&m1, &m1, Some(1.0), 0.1).is_none());

    // Stopword-only or empty content must return None
    let empty_m = MemoryRecord::new(
        MemoryType::Fact,
        "Empty".to_string(),
        "the a an in on".to_string(),
        vec![],
        0.9,
        None,
    );
    assert!(detect_memory_conflict(&m1, &empty_m).is_none());
}

// ============================================================================
// 3. models::link & models::stats — Functionality & Integrity Tests
// ============================================================================

#[test]
fn func_memory_link_and_extract_wikilinks() {
    let link = MemoryLink::new("src-1", "dst-1", None, None);
    assert_eq!(link.relation, "relates_to");
    assert_eq!(link.weight, 1.0);

    let targets = extract_wikilinks("Connects [[Auth Service]] to [[Postgres DB|Primary DB]].");
    assert_eq!(targets, vec!["Auth Service", "Postgres DB"]);
}

#[test]
fn integrity_extract_wikilinks_malformed_and_nested_brackets() {
    assert!(extract_wikilinks("Unclosed [[link without end").is_empty());
    assert!(extract_wikilinks("Empty [[]] and [[   ]]").is_empty());
    assert_eq!(
        extract_wikilinks("Nested [[outer [[Inner Target|Alias]] text"),
        vec!["Inner Target"]
    );
}

#[test]
fn func_and_integrity_stats_and_graph_serialization() {
    let stats = StorageStats {
        total_memories: 10,
        active_memories: 8,
        expired_memories: 2,
        total_vectors: 10,
        by_category: vec![("fact".to_string(), 8), ("decision".to_string(), 2)],
    };
    let json = serde_json::to_string(&stats).unwrap();
    let roundtrip: StorageStats = serde_json::from_str(&json).unwrap();
    assert_eq!(
        roundtrip.active_memories + roundtrip.expired_memories,
        roundtrip.total_memories
    );

    let snap = GraphSnapshot {
        nodes: vec![GraphNode {
            id: "n1".to_string(),
            label: "Node 1".to_string(),
            category: "fact".to_string(),
            tags: vec!["tag1".to_string()],
            confidence: 0.9,
            degree: 1,
            snippet: "snippet".to_string(),
        }],
        edges: vec![GraphEdge {
            source: "n1".to_string(),
            target: "n2".to_string(),
            relation: "relates_to".to_string(),
            weight: 1.0,
        }],
    };
    let snap_json = serde_json::to_string(&snap).unwrap();
    let snap_rt: GraphSnapshot = serde_json::from_str(&snap_json).unwrap();
    assert_eq!(snap_rt.nodes.len(), 1);
    assert_eq!(snap_rt.edges.len(), 1);

    let page = PaginatedMemories {
        items: vec![],
        total: 0,
        limit: 20,
        offset: 0,
        page: 1,
        total_pages: 1,
        has_more: false,
    };
    assert!(!page.has_more);
}
