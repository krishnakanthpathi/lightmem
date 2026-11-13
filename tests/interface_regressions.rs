use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn setup(root: &std::path::Path) {
    std::fs::create_dir(root.join("config")).unwrap();
    std::fs::write(
        root.join("config/config.json"),
        json!({
            "backend":"hash", "onnx_model":null, "reranker":"top1",
            "ollama_url":"http://localhost:11434", "embedding_model":"test"
        })
        .to_string(),
    )
    .unwrap();
}

fn cli(root: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lmem"));
    cmd.current_dir(root)
        .env("LIGHTMEM_CONFIG_DIR", root.join("config"))
        .env("LIGHTMEM_QA_DISABLE", "1")
        .env("LIGHTMEM_OFFLINE", "1")
        .env_remove("LIGHTMEM_DB");
    cmd
}

#[test]
fn config_reports_explicit_environment_and_global_database() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    setup(root);
    std::fs::create_dir(root.join(".lmem")).unwrap();
    std::fs::write(root.join(".lmem/memory.db"), "").unwrap();
    let explicit = root.join("explicit.db");
    let env_db = root.join("env.db");
    for (args, env, expected) in [
        (
            vec!["--db", explicit.to_str().unwrap()],
            Some(&env_db),
            explicit.clone(),
        ),
        (vec![], Some(&env_db), env_db.clone()),
        (vec!["--global"], None, root.join("config/memories.db")),
    ] {
        let mut cmd = cli(root);
        cmd.args(args).args(["config", "--json"]);
        if let Some(path) = env {
            cmd.env("LIGHTMEM_DB", path);
        }
        let out = cmd.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(value["active_db"], expected.to_string_lossy().as_ref());
    }
}

#[test]
fn mcp_wire_validates_envelopes_and_preserves_notification_semantics() {
    let dir = tempdir().unwrap();
    setup(dir.path());
    let input = [
        "{", // Syntax error, not an invalid request.
        r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":true,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":[],"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":7}"#,
        "null", // Not an object.
        r#"{"jsonrpc":"2.0"}"#, // Missing method.
        r#"{"jsonrpc":"2.0","method":"ping"}"#, // Notification (no id).
        r#"{"jsonrpc":"2.0","method":"unknown"}"#, // Notification missing method.
        r#"{"jsonrpc":"2.0","method":"ping","params":1}"#, // Notification with invalid params.
        r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#, // Valid request with null id.
        r#"{"jsonrpc":"2.0","id":"valid","method":"ping"}"#, // Valid request.
        r#"{"jsonrpc":"2.0","id":8,"method":"ping","params":{}}"#, // Invalid params (object where none expected).
        r#"{"jsonrpc":"2.0","id":9,"method":"unknown"}"#, // Unknown method.
        // Notification remember (no response expected).
        r#"{"jsonrpc":"2.0","method":"tools/call","params":{"name":"remember","arguments":{"content":"notification side effect","category":"fact"}}}"#,
        // Followed by a recall that should see the remembered content.
        r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"recall","arguments":{"query":"notification side effect"}}}"#,
    ].join("\n") + "\n";
    let mut child = cli(dir.path())
        .arg("--db")
        .arg(dir.path().join("memory.db"))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let responses: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    // Parse error for "{".
    assert_eq!(responses[0]["error"]["code"], -32700);
    // jsonrpc 1.0 -> invalid request.
    assert_eq!(responses[1]["error"]["code"], -32600);
    // id: true -> invalid request.
    assert_eq!(responses[2]["error"]["code"], -32600);
    // id: [] -> invalid request.
    assert_eq!(responses[3]["error"]["code"], -32600);
    // method: 7 -> invalid request.
    assert_eq!(responses[4]["error"]["code"], -32600);
    // "null" is a parse error.
    assert_eq!(responses[5]["error"]["code"], -32700);
    // responses[6] should be the notification handling result (id: null).
    assert_eq!(responses[6]["id"], Value::Null);
    // Notification returns an empty result.
    assert_eq!(responses[6]["result"], json!({}));
    // id: "valid", valid ping.
    assert_eq!(responses[7]["id"], "valid");
    assert_eq!(responses[7]["result"], json!({}));
    // id:8, invalid params -> ping ignores params.
    assert_eq!(responses[8]["id"], 8);
    assert_eq!(responses[8]["result"], json!({}));
    // id:9, method:unknown -> error -32601.
    assert_eq!(responses[9]["id"], 9);
    assert_eq!(responses[9]["error"]["code"], -32601);
    // Final recall sees the notification side-effect remember.
    assert_eq!(responses[10]["id"], 10);
    assert_eq!(responses[10]["result"]["isError"], false);
    let recall_text = responses[10]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(recall_text.contains("notification side effect"));
    assert!(recall_text.contains("\"count\": 1"));
}
