use lightmem::{IdentityRegistry, LightMemConfig, MemoryScope, Storage};
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir;

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lmem"))
        .current_dir(root)
        .env("LIGHTMEM_CONFIG_DIR", root.join("config"))
        .env("LIGHTMEM_OFFLINE", "1")
        .env_remove("LIGHTMEM_DB")
        .env_remove("LIGHTMEM_CONNECTION")
        .args(args)
        .output()
        .unwrap()
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let out = cli(root, args);
    assert!(
        out.status.success(),
        "{:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn setup(root: &Path) {
    std::fs::create_dir_all(root.join("config")).unwrap();
    let config = LightMemConfig {
        backend: "hash".into(),
        reranker: "top1".into(),
        ..Default::default()
    };
    std::fs::write(
        root.join("config/config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    ok(root, &["identity", "user-add", "alice", "--json"]);
    ok(
        root,
        &[
            "identity",
            "agent-add",
            "builder",
            "--user",
            "alice",
            "--json",
        ],
    );
    ok(
        root,
        &[
            "identity",
            "agent-add",
            "reviewer",
            "--user",
            "alice",
            "--json",
        ],
    );
    ok(
        root,
        &[
            "identity",
            "connection-add",
            "agy",
            "--user",
            "alice",
            "--agent",
            "builder",
            "--harness",
            "antigravity",
            "--project",
            "app",
            "--memory-scope",
            "project",
            "--json",
        ],
    );
    ok(
        root,
        &[
            "identity",
            "connection-add",
            "codex",
            "--user",
            "alice",
            "--agent",
            "reviewer",
            "--harness",
            "codex",
            "--project",
            "app",
            "--memory-scope",
            "project",
            "--json",
        ],
    );
}

#[test]
fn registry_scope_keys_and_registration_validation() {
    let dir = tempdir().unwrap();
    let registry = IdentityRegistry::open(dir.path().into()).unwrap();
    for user in ["Alice", "alice", "bob"] {
        registry.add_user(user, None).unwrap();
    }
    registry.add_agent("builder", "alice", None).unwrap();
    registry.add_agent("reviewer", "alice", None).unwrap();
    registry.add_agent("bob-agent", "bob", None).unwrap();
    registry.add_agent("upper", "Alice", None).unwrap();
    registry
        .add_connection(
            "a",
            "alice",
            "builder",
            "antigravity",
            Some("app".into()),
            MemoryScope::Project,
        )
        .unwrap();
    registry
        .add_connection(
            "b",
            "alice",
            "reviewer",
            "codex",
            Some("app".into()),
            MemoryScope::Project,
        )
        .unwrap();
    registry
        .add_connection(
            "other",
            "bob",
            "bob-agent",
            "codex",
            Some("app".into()),
            MemoryScope::Project,
        )
        .unwrap();
    registry
        .add_connection(
            "upper",
            "Alice",
            "upper",
            "codex",
            Some("app".into()),
            MemoryScope::Project,
        )
        .unwrap();
    let a = registry.resolve("a", None, None).unwrap();
    assert_eq!(
        a.db_path,
        registry.resolve("b", None, None).unwrap().db_path
    );
    assert_ne!(
        a.db_path,
        registry.resolve("other", None, None).unwrap().db_path
    );
    assert_ne!(
        a.db_path,
        registry.resolve("upper", None, None).unwrap().db_path
    );
    assert_ne!(
        registry
            .resolve("a", Some(MemoryScope::Agent), None)
            .unwrap()
            .db_path,
        registry
            .resolve("b", Some(MemoryScope::Agent), None)
            .unwrap()
            .db_path
    );
    assert_eq!(
        registry
            .resolve("a", Some(MemoryScope::User), None)
            .unwrap()
            .db_path,
        registry
            .resolve("b", Some(MemoryScope::User), None)
            .unwrap()
            .db_path
    );
    assert!(registry
        .add_connection("bad", "bob", "builder", "codex", None, MemoryScope::User)
        .is_err());
    assert!(registry
        .add_connection(
            "bad",
            "alice",
            "builder",
            "missing",
            None,
            MemoryScope::User
        )
        .is_err());
    assert!(registry
        .add_connection(
            "bad",
            "alice",
            "builder",
            "codex",
            None,
            MemoryScope::Project
        )
        .is_err());
    assert!(registry.add_user("../escape", None).is_err());
    assert!(registry.add_user("alice", None).is_err());
    assert!(registry
        .resolve("a", None, Some("../session".into()))
        .is_err());
    assert_eq!(
        registry
            .list("connections")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn cli_shared_decisions_private_notes_and_source_roundtrip() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let memory = ok(
        root,
        &[
            "--connection",
            "agy",
            "--session",
            "chat-123",
            "remember",
            "Decision: use SQLite WAL for persistence",
            "--json",
        ],
    );
    let id = memory["id"].as_str().unwrap();
    assert_eq!(memory["context"]["session_id"], "chat-123");
    assert_eq!(memory["context"]["harness_id"], "antigravity");
    let fetched = ok(root, &["--connection", "codex", "get", id, "--json"]);
    assert_eq!(fetched["memory"]["context"], memory["context"]);
    let recalled = ok(
        root,
        &["--connection", "codex", "recall", "SQLite WAL", "--json"],
    );
    assert!(!recalled.as_array().unwrap().is_empty());
    let private = ok(
        root,
        &[
            "--connection",
            "agy",
            "--scope",
            "agent",
            "remember",
            "Private builder checkpoint",
            "--json",
        ],
    );
    let private_id = private["id"].as_str().unwrap();
    assert!(!cli(
        root,
        &[
            "--connection",
            "codex",
            "--scope",
            "agent",
            "get",
            private_id,
            "--json"
        ]
    )
    .status
    .success());
    assert!(!cli(
        root,
        &["--connection", "codex", "get", private_id, "--json"]
    )
    .status
    .success());
    assert!(!cli(
        root,
        &["--connection", "codex", "link", id, private_id, "--json"]
    )
    .status
    .success());
    let stats = ok(root, &["--connection", "codex", "stats", "--json"]);
    assert_eq!(stats["total_memories"], 1);
    let backup = root.join("backup.json");
    ok(
        root,
        &[
            "--connection",
            "agy",
            "export",
            "--output",
            backup.to_str().unwrap(),
            "--json",
            "--output-json",
        ],
    );
    ok(
        root,
        &[
            "--db",
            root.join("import.db").to_str().unwrap(),
            "import",
            backup.to_str().unwrap(),
            "--json",
        ],
    );
    let imported = ok(
        root,
        &[
            "--db",
            root.join("import.db").to_str().unwrap(),
            "get",
            id,
            "--json",
        ],
    );
    assert_eq!(imported["memory"]["context"], memory["context"]);
    for flags in [vec!["--db", "wrong.db"], vec!["--global"]] {
        let mut args = vec!["--connection", "agy"];
        args.extend(flags);
        args.extend(["stats", "--json"]);
        assert!(!cli(root, &args).status.success());
    }
    assert!(!root.join("wrong.db").exists());
    assert!(!cli(root, &["--scope", "user", "stats", "--json"])
        .status
        .success());
    let env = Command::new(env!("CARGO_BIN_EXE_lmem"))
        .env("LIGHTMEM_CONFIG_DIR", root.join("config"))
        .env("LIGHTMEM_CONNECTION", "codex")
        .env_remove("LIGHTMEM_DB")
        .args(["context", "--json"])
        .output()
        .unwrap();
    assert!(env.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&env.stdout).unwrap()["connection_id"],
        "codex"
    );
}

#[test]
fn custom_harness_skill_and_bound_mcp_configuration() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    setup(root);
    ok(
        root,
        &[
            "identity",
            "harness-add",
            "my-harness",
            "--path",
            "custom-skills",
            "--json",
        ],
    );
    ok(
        root,
        &[
            "identity",
            "connection-add",
            "custom",
            "--user",
            "alice",
            "--agent",
            "builder",
            "--harness",
            "my-harness",
            "--json",
        ],
    );
    ok(root, &["--connection", "custom", "connect", "--json"]);
    let content = std::fs::read_to_string(root.join("custom-skills/lightmem/SKILL.md")).unwrap();
    assert!(content.contains("lmem --connection custom --scope user remember"));
    ok(
        root,
        &[
            "--connection",
            "agy",
            "connect",
            "--mcp",
            "--workspace",
            "--json",
        ],
    );
    let path = root.join(".gemini/mcp_config.json");
    let config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        config["mcpServers"]["lightmem"]["env"]["LIGHTMEM_CONFIG_DIR"],
        root.join("config").to_string_lossy().as_ref()
    );
    assert_eq!(
        config["mcpServers"]["lightmem"]["args"],
        json!(["--connection", "agy", "--scope", "project", "mcp"])
    );
    std::fs::write(&path, "{invalid").unwrap();
    assert!(!cli(
        root,
        &[
            "--connection",
            "agy",
            "connect",
            "--mcp",
            "--workspace",
            "--json"
        ]
    )
    .status
    .success());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "{invalid");
}

