use chrono::Utc;
use lightmem::models::{
    AttachmentMetadata, GraphEdge, GraphNode, GraphSnapshot, LightMemConfig, MemoryConflict,
    MemoryLink, MemoryRecord, MemoryType, PaginatedMemories, RelatedMemory, ScoredMemory,
    StorageStats,
};
use lightmem::services::{AnswerResult, ConnectResult, ObservedCandidate, PlatformInfo};
use lightmem::CliView;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn lmem_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lmem"))
}

fn run_lmem(cfg_dir: &Path, db_path: &Path, args: &[&str]) -> std::process::Output {
    Command::new(lmem_bin())
        .env("LIGHTMEM_CONFIG_DIR", cfg_dir)
        .env("LIGHTMEM_DB", db_path)
        .arg("--db")
        .arg(db_path)
        .args(args)
        .output()
        .expect("failed to execute lmem binary")
}

// ============================================================================
// 1. CLIVIEW FUNCTIONALITY & INTEGRITY TESTS
// ============================================================================

#[test]
fn func_cli_view_all_render_methods_human_and_json() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("view.db");

    // Theme helpers & banner
    let _ = CliView::crimson("c");
    let _ = CliView::crimson_bold("cb");
    let _ = CliView::rose("r");
    let _ = CliView::violet_bold("vb");
    let _ = CliView::gold("g");
    let _ = CliView::white_bold("wb");
    let _ = CliView::slate("s");
    let _ = CliView::emerald_bold("eb");

    let stats = StorageStats {
        total_memories: 2,
        active_memories: 1,
        expired_memories: 1,
        total_vectors: 1,
        by_category: vec![("fact".to_string(), 1), ("decision".to_string(), 1)],
    };
    let banner = CliView::banner_string(Some(&db_path), Some(&stats));
    assert!(banner.contains("L I G H T M E M"));
    CliView::render_intro(&db_path, &stats);
    assert!(!CliView::format_path(&db_path).is_empty());

    let mut rec = MemoryRecord::new(
        MemoryType::Fact,
        "Test View Title".to_string(),
        "Test view content body".to_string(),
        vec!["tag1".to_string()],
        0.92,
        Some("unit_test".to_string()),
    );
    rec.expired_at = Some(Utc::now());
    rec.attachments.push(AttachmentMetadata {
        id: "att-1".to_string(),
        memory_id: rec.id.clone(),
        filename: "pic.png".to_string(),
        mime_type: "image/png".to_string(),
        media_kind: "image".to_string(),
        size_bytes: 128,
        sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        dimensions: Some("16x16".to_string()),
        duration_secs: Some(1.5),
        created_at: Utc::now(),
        data_base64: None,
    });

    let conflict = MemoryConflict {
        older_memory: rec.clone(),
        newer_memory: rec.clone(),
        similarity: 0.88,
        overlap_ratio: 0.75,
    };

    for json in [false, true] {
        CliView::render_remembered(&rec, &db_path, json).unwrap();
        CliView::render_remembered_with_conflicts(
            &rec,
            std::slice::from_ref(&conflict),
            false,
            &db_path,
            json,
        )
        .unwrap();
        CliView::render_remembered_with_conflicts(
            &rec,
            std::slice::from_ref(&conflict),
            true,
            &db_path,
            json,
        )
        .unwrap();
        CliView::render_conflicts(std::slice::from_ref(&conflict), false, &db_path, json).unwrap();
        CliView::render_conflicts(std::slice::from_ref(&conflict), true, &db_path, json).unwrap();

        let scored = ScoredMemory {
            memory: rec.clone(),
            score: 0.91,
            bm25_rank: Some(1),
            vector_rank: Some(1),
        };
        CliView::render_recall("query", &[scored], &db_path, json).unwrap();

        let pag = PaginatedMemories {
            items: vec![rec.clone()],
            total: 2,
            limit: 1,
            offset: 0,
            page: 1,
            total_pages: 2,
            has_more: true,
        };
        CliView::render_paginated_list(&pag, &db_path, json).unwrap();
        CliView::render_forget(&rec.id, true, false, json).unwrap();
        CliView::render_forget(&rec.id, false, true, json).unwrap();

        let link = MemoryLink::new(
            rec.id.clone(),
            "target-12345678".to_string(),
            Some("relates_to".to_string()),
            Some(0.9),
        );
        CliView::render_inspect(&rec, &[link], &db_path, json).unwrap();
        CliView::render_dedup(0, json).unwrap();
        CliView::render_dedup(3, json).unwrap();
        CliView::render_clear(2, false, &db_path, json).unwrap();
        CliView::render_clear(2, true, &db_path, json).unwrap();

        let ans = AnswerResult {
            answer: "Extracted answer".to_string(),
            selected_memory: Some(rec.clone()),
            confidence: 0.93,
            reranker_used: "top1".to_string(),
        };
        CliView::render_answer(&ans, json).unwrap();
        CliView::render_config(&LightMemConfig::default(), true, json).unwrap();
        CliView::render_stats(&stats, &db_path, json).unwrap();
        CliView::render_link(&rec.id, "target-12345678", "relates_to", 1.0, json).unwrap();
        CliView::render_unlink(&rec.id, "target-12345678", Some("relates_to"), true, json).unwrap();
        CliView::render_unlink(&rec.id, "target-12345678", None, false, json).unwrap();

        let rel = RelatedMemory {
            memory: rec.clone(),
            distance: 1,
            relation_path: vec!["relates_to".to_string()],
            score: 0.85,
        };
        CliView::render_related("start", &[rel], json).unwrap();

        let snap = GraphSnapshot {
            nodes: vec![GraphNode {
                id: rec.id.clone(),
                label: rec.title.clone(),
                category: "fact".to_string(),
                tags: vec!["tag1".to_string()],
                degree: 1,
                confidence: 0.9,
                snippet: "snippet".to_string(),
            }],
            edges: vec![GraphEdge {
                source: rec.id.clone(),
                target: rec.id.clone(),
                relation: "relates_to".to_string(),
                weight: 1.0,
            }],
        };
        CliView::render_graph(&snap, json).unwrap();
        CliView::render_autolink(2, 5, 4, json).unwrap();
        CliView::render_autolink(0, 5, 4, json).unwrap();

        let conn_res = ConnectResult {
            platform: "claude".to_string(),
            target_file: db_path.clone(),
            created: true,
            message: "installed".to_string(),
        };
        CliView::render_connect(&[conn_res], json).unwrap();

        let plat_info = PlatformInfo {
            id: "claude".to_string(),
            name: "Claude Code".to_string(),
            target_file: db_path.clone(),
            detected: true,
        };
        CliView::render_platforms_list(&[plat_info], true, json).unwrap();

        let cand = ObservedCandidate::new(
            "Candidate Title",
            "Candidate Content",
            MemoryType::Fact,
            vec!["tag".to_string()],
            0.9,
        );
        CliView::render_observed_candidates(&[cand], true, json).unwrap();
        CliView::render_observed_summary(1, 0, false, json).unwrap();
        CliView::render_observed_summary(0, 1, true, json).unwrap();
    }

    CliView::render_export(&db_path, false, false).unwrap();
    CliView::render_export(&db_path, true, true).unwrap();
    CliView::render_import(5, &db_path, false).unwrap();
    CliView::render_import(5, &db_path, true).unwrap();
    CliView::render_version(false).unwrap();
    CliView::render_version(true).unwrap();
}

