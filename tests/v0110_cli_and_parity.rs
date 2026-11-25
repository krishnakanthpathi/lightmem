use lightmem::{LightMem, LightMemConfig, RemoteClient};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tempfile::{tempdir, TempDir};

const BIN: &str = env!("CARGO_BIN_EXE_lmem");
const REMOTE_KEY: &str = "v0110-parity-secret-key";

fn run_lmem(cfg: &Path, db: Option<&Path>, args: &[&str]) -> Output {
    run_lmem_with_stdin(cfg, db, args, None)
}

fn run_lmem_with_stdin(
    cfg: &Path,
    db: Option<&Path>,
    args: &[&str],
    stdin_data: Option<&[u8]>,
) -> Output {
    let mut cmd = Command::new(BIN);
    cmd.env("LIGHTMEM_CONFIG_DIR", cfg)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .env_remove("LIGHTMEM_DB")
        .env_remove("LIGHTMEM_CONNECTION");
    if let Some(db_path) = db {
        cmd.arg("--db").arg(db_path);
    }
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin_data.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    let mut child = cmd.spawn().expect("spawn lmem");
    if let Some(data) = stdin_data {
        let mut stdin = child.stdin.take().expect("stdin pipe");
        stdin.write_all(data).expect("write stdin");
    }
    child.wait_with_output().expect("wait lmem")
}

fn parse_json(out: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "expected success, got status {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        stdout,
        stderr
    );
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("invalid JSON output: {e}\nstdout: {stdout}\nstderr: {stderr}");
    })
}

fn read_cfg(cfg: &Path) -> LightMemConfig {
    let raw = std::fs::read_to_string(cfg.join("config.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn seed_hash_config(cfg: &Path) {
    let hash = LightMemConfig {
        backend: "hash".to_string(),
        reranker: "top1".to_string(),
        ..Default::default()
    };
    std::fs::create_dir_all(cfg).unwrap();
    std::fs::write(
        cfg.join("config.json"),
        serde_json::to_string_pretty(&hash).unwrap(),
    )
    .unwrap();
}

struct RemoteFixture {
    _dir: TempDir,
    url: String,
    cfg_dir: std::path::PathBuf,
}

fn spawn_remote_server() -> RemoteFixture {
    let dir = tempdir().unwrap();
    let cfg_dir = dir.path().join("cfg");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    seed_hash_config(&cfg_dir);

    let db_path = dir.path().join("server.db");
    let cfg = read_cfg(&cfg_dir);
    let controller = Arc::new(LightMem::open_at(&db_path, cfg).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");

    std::thread::spawn(move || {
        lightmem::services::api_server::serve_loop(listener, controller, Some(REMOTE_KEY.into()))
            .unwrap()
    });

    // Wait briefly until listener responds on /health
    let client = RemoteClient::new(url.clone(), Some(REMOTE_KEY.into()));
    for _ in 0..20 {
        if client.health().is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }

    RemoteFixture {
        _dir: dir,
        url,
        cfg_dir,
    }
}

#[test]
fn cli_config_disambiguation_and_subcommands() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("cfg");
    let db = dir.path().join("vault.db");
    seed_hash_config(&cfg);

    // 1. config path
    let path_json = parse_json(&run_lmem(&cfg, Some(&db), &["config", "path", "--json"]));
    assert_eq!(
        Path::new(path_json["config_file"].as_str().unwrap()),
        cfg.join("config.json")
    );

    // 2. config set + get + strict validation
    parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["config", "set", "entity_extraction", "on", "--json"],
    ));
    let got = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["config", "get", "entity_extraction", "--json"],
    ));
    assert_eq!(got["value"], "on");

    let bad_set = run_lmem(
        &cfg,
        Some(&db),
        &["config", "set", "entity_extraction", "sometimes", "--json"],
    );
    assert!(!bad_set.status.success());

    // 3. --embedding-model must not overwrite reranker
    parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["config", "--embedding-model", "nomic-embed-text", "--json"],
    ));
    let after_emb = read_cfg(&cfg);
    assert_eq!(after_emb.embedding_model, "nomic-embed-text");
    assert_eq!(after_emb.reranker, "top1");

    // 4. --answer-model preserves embedding backend ("hash")
    parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["config", "--answer-model", "openai:gpt-4o-mini", "--json"],
    ));
    let after_ans = read_cfg(&cfg);
    assert_eq!(after_ans.backend, "hash");
    assert_eq!(after_ans.reranker, "openai:gpt-4o-mini");

    // 5. --api-key sets remote api_key; --model-api-key sets LLM ollama_api_key
    parse_json(&run_lmem(
        &cfg,
        None,
        &[
            "config",
            "--remote",
            "http://127.0.0.1:9999",
            "--api-key",
            "remote-only-key",
            "--model-api-key",
            "model-only-key",
            "--json",
        ],
    ));
    let after_keys = read_cfg(&cfg);
    assert_eq!(after_keys.api_key.as_deref(), Some("remote-only-key"));
    assert_eq!(after_keys.ollama_api_key.as_deref(), Some("model-only-key"));

    // Clear remote preserves model key and clears remote url/key
    parse_json(&run_lmem(
        &cfg,
        None,
        &["config", "--clear-remote", "--json"],
    ));
    let after_clear = read_cfg(&cfg);
    assert!(after_clear.remote_url.is_none());
    assert!(after_clear.api_key.is_none());
    assert_eq!(
        after_clear.ollama_api_key.as_deref(),
        Some("model-only-key")
    );

    // 6. config reset --yes
    let bad_reset = run_lmem(&cfg, Some(&db), &["config", "reset", "--json"]);
    assert!(!bad_reset.status.success());
    parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["config", "reset", "--yes", "--json"],
    ));
    let after_reset = read_cfg(&cfg);
    assert_eq!(after_reset.backend, "onnx");
}

