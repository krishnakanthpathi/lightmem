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

    CliView::render_export(&db_path);
    CliView::render_import(5, &db_path);
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