#[test]
fn integrity_cli_view_handles_empty_slices_short_ids_and_extreme_scores() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("empty_view.db");

    // Empty collections and extreme confidence values (< 0.0, > 1.0, NaN) must never panic
    CliView::render_conflicts(&[], false, &db_path, false).unwrap();
    CliView::render_recall("empty", &[], &db_path, false).unwrap();

    let empty_pag = PaginatedMemories {
        items: vec![],
        total: 0,
        limit: 10,
        offset: 0,
        page: 1,
        total_pages: 1,
        has_more: false,
    };
    CliView::render_paginated_list(&empty_pag, &db_path, false).unwrap();

    // Short IDs (< 8 chars) and short SHA-256 strings (< 12 chars) must not slice-panic
    let mut short_rec = MemoryRecord::new(
        MemoryType::Fact,
        "Short".to_string(),
        "Line1\nLine2".to_string(),
        vec![],
        1.5,
        None,
    );
    short_rec.id = "id1".to_string();
    short_rec.attachments.push(AttachmentMetadata {
        id: "a1".to_string(),
        memory_id: "id1".to_string(),
        filename: "f.bin".to_string(),
        mime_type: "application/octet-stream".to_string(),
        media_kind: "binary".to_string(),
        size_bytes: 1,
        sha256: "short".to_string(),
        dimensions: None,
        duration_secs: None,
        created_at: Utc::now(),
        data_base64: None,
    });
    let short_link = MemoryLink::new(
        "id1".to_string(),
        "id2".to_string(),
        Some("rel".to_string()),
        Some(1.0),
    );
    CliView::render_inspect(&short_rec, &[short_link], &db_path, false).unwrap();
    CliView::render_link("id1", "id2", "rel", 1.0, false).unwrap();
    CliView::render_unlink("id1", "id2", None, true, false).unwrap();
    CliView::render_related("id1", &[], false).unwrap();
    CliView::render_connect(&[], false).unwrap();
    CliView::render_observed_candidates(&[], false, false).unwrap();
}

// ============================================================================
// 2. CLI SUBCOMMANDS FUNCTIONALITY & INTEGRITY TESTS
// ============================================================================