#[test]
fn cli_stdin_session_filters_update_forget_export_import_entities_and_history() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("cfg");
    let db = dir.path().join("vault.db");
    seed_hash_config(&cfg);

    // 1. Stdin remember via '-' + standalone --session without --connection
    let m1 = parse_json(&run_lmem_with_stdin(
        &cfg,
        Some(&db),
        &[
            "--session",
            "sprint-1",
            "remember",
            "-",
            "-t",
            "fact",
            "--title",
            "Primary Port",
            "--tags",
            "db,prod",
            "-c",
            "0.70",
            "--json",
        ],
        Some(b"PostgreSQL primary listens on port 5432"),
    ));
    let m1_id = m1["id"].as_str().unwrap().to_string();

    let m2 = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &[
            "--session",
            "sprint-2",
            "remember",
            "Redis cache listens on port 6379",
            "-t",
            "fact",
            "--title",
            "Cache Port",
            "--tags",
            "cache,prod",
            "-c",
            "0.95",
            "--json",
        ],
    ));
    let m2_id = m2["id"].as_str().unwrap().to_string();

    // 2. Insert entity directly into SQLite vault so entity filtering + `entities` CLI work cleanly
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_name, entity_val, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![m1_id, "port", "5432", "2026-01-01T00:00:00Z"],
        )
        .unwrap();
    }

    // 3. `lmem entities` summary and `--memories` mode
    let ent_summary = parse_json(&run_lmem(&cfg, Some(&db), &["entities", "--json"]));
    assert!(ent_summary
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["name"] == "port" && e["val"] == "5432"));

    let ent_memories = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["entities", "port=5432", "--memories", "--json"],
    ));
    assert_eq!(ent_memories["items"].as_array().unwrap().len(), 1);
    assert_eq!(ent_memories["items"][0]["id"], m1_id);

    // 4. update: --add-tag, --remove-tag, --ttl, --clear-ttl, --confidence, --provenance
    let updated = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &[
            "update",
            &m1_id,
            "--add-tag",
            "critical",
            "--remove-tag",
            "db",
            "--confidence",
            "0.85",
            "--ttl",
            "48h",
            "--provenance",
            "validated",
            "--json",
        ],
    ));
    let tags: Vec<&str> = updated["memory"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(tags.contains(&"critical"));
    assert!(tags.contains(&"prod"));
    assert!(!tags.contains(&"db"));
    assert!(updated["memory"]["expired_at"].is_string());

    let ttl_cleared = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["update", &m1_id, "--clear-ttl", "--json"],
    ));
    assert!(ttl_cleared["memory"]["expired_at"].is_null());

    // 5. Filter + sort across list, recall, answer
    let list_sorted = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["list", "--status", "all", "--sort", "confidence", "--json"],
    ));
    let items = list_sorted["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], m2_id); // 0.95 before 0.85

    let list_sess = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &[
            "list",
            "--session",
            "sprint-1",
            "--tag",
            "critical",
            "--entity",
            "port=5432",
            "--json",
        ],
    ));
    assert_eq!(list_sess["items"].as_array().unwrap().len(), 1);
    assert_eq!(list_sess["items"][0]["id"], m1_id);

    let recall_filtered = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &[
            "recall",
            "port",
            "--session",
            "sprint-1",
            "--tag",
            "critical",
            "--entity",
            "port",
            "--json",
        ],
    ));
    assert_eq!(recall_filtered.as_array().unwrap().len(), 1);
    assert_eq!(recall_filtered[0]["memory"]["id"], m1_id);

    let ans_filtered = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &[
            "answer",
            "Which port?",
            "--session",
            "sprint-2",
            "--tag",
            "cache",
            "--json",
        ],
    ));
    assert_eq!(ans_filtered["selected_memory"]["id"], m2_id);

    // 6. Export conflicts + filtered JSON export + import conflict validation
    assert!(!run_lmem(&cfg, Some(&db), &["export", "--okf", "--bundle"])
        .status
        .success());
    assert!(
        !run_lmem(&cfg, Some(&db), &["export", "--bundle", "--tag", "prod"])
            .status
            .success()
    );

    let exported_path = dir.path().join("subset.json");
    let exp_out = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &[
            "export",
            "--format",
            "json",
            "--session",
            "sprint-1",
            "--tag",
            "critical",
            "--entity",
            "port=5432",
            "-o",
            exported_path.to_str().unwrap(),
            "--json",
        ],
    ));
    assert_eq!(exp_out["status"], "ok");
    let exported_doc: Value =
        serde_json::from_str(&std::fs::read_to_string(&exported_path).unwrap()).unwrap();
    let exported_mems = exported_doc["memories"].as_array().unwrap();
    assert_eq!(exported_mems.len(), 1);
    assert_eq!(exported_mems[0]["id"], m1_id);

    assert!(!run_lmem(
        &cfg,
        Some(&db),
        &[
            "import",
            exported_path.to_str().unwrap(),
            "--replace",
            "--merge"
        ]
    )
    .status
    .success());
    assert!(!run_lmem(
        &cfg,
        Some(&db),
        &["import", exported_path.to_str().unwrap(), "--merge"]
    )
    .status
    .success());

    // 7. Forget multi-ID & title + missing ID non-zero exit + status=all
    let missing_forget = run_lmem(&cfg, Some(&db), &["forget", "does-not-exist", "--json"]);
    assert!(!missing_forget.status.success());

    let multi_forget = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["forget", &m1_id, "Cache Port", "--json"],
    ));
    assert_eq!(multi_forget.as_array().unwrap().len(), 2);
    assert!(multi_forget
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["success"] == true));

    let list_expired = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["list", "--status", "all", "--json"],
    ));
    assert_eq!(list_expired["items"].as_array().unwrap().len(), 2);

    // 8. History records failed command errors & redacts secret flags
    let _ = run_lmem(
        &cfg,
        Some(&db),
        &[
            "config",
            "--remote-api-key",
            "topsecret123",
            "--embedding-api-key",
            "embsecret456",
            "--dry-run",
        ],
    );
    let hist = parse_json(&run_lmem(
        &cfg,
        Some(&db),
        &["history", "-l", "50", "--json"],
    ));
    let entries = hist.as_array().unwrap();
    assert!(entries
        .iter()
        .any(|e| e["status"] == "error" && e["error_message"].is_string()));
    for e in entries {
        let cmd_str = e["command"].as_str().unwrap_or("");
        assert!(!cmd_str.contains("topsecret123"));
        assert!(!cmd_str.contains("embsecret456"));
    }

    // 9. serve --status --json works without an active server
    let srv_status = parse_json(&run_lmem(&cfg, Some(&db), &["serve", "--status", "--json"]));
    assert_eq!(srv_status["running"], false);
}

