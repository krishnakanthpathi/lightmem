use crate::controllers::LightMem;
use crate::models::{parse_ttl_duration, LightMemConfig, MemoryType};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

pub struct McpServer {
    global: bool,
    db_path: Option<PathBuf>,
}

impl McpServer {
    pub fn new(global: bool, db_path: Option<PathBuf>) -> Self {
        Self { global, db_path }
    }

    fn open_controller(&self) -> Result<LightMem> {
        if let Some(ref path) = self.db_path {
            let config = LightMemConfig::try_load().unwrap_or_default();
            LightMem::open_at(path, config)
        } else {
            LightMem::open_default(self.global)
        }
    }

    pub fn run_stdio(&self) -> Result<()> {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut reader = std::io::BufReader::new(stdin.lock());
        let mut writer = stdout.lock();

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                line.clear();
                continue;
            }

            let request: Result<JsonRpcRequest, _> = serde_json::from_str(trimmed);
            match request {
                Ok(req) => {
                    if let Some(resp) = self.handle_request(req) {
                        let serialized = serde_json::to_string(&resp)?;
                        writer.write_all(serialized.as_bytes())?;
                        writer.write_all(b"\n")?;
                        writer.flush()?;
                    }
                }
                Err(err) => {
                    let err_resp = JsonRpcResponse {
                        jsonrpc: "2.0",
                        id: Value::Null,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {}", err),
                            data: None,
                        }),
                    };
                    let serialized = serde_json::to_string(&err_resp)?;
                    writer.write_all(serialized.as_bytes())?;
                    writer.write_all(b"\n")?;
                    writer.flush()?;
                }
            }
            line.clear();
        }

        Ok(())
    }

    fn handle_request(&self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = match req.id {
            Some(id) => id,
            None => {
                // Notifications don't receive responses
                return None;
            }
        };

        match req.method.as_str() {
            "initialize" => {
                let result = json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {}
                    },
                    "serverInfo": {
                        "name": "lightmem",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                });
                Some(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(result),
                    error: None,
                })
            }
            "ping" => Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(json!({})),
                error: None,
            }),
            "tools/list" => {
                let tools = self.list_tools();
                Some(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({ "tools": tools })),
                    error: None,
                })
            }
            "tools/call" => {
                let params = req.params.unwrap_or(Value::Null);
                let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                let tool_result = self.call_tool(tool_name, arguments);
                let (content, is_error) = match tool_result {
                    Ok(val) => {
                        let text = if val.is_string() {
                            val.as_str().unwrap().to_string()
                        } else {
                            serde_json::to_string_pretty(&val).unwrap_or_else(|_| val.to_string())
                        };
                        (vec![json!({ "type": "text", "text": text })], false)
                    }
                    Err(err) => (
                        vec![json!({ "type": "text", "text": format!("Error: {:#}", err) })],
                        true,
                    ),
                };

                Some(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: Some(json!({
                        "content": content,
                        "isError": is_error
                    })),
                    error: None,
                })
            }
            other => Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", other),
                    data: None,
                }),
            }),
        }
    }

    fn list_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "remember",
                "description": "Store a memory, fact, preference, decision, instruction, or attached file/image into persistent SQLite memory",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "Textual content of the memory (optional when attaching files)"
                        },
                        "category": {
                            "type": "string",
                            "description": "Category: fact, decision, instruction, preference, learning, goal, commitment, artifact, event, relationship, observation, error, context, password",
                            "enum": [
                                "fact", "decision", "instruction", "preference", "learning",
                                "goal", "commitment", "artifact", "event", "relationship",
                                "observation", "error", "context", "password"
                            ]
                        },
                        "title": {
                            "type": "string",
                            "description": "Short memory title (defaults to first line or filename)"
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tags to classify and index this memory"
                        },
                        "confidence": {
                            "type": "number",
                            "description": "Confidence score between 0.0 and 1.0 (default: 0.9)"
                        },
                        "files": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "One or more file paths to attach as binary BLOBs"
                        },
                        "extract": {
                            "type": "boolean",
                            "description": "Invoke configured LLM to extract text/descriptions from attached files (default: false)"
                        },
                        "ttl": {
                            "type": "string",
                            "description": "Auto-expire duration, e.g. '1h', '24h', '7d', '30d'"
                        },
                        "supersede": {
                            "type": "boolean",
                            "description": "Automatically supersede and merge older conflicting memories (default: false)"
                        },
                        "provenance": {
                            "type": "string",
                            "description": "Origin or verification level (e.g. 'explicit_statement', 'validated', 'observed')"
                        }
                    }
                }
            }),
            json!({
                "name": "recall",
                "description": "Recall memories via hybrid semantic vector + FTS5 BM25 search with precision reranking and optional graph expansion",
                "inputSchema": {
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query string"
                        },
                        "category": {
                            "type": "string",
                            "description": "Filter by category: fact, decision, instruction, preference, learning, etc."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Max results to return (default: 10)"
                        },
                        "min_similarity": {
                            "type": "number",
                            "description": "Minimum similarity threshold (0.0 to 1.0)"
                        },
                        "precision": {
                            "type": "boolean",
                            "description": "Run precision QA reranker on recalled memories"
                        },
                        "multi_hop": {
                            "type": "boolean",
                            "description": "Expand results with 1-hop connected neighbors in the knowledge graph"
                        },
                        "as_of": {
                            "type": "string",
                            "description": "Point-in-time recall: what was active as of date (YYYY-MM-DD or RFC3339)"
                        },
                        "date": {
                            "type": "string",
                            "description": "Filter to memories from a specific calendar day (YYYY-MM-DD)"
                        }
                    }
                }
            }),
            json!({
                "name": "answer",
                "description": "Ask a question and synthesize/extract the factual answer from stored memories using local ONNX Extractive QA or Ollama",
                "inputSchema": {
                    "type": "object",
                    "required": ["question"],
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question to answer"
                        },
                        "category": {
                            "type": "string",
                            "description": "Filter candidate memories by category"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Max candidate memories to retrieve for reranking (default: 10)"
                        },
                        "precision": {
                            "type": "boolean",
                            "description": "Force use of precision extractive QA reranker"
                        },
                        "reranker": {
                            "type": "string",
                            "description": "Override reranker mode ('onnx', 'ollama', 'ollama:<model>', or 'top1')"
                        }
                    }
                }
            }),
            json!({
                "name": "graph_related",
                "description": "Traverse and list multi-hop connected memories from a starting node in the knowledge graph",
                "inputSchema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Starting memory ID, title, or prefix"
                        },
                        "hops": {
                            "type": "integer",
                            "description": "Maximum hop distance to traverse (1 to 5, default: 2)"
                        }
                    }
                }
            }),
            json!({
                "name": "conflicts",
                "description": "Find overlapping or conflicting memories via vector similarity + token overlap, and optionally merge them",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "min_similarity": {
                            "type": "number",
                            "description": "Minimum similarity threshold to flag as conflicting (default: 0.78)"
                        },
                        "resolve": {
                            "type": "boolean",
                            "description": "Auto-merge detected conflict pairs using the reranker (default: false)"
                        }
                    }
                }
            }),
            json!({
                "name": "config",
                "description": "Inspect or update LightMem runtime configuration (backend engine, ONNX embedding model, reranker, Ollama URL, active database path)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "backend": {
                            "type": "string",
                            "description": "Set backend engine: 'onnx', 'ollama', or 'hash'"
                        },
                        "onnx_model": {
                            "type": "string",
                            "description": "ONNX embedding model ('bge-small', 'minilm', 'nomic')"
                        },
                        "reranker": {
                            "type": "string",
                            "description": "Reranker model/engine ('minilm-squad2', 'tinyroberta-squad2', 'onnx', 'ollama', 'top1')"
                        },
                        "url": {
                            "type": "string",
                            "description": "Ollama server URL (e.g. http://localhost:11434)"
                        },
                        "preset": {
                            "type": "string",
                            "description": "Apply setup preset: 'local', 'ollama-cloud', or 'ollama-local'"
                        }
                    }
                }
            }),
            json!({
                "name": "forget",
                "description": "Forget / soft-retire or permanently delete a memory by ID or title",
                "inputSchema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Memory ID, title, or prefix"
                        },
                        "hard": {
                            "type": "boolean",
                            "description": "Permanently delete instead of soft-retiring (default: false)"
                        }
                    }
                }
            }),
            json!({
                "name": "stats",
                "description": "Retrieve storage statistics, active database path, memory counts, and embedding status",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }),
        ]
    }

    fn call_tool(&self, name: &str, args: Value) -> Result<Value> {
        let lm = self.open_controller()?;

        match name {
            "remember" => {
                let content = args.get("content").and_then(|v| v.as_str());
                let category = args
                    .get("category")
                    .and_then(|v| v.as_str())
                    .and_then(|s| MemoryType::from_str(s).ok());
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let tags = match args.get("tags") {
                    Some(Value::Array(arr)) => arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect(),
                    Some(Value::String(s)) => s
                        .split(',')
                        .map(|item| item.trim().to_string())
                        .filter(|item| !item.is_empty())
                        .collect(),
                    _ => Vec::new(),
                };
                let confidence = args
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32);
                let files: Vec<PathBuf> = match args.get("files") {
                    Some(Value::Array(arr)) => arr
                        .iter()
                        .filter_map(|v| v.as_str().map(PathBuf::from))
                        .collect(),
                    _ => Vec::new(),
                };
                let extract = args
                    .get("extract")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let ttl = args
                    .get("ttl")
                    .and_then(|v| v.as_str())
                    .and_then(|s| parse_ttl_duration(s).ok());
                let supersede = args
                    .get("supersede")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let provenance = args
                    .get("provenance")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let (record, conflicts) = lm.remember_with_attachments(
                    content, &files, extract, None, category, title, tags, confidence, ttl,
                    supersede, provenance,
                )?;

                Ok(json!({
                    "status": "stored",
                    "memory": record,
                    "conflicts_detected": conflicts.len(),
                    "superseded": supersede && !conflicts.is_empty()
                }))
            }

            "recall" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: query")?;
                let category = args
                    .get("category")
                    .and_then(|v| v.as_str())
                    .and_then(|s| MemoryType::from_str(s).ok());
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                let min_sim = args
                    .get("min_similarity")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32);
                let precision = args
                    .get("precision")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let multi_hop = args
                    .get("multi_hop")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let as_of = args
                    .get("as_of")
                    .and_then(|v| v.as_str())
                    .and_then(|s| parse_as_of_date(s).ok());
                let on_date = args
                    .get("date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok());

                let results =
                    lm.recall_expanded(query, category, as_of, on_date, limit, min_sim, multi_hop)?;

                let ranked = if precision && !results.is_empty() {
                    let ans = lm.answer_with_options(
                        query,
                        category,
                        as_of,
                        on_date,
                        limit,
                        Some("onnx"),
                    )?;
                    let mut res = results;
                    if let Some(ref selected) = ans.selected_memory {
                        if let Some(idx) = res.iter().position(|r| r.memory.id == selected.id) {
                            let mut item = res.remove(idx);
                            item.score = 1.0;
                            res.insert(0, item);
                        }
                    }
                    res
                } else {
                    results
                };

                let items: Vec<Value> = ranked
                    .into_iter()
                    .map(|r| {
                        json!({
                            "id": r.memory.id,
                            "title": r.memory.title,
                            "content": r.memory.content,
                            "category": r.memory.category,
                            "score": r.score,
                            "tags": r.memory.tags,
                            "created_at": r.memory.created_at,
                            "attachments_count": r.memory.attachments.len()
                        })
                    })
                    .collect();

                Ok(json!({
                    "query": query,
                    "count": items.len(),
                    "results": items
                }))
            }

            "answer" => {
                let question = args
                    .get("question")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: question")?;
                let category = args
                    .get("category")
                    .and_then(|v| v.as_str())
                    .and_then(|s| MemoryType::from_str(s).ok());
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                let reranker_override = args.get("reranker").and_then(|v| v.as_str());
                let precision = args
                    .get("precision")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let res = if precision || reranker_override.is_some() {
                    let effective_r =
                        reranker_override.or(if precision { Some("onnx") } else { None });
                    lm.answer_with_options(question, category, None, None, limit, effective_r)?
                } else {
                    lm.answer(question, category, None, limit, false)?
                };

                Ok(json!({
                    "question": question,
                    "answer": res.answer,
                    "confidence": res.confidence,
                    "reranker_used": res.reranker_used,
                    "selected_memory_id": res.selected_memory.as_ref().map(|m| &m.id)
                }))
            }

            "graph_related" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: id")?;
                let hops = args.get("hops").and_then(|v| v.as_u64()).unwrap_or(2) as usize;

                let related = lm.related(id, hops)?;
                Ok(json!({
                    "origin_id": id,
                    "hops": hops,
                    "count": related.len(),
                    "related": related
                }))
            }

            "conflicts" => {
                let min_sim = args
                    .get("min_similarity")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32)
                    .unwrap_or(0.78);
                let resolve = args
                    .get("resolve")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let conflicts = lm.find_conflicts_with_options(resolve, min_sim, None)?;
                Ok(json!({
                    "threshold": min_sim,
                    "resolved": resolve,
                    "count": conflicts.len(),
                    "conflicts": conflicts
                }))
            }

            "config" => {
                let mut cfg = LightMemConfig::try_load().unwrap_or_default();
                let mut updated = false;

                if let Some(backend) = args.get("backend").and_then(|v| v.as_str()) {
                    cfg.backend = backend.to_string();
                    updated = true;
                }
                if let Some(onnx_m) = args.get("onnx_model").and_then(|v| v.as_str()) {
                    cfg.onnx_model = Some(onnx_m.to_string());
                    updated = true;
                }
                if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
                    cfg.ollama_url = LightMemConfig::normalize_ollama_url(url);
                    updated = true;
                }
                if let Some(r) = args.get("reranker").and_then(|v| v.as_str()) {
                    cfg.reranker = LightMemConfig::normalize_reranker(r, None);
                    updated = true;
                }
                if let Some(preset) = args.get("preset").and_then(|v| v.as_str()) {
                    cfg.apply_preset(preset)?;
                    updated = true;
                }

                if updated {
                    cfg.save()?;
                }

                Ok(json!({
                    "updated": updated,
                    "backend": cfg.backend,
                    "onnx_model": cfg.onnx_model,
                    "reranker": cfg.reranker,
                    "ollama_url": cfg.ollama_url,
                    "active_db": lm.db_path().display().to_string()
                }))
            }

            "forget" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: id")?;
                let hard = args.get("hard").and_then(|v| v.as_bool()).unwrap_or(false);

                let forgotten = lm.forget(id, hard)?;
                Ok(json!({
                    "id": id,
                    "forgotten": forgotten,
                    "hard_delete": hard
                }))
            }

            "stats" => {
                let stats = lm.stats()?;
                Ok(json!({
                    "active_memories": stats.active_memories,
                    "expired_memories": stats.expired_memories,
                    "total_memories": stats.total_memories,
                    "total_vectors": stats.total_vectors,
                    "by_category": stats.by_category,
                    "db_path": lm.db_path().display().to_string()
                }))
            }

            other => anyhow::bail!("Unknown tool: {}", other),
        }
    }
}