#[test]
fn func_cli_subcommands_remember_recall_list_inspect_answer_stats() {
    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join("config");
    let db_path = dir.path().join("cli.db");

    // 1. Configure hash backend + top1 reranker via `lmem config`
    let out = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "config",
            "--backend",
            "hash",
            "--reranker",
            "top1",
            "--json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cfg_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cfg_json["backend"], "hash");

    // 2. Remember a memory with a file attachment
    let sample_file = dir.path().join("architecture.txt");
    std::fs::write(&sample_file, b"Zero-latency SQLite FTS5 + WAL architecture").unwrap();

    let out = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "remember",
            "LightMem uses SQLite WAL and FTS5 for hybrid retrieval.",
            "-f",
            sample_file.to_str().unwrap(),
            "-t",
            "decision",
            "--title",
            "SQLite WAL Decision",
            "--tags",
            "sqlite,fts5",
            "--ttl",
            "7d",
            "--json",
        ],
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mem_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let mem_id = mem_json["id"].as_str().unwrap().to_string();
    assert_eq!(mem_json["category"], "decision");
    assert_eq!(mem_json["attachments"].as_array().unwrap().len(), 1);

    // 3. Recall (JSON and `--path` attachment materialization)
    let out = run_lmem(
        &cfg_dir,
        &db_path,
        &["recall", "SQLite WAL", "--limit", "5", "--json"],
    );
    assert!(out.status.success());
    let recall_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(recall_json.as_array().unwrap().len(), 1);

    let out_path = run_lmem(&cfg_dir, &db_path, &["recall", "SQLite WAL", "--path"]);
    assert!(out_path.status.success());
    let materialized_line = String::from_utf8_lossy(&out_path.stdout).trim().to_string();
    assert!(Path::new(&materialized_line).exists());

    // 4. List (`--page 1 --limit 10 --json`)
    let out = run_lmem(
        &cfg_dir,
        &db_path,
        &["list", "--page", "1", "--limit", "10", "--json"],
    );
    assert!(out.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(list_json["total"], 1);

    // 5. Inspect (`--json` and `--save-to`)
    let out = run_lmem(&cfg_dir, &db_path, &["inspect", &mem_id, "--json"]);
    assert!(out.status.success());
    let inspect_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(inspect_json["memory"]["id"], mem_id);

    let export_att_dir = dir.path().join("saved_atts");
    let out = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "inspect",
            &mem_id,
            "--save-to",
            export_att_dir.to_str().unwrap(),
        ],
    );
    assert!(out.status.success());
    assert!(export_att_dir.join("architecture.txt").exists());

    // 6. Answer (`--reranker top1 --json`)
    let out = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "answer",
            "What does LightMem use for hybrid retrieval?",
            "--reranker",
            "top1",
            "--json",
        ],
    );
    assert!(out.status.success());
    let ans_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(ans_json["answer"].as_str().unwrap().contains("SQLite WAL"));

    // 7. Stats (`--json`)
    let out = run_lmem(&cfg_dir, &db_path, &["stats", "--json"]);
    assert!(out.status.success());
    let stats_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(stats_json["active_memories"], 1);
}

#[test]
fn func_cli_subcommands_graph_link_unlink_related_autolink_conflicts_dedup() {
    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join("config");
    let db_path = dir.path().join("graph_cli.db");

    run_lmem(
        &cfg_dir,
        &db_path,
        &["config", "--backend", "hash", "--reranker", "top1"],
    );

    let out1 = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "remember",
            "Backend API gateway runs on port 8080.",
            "-t",
            "fact",
            "--title",
            "API Gateway Port",
            "--tags",
            "gateway",
            "--json",
        ],
    );
    let m1: serde_json::Value = serde_json::from_slice(&out1.stdout).unwrap();
    let id1 = m1["id"].as_str().unwrap().to_string();

    let out2 = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "remember",
            "Auth service validates JWT tokens for the gateway.",
            "-t",
            "fact",
            "--title",
            "Auth Service JWT",
            "--tags",
            "gateway",
            "--json",
        ],
    );
    let m2: serde_json::Value = serde_json::from_slice(&out2.stdout).unwrap();
    let id2 = m2["id"].as_str().unwrap().to_string();

    // Link, Related, Graph, Autolink, Unlink
    let link_out = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "link",
            &id1,
            &id2,
            "-r",
            "depends_on",
            "-w",
            "0.9",
            "--json",
        ],
    );
    assert!(link_out.status.success());

    let rel_out = run_lmem(
        &cfg_dir,
        &db_path,
        &["related", &id1, "--hops", "2", "--json"],
    );
    assert!(rel_out.status.success());
    let rel_json: serde_json::Value = serde_json::from_slice(&rel_out.stdout).unwrap();
    assert_eq!(rel_json.as_array().unwrap().len(), 1);

    let graph_out = run_lmem(&cfg_dir, &db_path, &["graph", "--json"]);
    assert!(graph_out.status.success());
    let graph_json: serde_json::Value = serde_json::from_slice(&graph_out.stdout).unwrap();
    assert_eq!(graph_json["nodes"].as_array().unwrap().len(), 2);

    let unlink_out = run_lmem(
        &cfg_dir,
        &db_path,
        &["unlink", &id1, &id2, "-r", "depends_on", "--json"],
    );
    assert!(unlink_out.status.success());

    let autolink_out = run_lmem(&cfg_dir, &db_path, &["autolink", "--json"]);
    assert!(autolink_out.status.success());

    // Conflicts & Deduplicate
    let _ = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "remember",
            "Backend API gateway runs on port 9090.",
            "-t",
            "fact",
            "--title",
            "API Gateway Port",
            "--tags",
            "gateway",
        ],
    );
    let conf_out = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "conflicts",
            "--min-similarity",
            "0.5",
            "--yes",
            "--reranker",
            "top1",
            "--json",
        ],
    );
    assert!(conf_out.status.success());

    let dedup_out = run_lmem(&cfg_dir, &db_path, &["deduplicate", "--json"]);
    assert!(dedup_out.status.success());
}