#[test]
fn remote_parity_endpoints_and_cli_round_trip() {
    let srv = spawn_remote_server();

    let m1 = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "--session",
            "rem-1",
            "remember",
            "Remote Postgres uses port 5432",
            "-t",
            "fact",
            "--title",
            "Remote DB",
            "--tags",
            "db",
            "-c",
            "0.75",
            "--json",
        ],
    ));
    let m1_id = m1["id"].as_str().unwrap().to_string();

    let m2 = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "--session",
            "rem-2",
            "remember",
            "Remote Redis cache uses port 6379",
            "-t",
            "fact",
            "--title",
            "Remote Cache",
            "--tags",
            "cache",
            "-c",
            "0.92",
            "--json",
        ],
    ));
    let m2_id = m2["id"].as_str().unwrap().to_string();
    assert_ne!(m1_id, m2_id);

    // Update remotely via /v1/update
    let upd = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "update",
            &m1_id,
            "--add-tag",
            "updated-remote",
            "--ttl",
            "24h",
            "--json",
        ],
    ));
    assert!(upd["memory"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "updated-remote"));

    // tags, entities, list sort & session
    let tags = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "tags",
            "--json",
        ],
    ));
    assert!(tags
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["tag"] == "updated-remote"));

    let entities = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "entities",
            "--json",
        ],
    ));
    assert!(entities.is_array());

    let listed = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "list",
            "--status",
            "all",
            "--sort",
            "confidence",
            "--json",
        ],
    ));
    assert_eq!(listed["items"][0]["id"], m2_id);

    // deduplicate, autolink, conflicts, observe, multi-ID forget remotely
    let dedup = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "deduplicate",
            "--json",
        ],
    ));
    assert!(dedup["merged"].is_number());

    let autolink = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "autolink",
            "--json",
        ],
    ));
    assert!(autolink["total_nodes"].is_number());

    let conflicts = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "conflicts",
            "--json",
        ],
    ));
    assert!(conflicts["conflicts"].is_array());

    let obs = parse_json(&run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "observe",
            "User prefers dark terminal themes",
            "--yes",
            "--json",
        ],
    ));
    assert!(!obs.as_array().unwrap().is_empty());

    // Remote inspect on missing term must exit non-zero
    let missing_inspect = run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "inspect",
            "missing-remote-id",
            "--json",
        ],
    );
    assert!(!missing_inspect.status.success());

    // Remote clear without --yes even with --json must fail
    let bad_clear = run_lmem(
        &srv.cfg_dir,
        None,
        &[
            "--remote",
            &srv.url,
            "--api-key",
            REMOTE_KEY,
            "clear",
            "--soft",
            "--json",
        ],
    );
    assert!(!bad_clear.status.success());
}