fn parse_as_of_date(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
    let naive_dt = date.and_hms_opt(23, 59, 59).unwrap();
    Ok(Utc.from_utc_datetime(&naive_dt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_initialize() {
        let server = McpServer::new(false, None);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "initialize".to_string(),
            params: Some(json!({
                "protocolVersion": "2024-11-05"
            })),
        };

        let resp = server.handle_request(req).expect("Response expected");
        assert_eq!(resp.id, json!(1));
        let res = resp.result.unwrap();
        assert_eq!(res["protocolVersion"], "2024-11-05");
        assert_eq!(res["serverInfo"]["name"], "lightmem");
    }

    #[test]
    fn test_mcp_tools_list() {
        let server = McpServer::new(false, None);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(2)),
            method: "tools/list".to_string(),
            params: None,
        };

        let resp = server.handle_request(req).expect("Response expected");
        let res = resp.result.unwrap();
        let tools = res["tools"].as_array().unwrap();
        let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

        assert!(tool_names.contains(&"remember"));
        assert!(tool_names.contains(&"recall"));
        assert!(tool_names.contains(&"answer"));
        assert!(tool_names.contains(&"graph_related"));
        assert!(tool_names.contains(&"conflicts"));
        assert!(tool_names.contains(&"config"));
        assert!(tool_names.contains(&"forget"));
        assert!(tool_names.contains(&"stats"));
    }

    #[test]
    fn test_mcp_ping() {
        let server = McpServer::new(false, None);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!("ping-1")),
            method: "ping".to_string(),
            params: None,
        };

        let resp = server.handle_request(req).expect("Response expected");
        assert_eq!(resp.id, json!("ping-1"));
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_mcp_notification_no_response() {
        let server = McpServer::new(false, None);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "notifications/initialized".to_string(),
            params: None,
        };

        let resp = server.handle_request(req);
        assert!(resp.is_none());
    }

    #[test]
    fn test_mcp_unknown_method() {
        let server = McpServer::new(false, None);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(99)),
            method: "unknown/method".to_string(),
            params: None,
        };

        let resp = server.handle_request(req).expect("Response expected");
        assert!(resp.result.is_none());
        assert_eq!(resp.error.unwrap().code, -32601);
    }
}