#[test]
fn func_cli_subcommands_observe_export_import_connect_completions_reindex_forget_clear() {
    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join("config");
    let db_path = dir.path().join("ops_cli.db");

    run_lmem(
        &cfg_dir,
        &db_path,
        &["config", "--backend", "hash", "--reranker", "top1"],
    );

    // 1. Observe (--dry-run and --yes)
    let obs_dry = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "observe",
            "User: I prefer using 4 spaces for Rust indentation.\nDecision: We selected SQLite WAL mode.",
            "--dry-run",
            "--confidence",
            "0.5",
            "--reranker",
            "top1",
            "--json",
        ],
    );
    assert!(obs_dry.status.success());

    let obs_save = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "observe",
            "User: I prefer using 4 spaces for Rust indentation.\nDecision: We selected SQLite WAL mode.",
            "--yes",
            "--confidence",
            "0.5",
            "--reranker",
            "top1",
            "--json",
        ],
    );
    assert!(obs_save.status.success());

    // 2. Reindex (`--yes`)
    let reindex_out = run_lmem(&cfg_dir, &db_path, &["reindex", "--yes"]);
    assert!(reindex_out.status.success());

    // 3. Export (`--json` and `--okf`) & Import into fresh DB
    let json_backup = dir.path().join("backup.json");
    let exp_out = run_lmem(
        &cfg_dir,
        &db_path,
        &["export", "--json", "-o", json_backup.to_str().unwrap()],
    );
    assert!(exp_out.status.success());
    assert!(json_backup.exists());

    let db2_path = dir.path().join("imported.db");
    let imp_out = run_lmem(
        &cfg_dir,
        &db2_path,
        &["import", json_backup.to_str().unwrap()],
    );
    assert!(imp_out.status.success());

    // 4. Connect (`--list` and `-p <custom_dir>`)
    let conn_list = run_lmem(&cfg_dir, &db_path, &["connect", "--list", "--json"]);
    assert!(conn_list.status.success());

    let custom_skill_dir = dir.path().join("agent_skill");
    let conn_deploy = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "connect",
            "-p",
            custom_skill_dir.to_str().unwrap(),
            "--json",
        ],
    );
    assert!(conn_deploy.status.success());
    assert!(custom_skill_dir.join("lightmem/SKILL.md").exists());

    // 5. Completions (`zsh`)
    let comp_out = run_lmem(&cfg_dir, &db_path, &["completions", "zsh"]);
    assert!(comp_out.status.success());
    assert!(!comp_out.stdout.is_empty());

    // 6. Forget (`--all`) & Clear (`--json`)
    let forget_out = run_lmem(&cfg_dir, &db_path, &["forget", "--all", "--json"]);
    assert!(forget_out.status.success());

    let clear_out = run_lmem(&cfg_dir, &db_path, &["clear", "--json"]);
    assert!(clear_out.status.success());
}