#[test]
fn mcp_server_exposes_new_tools_and_validates_config() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("cfg");
    let db = dir.path().join("mcp.db");
    seed_hash_config(&cfg);

    let mut child = Command::new(BIN)
        .env("LIGHTMEM_CONFIG_DIR", &cfg)
        .env("LIGHTMEM_QA_DISABLE", "1")
        .env_remove("LIGHTMEM_DB")
        .env_remove("LIGHTMEM_CONNECTION")
        .arg("--db")
        .arg(&db)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    let requests = vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"config","arguments":{"backend":"invalid-backend"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"remember","arguments":{"content":"MCP test memory","tags":["mcp"],"title":"MCP Card"}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"entities","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"clear","arguments":{"soft":true,"yes":false}}}),
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"history","arguments":{"limit":5}}}),
    ];

    for req in requests {
        writeln!(stdin, "{}", req).unwrap();
    }
    drop(stdin);

    let mut responses = Vec::new();
    for _ in 0..7 {
        let mut line = String::new();
        let n = reader.read_line(&mut line).unwrap();
        assert!(n > 0, "unexpected EOF reading MCP response");
        responses.push(serde_json::from_str::<Value>(&line).unwrap());
    }
    let _ = child.wait();

    // Check tools/list includes expanded 0.11.0 tool surface
    let tool_names: Vec<&str> = responses[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for expected in [
        "update",
        "restore",
        "clear",
        "links",
        "tags",
        "entities",
        "deduplicate",
        "autolink",
        "observe",
        "export",
        "import",
        "bundle_export",
        "bundle_import",
        "bundle_info",
        "history",
    ] {
        assert!(
            tool_names.contains(&expected),
            "missing MCP tool: {expected}"
        );
    }

    // id=3: invalid config backend must return isError=true
    assert_eq!(responses[2]["result"]["isError"], true);
    // id=4: remember succeeded
    assert_ne!(responses[3]["result"]["isError"], true);
    // id=5: entities succeeded
    assert_ne!(responses[4]["result"]["isError"], true);
    // id=6: clear with yes=false must return isError=true
    assert_eq!(responses[5]["result"]["isError"], true);
    // id=7: history succeeded
    assert_ne!(responses[6]["result"]["isError"], true);
}