#[cfg(feature = "mcp")]
#[test]
fn mcp_uses_bound_identity_and_records_session() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let mut child = Command::new(env!("CARGO_BIN_EXE_lmem"))
        .env("LIGHTMEM_CONFIG_DIR", root.join("config"))
        .env_remove("LIGHTMEM_DB")
        .env_remove("LIGHTMEM_CONNECTION")
        .args(["--connection", "agy", "--session", "mcp-chat", "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for request in [
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"context","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"remember","arguments":{"content":"Decision: MCP shares this project","user_id":"spoofed", "session_id":"per-call-chat"}}}),
        ] {
            writeln!(input, "{request}").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for response in &responses {
        assert_eq!(response["result"]["isError"], false, "{response}");
    }
    let context: Value = serde_json::from_str(
        responses[0]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(context["user_id"], "alice");
    let memory: Value = serde_json::from_str(
        responses[1]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(memory["memory"]["context"]["session_id"], "per-call-chat");
    assert_eq!(memory["memory"]["context"]["user_id"], "alice");
}

#[test]
fn schema_v4_migration_keeps_legacy_memories_unattributed() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("legacy.db");
    let storage = Storage::open(&path).unwrap();
    let memory = lightmem::MemoryRecord::new(
        lightmem::MemoryType::Decision,
        "Legacy decision".into(),
        "Keep existing SQLite data".into(),
        vec!["legacy".into()],
        0.95,
        None,
    );
    storage.insert_memory(&memory, Some(&[1.0, 0.0])).unwrap();
    drop(storage);
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("ALTER TABLE memories DROP COLUMN context_json", [])
        .unwrap();
    conn.pragma_update(None, "user_version", 4).unwrap();
    drop(conn);
    let storage = Storage::open(&path).unwrap();
    assert_eq!(storage.count_memories(None, None, None).unwrap(), 1);
    let legacy = storage.get_memory(&memory.id).unwrap().unwrap();
    assert_eq!(legacy.content, memory.content);
    assert!(legacy.context.is_none());
    drop(storage);
    let conn = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
}