#[test]
fn integrity_cli_rejects_invalid_flags_missing_targets_and_non_interactive_migration() {
    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join("config");
    let db_path = dir.path().join("err_cli.db");

    run_lmem(
        &cfg_dir,
        &db_path,
        &["config", "--backend", "hash", "--reranker", "top1"],
    );

    // 1. Blank remember without files fails with non-zero exit code
    let bad_rem = run_lmem(&cfg_dir, &db_path, &["remember", "   "]);
    assert!(!bad_rem.status.success());

    // 2. Inspect non-existent memory ID fails with non-zero exit code
    let bad_insp = run_lmem(&cfg_dir, &db_path, &["inspect", "nonexistent-uuid-9999"]);
    assert!(!bad_insp.status.success());

    // 3. Forget without ID and without --all fails with non-zero exit code
    let bad_forget = run_lmem(&cfg_dir, &db_path, &["forget"]);
    assert!(!bad_forget.status.success());

    // 4. List with --page 0 fails
    let bad_list = run_lmem(&cfg_dir, &db_path, &["list", "--page", "0"]);
    assert!(!bad_list.status.success());

    // 5. Recall with out-of-range --min-similarity fails
    let bad_recall = run_lmem(
        &cfg_dir,
        &db_path,
        &["recall", "test", "--min-similarity", "1.5"],
    );
    assert!(!bad_recall.status.success());

    // 6. Switching embedding backend on a non-empty DB without --yes in non-interactive mode must fail
    let ok_rem = run_lmem(
        &cfg_dir,
        &db_path,
        &["remember", "Existing memory in hash index"],
    );
    assert!(ok_rem.status.success());

    let bad_mig = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "config",
            "--backend",
            "ollama",
            "--model",
            "nomic-embed-text",
        ],
    );
    assert!(
        !bad_mig.status.success(),
        "Non-interactive config change requiring migration without --yes must exit non-zero"
    );
}

#[test]
fn test_new_cli_json_options_and_version() {
    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join(".lightmem");
    let db_path = cfg_dir.join("test_json.db");
    run_lmem(
        &cfg_dir,
        &db_path,
        &["config", "--backend", "hash", "--reranker", "top1"],
    );

    // 1. --version and --version --json
    let ver_plain = run_lmem(&cfg_dir, &db_path, &["--version"]);
    assert!(ver_plain.status.success());
    let ver_plain_str = String::from_utf8_lossy(&ver_plain.stdout);
    assert!(ver_plain_str.contains(&format!("lmem {}", env!("CARGO_PKG_VERSION"))));

    let ver_json = run_lmem(&cfg_dir, &db_path, &["--version", "--json"]);
    assert!(ver_json.status.success());
    let v_val: serde_json::Value = serde_json::from_slice(&ver_json.stdout).unwrap();
    assert_eq!(v_val["name"], "lightmem");
    assert_eq!(v_val["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v_val["schema_version"], 1);

    // 2. version subcommand and version --json
    let sub_ver = run_lmem(&cfg_dir, &db_path, &["version"]);
    assert!(sub_ver.status.success());
    let sub_ver_json = run_lmem(&cfg_dir, &db_path, &["version", "--json"]);
    assert!(sub_ver_json.status.success());
    let sub_val: serde_json::Value = serde_json::from_slice(&sub_ver_json.stdout).unwrap();
    assert_eq!(sub_val["version"], env!("CARGO_PKG_VERSION"));

    // 3. Export with --output-json
    let rem = run_lmem(&cfg_dir, &db_path, &["remember", "Memory to test export"]);
    assert!(rem.status.success());
    let backup_file = dir.path().join("backup.json");
    let exp_json = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "export",
            "--json",
            "-o",
            backup_file.to_str().unwrap(),
            "--output-json",
        ],
    );
    assert!(exp_json.status.success());
    let exp_val: serde_json::Value = serde_json::from_slice(&exp_json.stdout).unwrap();
    assert_eq!(exp_val["status"], "ok");
    assert_eq!(exp_val["format"], "json");

    // 4. Import with --json
    let db2_path = dir.path().join("import_json.db");
    let imp_json = run_lmem(
        &cfg_dir,
        &db2_path,
        &["import", backup_file.to_str().unwrap(), "--json"],
    );
    assert!(imp_json.status.success());
    let imp_val: serde_json::Value = serde_json::from_slice(&imp_json.stdout).unwrap();
    assert_eq!(imp_val["status"], "ok");
    assert!(imp_val["imported"].as_u64().unwrap() >= 1);

    // 5. Reindex with --json
    let reindex_json = run_lmem(&cfg_dir, &db_path, &["reindex", "--yes", "--json"]);
    assert!(reindex_json.status.success());
    let reindex_val: serde_json::Value = serde_json::from_slice(&reindex_json.stdout).unwrap();
    assert_eq!(reindex_val["status"], "ok");

    // 6. Setup with --json on empty DB
    let empty_setup_db = dir.path().join("setup_empty.db");
    let setup_json = run_lmem(
        &cfg_dir,
        &empty_setup_db,
        &[
            "setup",
            "--url",
            "http://localhost:11434",
            "--model",
            "top1",
            "--yes",
            "--json",
        ],
    );
    assert!(setup_json.status.success());
    let setup_val: serde_json::Value = serde_json::from_slice(&setup_json.stdout).unwrap();
    assert_eq!(setup_val["reranker"], "top1");

    // 7. Root --json without subcommand outputs stats
    let root_json = run_lmem(&cfg_dir, &db_path, &["--json"]);
    assert!(root_json.status.success());
    let root_val: serde_json::Value = serde_json::from_slice(&root_json.stdout).unwrap();
    assert!(root_val.get("total_memories").is_some());
}

#[test]
fn test_open_image_status_and_docs_cli_surface() {
    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join(".lightmem");
    let db_path = cfg_dir.join("surface.db");
    run_lmem(
        &cfg_dir,
        &db_path,
        &["config", "--backend", "hash", "--reranker", "top1"],
    );

    // Help flags expose --open, --image, status/vaults, docs/doc/web
    let rec_help = run_lmem(&cfg_dir, &db_path, &["recall", "--help"]);
    assert!(rec_help.status.success());
    let rec_help_str = String::from_utf8_lossy(&rec_help.stdout);
    assert!(rec_help_str.contains("--open"));
    assert!(rec_help_str.contains("--image"));

    let insp_help = run_lmem(&cfg_dir, &db_path, &["inspect", "--help"]);
    assert!(insp_help.status.success());
    assert!(String::from_utf8_lossy(&insp_help.stdout).contains("--open"));

    for alias in ["docs", "doc", "web"] {
        let docs_help = run_lmem(&cfg_dir, &db_path, &[alias, "--help"]);
        assert!(docs_help.status.success(), "{alias} --help failed");
        let s = String::from_utf8_lossy(&docs_help.stdout);
        assert!(s.contains("--port") && s.contains("--landing") && s.contains("--no-browser"));
    }

    // status and vaults aliases return valid JSON
    for cmd_name in ["status", "vaults"] {
        let st_out = run_lmem(&cfg_dir, &db_path, &[cmd_name, "--json"]);
        assert!(st_out.status.success(), "{cmd_name} --json failed");
        let v: serde_json::Value = serde_json::from_slice(&st_out.stdout).unwrap();
        assert!(v.get("default_vault").is_some());
        assert!(v.get("connections").is_some());
    }

    // recall --image with missing path exits non-zero cleanly
    let bad_img = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "recall",
            "--image",
            dir.path().join("missing.png").to_str().unwrap(),
        ],
    );
    assert!(!bad_img.status.success());

    // Text-only memory: recall --open --json & inspect --open --path do not spawn external GUI apps
    let rem = run_lmem(
        &cfg_dir,
        &db_path,
        &["remember", "Text only entry", "--no-entities", "--json"],
    );
    assert!(rem.status.success());
    let mem_id = serde_json::from_slice::<serde_json::Value>(&rem.stdout).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let rec_open = run_lmem(
        &cfg_dir,
        &db_path,
        &["recall", "Text only", "--open", "--json"],
    );
    assert!(rec_open.status.success());
    let rec_arr: Vec<serde_json::Value> = serde_json::from_slice(&rec_open.stdout).unwrap();
    assert!(rec_arr.is_empty());

    let insp_open = run_lmem(
        &cfg_dir,
        &db_path,
        &["inspect", &mem_id, "--open", "--path"],
    );
    assert!(!insp_open.status.success());
}

fn minimal_1x1_png(rgb: [u8; 3]) -> Vec<u8> {
    // Valid uncompressed zlib-stored 1x1 RGB PNG (IHDR + IDAT + IEND)
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    fn push_chunk(dst: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
        dst.extend_from_slice(&(data.len() as u32).to_be_bytes());
        dst.extend_from_slice(tag);
        dst.extend_from_slice(data);
        // CRC32 (IEEE 802.3)
        let mut crc: u32 = 0xffff_ffff;
        for &b in tag.iter().chain(data.iter()) {
            crc ^= b as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        dst.extend_from_slice(&(!crc).to_be_bytes());
    }
    // 1x1, 8-bit RGB (color type 2)
    push_chunk(&mut out, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]);
    // Raw scanline: filter 0 + R G B
    let raw = [0u8, rgb[0], rgb[1], rgb[2]];
    // Zlib stream: CMF=0x78, FLG=0x01 (no dict, FCHECK ok), BFINAL=1 non-compressed block len=4 nlen=!4
    let mut z = vec![0x78, 0x01, 0x01, 0x04, 0x00, 0xfb, 0xff];
    z.extend_from_slice(&raw);
    // Adler32 of raw
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for &b in &raw {
        s1 = (s1 + b as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    z.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
    push_chunk(&mut out, b"IDAT", &z);
    push_chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(unix)]
#[test]
fn e2e_cli_open_flag_dedup_and_extension_tags_with_os_shim() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join("cfg");
    let db_path = dir.path().join("open_e2e.db");
    let shim_bin = dir.path().join("shim_bin");
    std::fs::create_dir_all(&shim_bin).unwrap();
    let open_log = dir.path().join("open_calls.log");

    for cmd_name in ["open", "xdg-open"] {
        let shim = shim_bin.join(cmd_name);
        std::fs::write(
            &shim,
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\n",
                open_log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let orig_path = std::env::var("PATH").unwrap_or_default();
    let shim_path_env = format!("{}:{}", shim_bin.display(), orig_path);

    let run_with_shim = |args: &[&str]| {
        Command::new(lmem_bin())
            .env("LIGHTMEM_CONFIG_DIR", &cfg_dir)
            .env("LIGHTMEM_DB", &db_path)
            .env("PATH", &shim_path_env)
            .arg("--db")
            .arg(&db_path)
            .args(args)
            .output()
            .unwrap()
    };

    assert!(
        run_with_shim(&["config", "--backend", "hash", "--reranker", "top1"])
            .status
            .success()
    );

    let png_bytes = minimal_1x1_png([240, 20, 20]);
    let img1 = dir.path().join("crimson.png");
    let img2 = dir.path().join("crimson_dup.png");
    std::fs::write(&img1, &png_bytes).unwrap();
    std::fs::write(&img2, &png_bytes).unwrap();

    let r1 = run_with_shim(&[
        "remember",
        "Crimson square asset",
        "-f",
        img1.to_str().unwrap(),
        "--no-entities",
        "--json",
    ]);
    assert!(r1.status.success());
    let j1: serde_json::Value = serde_json::from_slice(&r1.stdout).unwrap();
    let mem_id = j1["id"].as_str().unwrap().to_string();
    let tags: Vec<String> = j1["tags"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t.as_str().map(String::from))
        .collect();
    assert!(tags.contains(&"png".to_string()), "tags: {:?}", tags);
    assert!(tags.contains(&"image".to_string()), "tags: {:?}", tags);

    // Duplicate upload emits dedup notice
    let r2 = run_with_shim(&[
        "remember",
        "Duplicate crimson upload",
        "-f",
        img2.to_str().unwrap(),
        "--no-entities",
    ]);
    assert!(r2.status.success());
    let dup_out = format!(
        "{}{}",
        String::from_utf8_lossy(&r2.stdout),
        String::from_utf8_lossy(&r2.stderr)
    );
    assert!(
        dup_out.to_lowercase().contains("already exists"),
        "Expected duplicate notice, got: {}",
        dup_out
    );

    // Check --open across recall, recall --path, recall --save-to, inspect, inspect --path, inspect --save-to
    let save_recall = dir.path().join("save_recall");
    let save_inspect = dir.path().join("save_inspect");
    let cases: &[(&[&str], usize)] = &[
        (&["recall", "Crimson square", "-l", "1", "--open"], 1),
        (
            &["recall", "Crimson square", "-l", "1", "--open", "--path"],
            1,
        ),
        (
            &[
                "recall",
                "Crimson square",
                "-l",
                "1",
                "--open",
                "--save-to",
                save_recall.to_str().unwrap(),
            ],
            1,
        ),
        (&["inspect", &mem_id, "--open"], 1),
        (&["inspect", &mem_id, "--open", "--path"], 1),
        (
            &[
                "inspect",
                &mem_id,
                "--open",
                "--save-to",
                save_inspect.to_str().unwrap(),
            ],
            1,
        ),
    ];

    for (args, expected_count) in cases {
        let _ = std::fs::remove_file(&open_log);
        let out = run_with_shim(args);
        assert!(
            out.status.success(),
            "args {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        // `.spawn()` executes child opener asynchronously; wait briefly for shim write.
        let mut logged = String::new();
        for _ in 0..40 {
            if let Ok(s) = std::fs::read_to_string(&open_log) {
                let count = s.lines().filter(|l| !l.trim().is_empty()).count();
                if count >= *expected_count {
                    logged = s;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let lines: Vec<&str> = logged.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            lines.len(),
            *expected_count,
            "args {:?} logged: {:?}",
            args,
            lines
        );
        for opened_path in lines {
            assert_eq!(std::fs::read(opened_path).unwrap(), png_bytes);
        }
    }
}

#[test]
fn e2e_cli_multimodal_recall_image_and_live_docs_server() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join("cfg");
    let db_path = dir.path().join("mm_e2e.db");

    let red_png = dir.path().join("red.png");
    let blue_png = dir.path().join("blue.png");
    std::fs::write(&red_png, minimal_1x1_png([255, 0, 0])).unwrap();
    std::fs::write(&blue_png, minimal_1x1_png([0, 0, 255])).unwrap();

    // Precompute red PNG prefix in base64 using known file bytes
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let red_tag_b64 = {
        // Minimal PNG shares the 1x1 IHDR, differing at the red pixel bytes around base64 offset 40+
        // Just pass distinct labels or decode full payload dynamically below!
        "red_marker"
    };
    let _ = red_tag_b64;

    let seen_img_queries = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let seen_clone = Arc::clone(&seen_img_queries);
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_srv = Arc::clone(&stop);

    // Inspect exact base64 strings directly from red.png / blue.png
    let red_bytes = std::fs::read(&red_png).unwrap();
    let blue_bytes = std::fs::read(&blue_png).unwrap();

    let srv = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        while !stop_srv.load(std::sync::atomic::Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                    let mut reader = BufReader::new(&mut stream);
                    let mut content_len = 0usize;
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0
                            || line == "\r\n"
                            || line == "\n"
                        {
                            break;
                        }
                        if let Some((k, v)) = line.split_once(':') {
                            if k.trim().eq_ignore_ascii_case("content-length") {
                                content_len = v.trim().parse().unwrap_or(0);
                            }
                        }
                    }
                    let mut body_buf = vec![0u8; content_len];
                    if content_len > 0 {
                        let _ = reader.read_exact(&mut body_buf);
                    }
                    drop(reader);
                    let req_json: serde_json::Value =
                        serde_json::from_slice(&body_buf).unwrap_or_default();
                    let vec_for_item = |item: &serde_json::Value| -> Vec<f32> {
                        if let Some(img_b64) = item.get("image").and_then(|v| v.as_str()) {
                            // Compare base64 equality against known red vs blue byte sequences
                            let is_red = img_b64 == encode_b64_for_test(&red_bytes);
                            let is_blue = img_b64 == encode_b64_for_test(&blue_bytes);
                            if is_red {
                                return vec![1.0, 0.0, 0.0, 0.0];
                            } else if is_blue {
                                return vec![0.0, 1.0, 0.0, 0.0];
                            }
                        }
                        vec![0.0, 0.0, 1.0, 0.0]
                    };
                    let embs: Vec<Vec<f32>> = match req_json.get("input") {
                        Some(serde_json::Value::Array(arr)) => {
                            arr.iter().map(vec_for_item).collect()
                        }
                        Some(obj @ serde_json::Value::Object(_)) => {
                            seen_clone.lock().unwrap().push(obj.clone());
                            vec![vec_for_item(obj)]
                        }
                        _ => vec![vec![0.0, 0.0, 1.0, 0.0]],
                    };
                    let resp_body = serde_json::json!({ "embeddings": embs }).to_string();
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resp_body.len(),
                        resp_body
                    );
                    let _ = stream.write_all(resp.as_bytes());
                    let _ = stream.flush();
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
    });

    assert!(run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "config",
            "--backend",
            "ollama",
            "--url",
            &format!("http://127.0.0.1:{}", port),
            "--model",
            "embeddinggemma-2",
            "--reranker",
            "top1",
        ],
    )
    .status
    .success());

    let rem_red = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "remember",
            "Sample alpha",
            "-f",
            red_png.to_str().unwrap(),
            "--no-entities",
            "--json",
        ],
    );
    assert!(rem_red.status.success());
    let red_id = serde_json::from_slice::<serde_json::Value>(&rem_red.stdout).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let rem_blue = run_lmem(
        &cfg_dir,
        &db_path,
        &[
            "remember",
            "Sample beta",
            "-f",
            blue_png.to_str().unwrap(),
            "--no-entities",
            "--json",
        ],
    );
    assert!(rem_blue.status.success());

    let rec = run_lmem(
        &cfg_dir,
        &db_path,
        &["recall", "--image", red_png.to_str().unwrap(), "--json"],
    );
    assert!(
        rec.status.success(),
        "recall --image failed: {}",
        String::from_utf8_lossy(&rec.stderr)
    );
    let hits: Vec<serde_json::Value> = serde_json::from_slice(&rec.stdout).unwrap();
    assert_eq!(hits[0]["memory"]["id"].as_str().unwrap(), red_id);
    assert!(!seen_img_queries.lock().unwrap().is_empty());

    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    srv.join().unwrap();

    // Live `lmem docs` child process HTTP E2E
    let docs_probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let docs_port = docs_probe.local_addr().unwrap().port();
    drop(docs_probe);

    let mut docs_child = Command::new(lmem_bin())
        .env("LIGHTMEM_CONFIG_DIR", &cfg_dir)
        .args(["docs", "--port", &docs_port.to_string(), "--no-browser"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let addr = format!("127.0.0.1:{}", docs_port);
    let mut connected = None;
    for _ in 0..40 {
        if let Ok(stream) = std::net::TcpStream::connect(&addr) {
            connected = Some(stream);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let mut s = connected.expect("lmem docs server did not bind in time");
    let big_hdr = "X".repeat(24_000);
    write!(
        s,
        "GET /docs HTTP/1.1\r\nHost: {}\r\nX-Big: {}\r\nConnection: close\r\n\r\n",
        addr, big_hdr
    )
    .unwrap();
    s.flush().unwrap();
    let mut resp = String::new();
    s.read_to_string(&mut resp).unwrap();
    assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(resp.contains("LightMem"));

    let _ = docs_child.kill();
    let _ = docs_child.wait();
}

fn encode_b64_for_test(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
