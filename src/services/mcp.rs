use crate::controllers::LightMem;
use crate::models::{
    parse_ttl_duration, AttachmentInput, EntityFilter, LightMemConfig, ListSort, MemoryContext,
    MemoryScope, MemoryStatus, MemoryType,
};
use crate::services::{ExportFilter, HistoryService};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

fn deserialize_some_value<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Value::deserialize(deserializer)?))
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default, deserialize_with = "deserialize_some_value")]
    id: Option<Value>,
    #[serde(default)]
    method: Option<Value>,
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
    connection: Option<crate::models::ResolvedConnection>,
    config_override: Option<LightMemConfig>,
    controller: std::sync::Mutex<Option<Arc<LightMem>>>,
}

impl McpServer {
    pub fn new(global: bool, db_path: Option<PathBuf>) -> Self {
        Self {
            global,
            db_path,
            connection: None,
            config_override: None,
            controller: std::sync::Mutex::new(None),
        }
    }

    pub fn with_connection(
        mut self,
        connection: Option<crate::models::ResolvedConnection>,
    ) -> Self {
        self.connection = connection;
        self
    }

    #[cfg(test)]
    pub fn with_config(mut self, config: LightMemConfig) -> Self {
        self.config_override = Some(config);
        self
    }

    fn open_controller(&self) -> Result<LightMem> {
        if let Some(ref connection) = self.connection {
            return LightMem::open_resolved(connection);
        }
        if let Some(ref path) = self.db_path {
            let config = self
                .config_override
                .clone()
                .unwrap_or_else(|| LightMemConfig::try_load().unwrap_or_default());
            LightMem::open_at(path, config)
        } else {
            LightMem::open_default(self.global)
        }
    }

    fn get_controller(&self) -> Result<Arc<LightMem>> {
        let mut guard = self
            .controller
            .lock()
            .map_err(|_| anyhow::anyhow!("McpServer controller mutex poisoned"))?;
        if let Some(ref lm) = *guard {
            return Ok(Arc::clone(lm));
        }
        let lm = Arc::new(self.open_controller()?);
        *guard = Some(Arc::clone(&lm));
        Ok(lm)
    }

    fn invalidate_controller(&self) {
        if let Ok(mut guard) = self.controller.lock() {
            *guard = None;
        }
    }

    fn is_known_tool(&self, name: &str) -> bool {
        matches!(
            name,
            "remember"
                | "recall"
                | "answer"
                | "context"
                | "graph_related"
                | "conflicts"
                | "config"
                | "forget"
                | "clear"
                | "stats"
                | "list"
                | "inspect"
                | "update"
                | "link"
                | "unlink"
                | "links"
                | "graph"
                | "tags"
                | "entities"
                | "deduplicate"
                | "autolink"
                | "observe"
                | "restore"
                | "export"
                | "import"
                | "bundle_export"
                | "bundle_import"
                | "bundle_info"
                | "history"
        )
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
        let is_notification = req.id.is_none();

        // Validate jsonrpc version.
        if req.jsonrpc != "2.0" {
            if is_notification {
                return None;
            }
            return Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id: req.id.unwrap_or(Value::Null),
                result: None,
                error: Some(JsonRpcError {
                    code: -32600,
                    message: "Invalid Request: jsonrpc must be \"2.0\"".into(),
                    data: None,
                }),
            });
        }

        // Validate method: must be present and a string.
        let method = match req.method {
            Some(Value::String(ref m)) => m.clone(),
            Some(_) => {
                if is_notification {
                    return None;
                }
                return Some(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: req.id.unwrap_or(Value::Null),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32600,
                        message: "Invalid Request: method must be a string".into(),
                        data: None,
                    }),
                });
            }
            None => {
                if is_notification {
                    return None;
                }
                return Some(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: req.id.unwrap_or(Value::Null),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32600,
                        message: "Invalid Request: method missing".into(),
                        data: None,
                    }),
                });
            }
        };

        if is_notification {
            if method == "tools/call" {
                let params = req.params.unwrap_or(Value::Null);
                let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                let _ = self.call_tool(tool_name, arguments);
            }
            return None;
        }

        let id = req.id.unwrap();

        // Validate id: must be String, Number, or Null per JSON-RPC 2.0.
        if !matches!(&id, Value::String(_) | Value::Number(_) | Value::Null) {
            return Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32600,
                    message: "Invalid Request: id must be String, Number, or Null".into(),
                    data: None,
                }),
            });
        }

        match method.as_str() {
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

                if tool_name.is_empty() {
                    return Some(JsonRpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: "Invalid params: missing tool name".into(),
                            data: None,
                        }),
                    });
                }

                if !self.is_known_tool(tool_name) {
                    return Some(JsonRpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32602,
                            message: format!("Unknown tool: {}", tool_name),
                            data: None,
                        }),
                    });
                }

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
            json!({ "name": "context", "description": "Show the active user, agent, harness, memory scope, and database", "inputSchema": {"type": "object", "properties": {}} }),
            json!({
                "name": "remember",
                "description": "Store a memory, fact, preference, decision, instruction, or attached file/image into persistent SQLite memory",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": {
                            "type": "string",
                            "description": "Current conversation ID for source attribution; user, agent, and harness remain bound by the server"
                        },
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
                        "extract_entities": {
                            "type": "boolean",
                            "description": "Automatically extract key entities/slots via LLM (default: true)"
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
                        },
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Filter by one or more tags"
                        },
                        "media": {
                            "type": "string",
                            "description": "Filter by attachment media kind (image, audio, video, document, code, archive, binary, or any)"
                        },
                        "session": {
                            "type": "string",
                            "description": "Filter by conversation/session ID"
                        },
                        "entity": {
                            "type": "string",
                            "description": "Filter by extracted entity ('name' or 'name=val')"
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
                        "tags": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Filter candidate memories by tag"
                        },
                        "session": {
                            "type": "string",
                            "description": "Filter candidate memories by session ID"
                        },
                        "entity": {
                            "type": "string",
                            "description": "Filter candidate memories by entity ('name' or 'name=val')"
                        },
                        "as_of": {
                            "type": "string",
                            "description": "Point-in-time view (YYYY-MM-DD or RFC3339)"
                        },
                        "date": {
                            "type": "string",
                            "description": "Filter candidate memories by calendar day (YYYY-MM-DD)"
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
                            "description": "Override reranker mode ('onnx', 'openai:<model>', 'gemini:<model>', 'ollama:<model>', or 'top1')"
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
                        },
                        "reranker": {
                            "type": "string",
                            "description": "Override answer/merge model spec"
                        }
                    }
                }
            }),
            json!({
                "name": "config",
                "description": "Inspect or update LightMem runtime configuration (backend engine, ONNX embedding model, answer reranker, URLs, presets, and entity extraction)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "backend": {
                            "type": "string",
                            "enum": ["onnx", "ollama", "hash"],
                            "description": "Set embedding backend engine: 'onnx', 'ollama', or 'hash'"
                        },
                        "onnx_model": {
                            "type": "string",
                            "description": "ONNX embedding model ('bge-small', 'minilm', 'nomic', or directory path)"
                        },
                        "embedding_model": {
                            "type": "string",
                            "description": "Embedding model name"
                        },
                        "embedding_endpoint": {
                            "type": "string",
                            "description": "Dedicated embedding endpoint URL (or 'none' to clear)"
                        },
                        "reranker": {
                            "type": "string",
                            "description": "Answer model/engine ('minilm-squad2', 'tinyroberta-squad2', 'onnx', 'ollama:<model>', 'openai:<model>', 'top1', etc.)"
                        },
                        "url": {
                            "type": "string",
                            "description": "Generative model endpoint URL (e.g. http://localhost:11434 or https://api.openai.com)"
                        },
                        "preset": {
                            "type": "string",
                            "description": "Apply setup preset: local, ollama, openai, gemini, groq, openrouter, deepseek, mistral, custom, or demo"
                        },
                        "entity_extraction": {
                            "type": "string", "enum": ["off", "on"],
                            "description": "Toggle entity extraction ('on' or 'off'); off by default"
                        },
                        "remote_url": {
                            "type": "string",
                            "description": "Remote LightMem server URL (or 'none' to clear)"
                        },
                        "clear_remote": {
                            "type": "boolean",
                            "description": "Clear remote server URL and API key"
                        }
                    }
                }
            }),
            json!({
                "name": "list",
                "description": "List memories chronologically with pagination, category, tag, media, session, entity, date, sort, or status filters",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": { "type": "integer", "description": "Max results per page (default: 20)" },
                        "offset": { "type": "integer", "description": "Records to skip (default: 0)" },
                        "page": { "type": "integer", "description": "Page number (1-indexed; overrides offset)" },
                        "category": { "type": "string", "description": "Filter by category" },
                        "tag": { "type": "string", "description": "Filter by single tag" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Filter by multiple tags" },
                        "media": { "type": "string", "description": "Filter by attachment media kind" },
                        "session": { "type": "string", "description": "Filter by session ID" },
                        "entity": { "type": "string", "description": "Filter by entity ('name' or 'name=val')" },
                        "as_of": { "type": "string", "description": "Point-in-time view (YYYY-MM-DD or RFC3339)" },
                        "date": { "type": "string", "description": "Filter to calendar day (YYYY-MM-DD)" },
                        "status": { "type": "string", "enum": ["active", "expired", "all"], "description": "Filter by status (default: active)" },
                        "sort": { "type": "string", "enum": ["newest", "oldest", "updated", "confidence"], "description": "Sort order (default: newest)" }
                    }
                }
            }),
            json!({
                "name": "inspect",
                "description": "Deeply inspect one or more memory records by ID, prefix, or title, including attachments and knowledge graph links",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "term": { "type": "string", "description": "Memory ID, prefix, or title" },
                        "terms": { "type": "array", "items": { "type": "string" }, "description": "Multiple memory IDs, prefixes, or titles" }
                    }
                }
            }),
            json!({
                "name": "update",
                "description": "Update an existing memory's content, title, category, tags, TTL, provenance, confidence, or attachments in-place",
                "inputSchema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": { "type": "string", "description": "Memory ID, prefix, or title" },
                        "content": { "type": "string", "description": "New text content" },
                        "title": { "type": "string", "description": "New title" },
                        "category": { "type": "string", "description": "New category" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Replace tags" },
                        "add_tags": { "type": "array", "items": { "type": "string" }, "description": "Add tags incrementally" },
                        "remove_tags": { "type": "array", "items": { "type": "string" }, "description": "Remove specific tags" },
                        "confidence": { "type": "number", "description": "New confidence score (0.0 to 1.0)" },
                        "provenance": { "type": "string", "description": "Updated provenance / source" },
                        "ttl": { "type": "string", "description": "Set/update expiration TTL duration (e.g. '24h', '7d')" },
                        "clear_ttl": { "type": "boolean", "description": "Clear expiration TTL" },
                        "files": { "type": "array", "items": { "type": "string" }, "description": "Additional file paths to attach" },
                        "refresh_entities": { "type": "boolean", "description": "Re-extract structured entities via configured LLM" }
                    }
                }
            }),
            json!({
                "name": "link",
                "description": "Create a directional relationship link between two memories",
                "inputSchema": {
                    "type": "object",
                    "required": ["source", "target"],
                    "properties": {
                        "source": { "type": "string", "description": "Source memory ID, title, or prefix" },
                        "target": { "type": "string", "description": "Target memory ID, title, or prefix" },
                        "relation": { "type": "string", "description": "Relationship type (default: relates_to)" },
                        "weight": { "type": "number", "description": "Connection weight / strength (default: 1.0)" }
                    }
                }
            }),
            json!({
                "name": "unlink",
                "description": "Remove a relationship link between two memories",
                "inputSchema": {
                    "type": "object",
                    "required": ["source", "target"],
                    "properties": {
                        "source": { "type": "string", "description": "Source memory ID, title, or prefix" },
                        "target": { "type": "string", "description": "Target memory ID, title, or prefix" },
                        "relation": { "type": "string", "description": "Specific relation to remove (omitting removes all)" }
                    }
                }
            }),
            json!({
                "name": "links",
                "description": "List incoming and outgoing relationship links for a specific memory",
                "inputSchema": {
                    "type": "object",
                    "required": ["id"],
                    "properties": {
                        "id": { "type": "string", "description": "Memory ID, title, or prefix" }
                    }
                }
            }),
            json!({
                "name": "graph",
                "description": "Retrieve knowledge graph nodes and edges with optional focus memory and hop radius",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "focus": { "type": "string", "description": "Focus the graph on a specific memory neighborhood (ID or title)" },
                        "hops": { "type": "integer", "description": "Maximum hop radius when focused (default: 2)" }
                    }
                }
            }),
            json!({
                "name": "tags",
                "description": "List all distinct tags across active memories and their counts",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Optional substring filter on tag names" }
                    }
                }
            }),
            json!({
                "name": "entities",
                "description": "List extracted entities across active memories or retrieve memories matching an entity",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Optional entity slot name filter" },
                        "val": { "type": "string", "description": "Optional entity value filter (returns matching memories)" },
                        "memories": { "type": "boolean", "description": "Return matching memory records instead of entity summary counts" }
                    }
                }
            }),
            json!({
                "name": "deduplicate",
                "description": "Merge identical or near-duplicate active memories and rebuild merged index cards",
                "inputSchema": { "type": "object", "properties": {} }
            }),
            json!({
                "name": "autolink",
                "description": "Automatically discover and link related memories via mentions, shared tags, and vector similarity",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "min_similarity": { "type": "number", "description": "Minimum cosine similarity threshold (default: 0.75)" }
                    }
                }
            }),
            json!({
                "name": "observe",
                "description": "Extract atomic candidate memories from conversation or notes text, optionally saving them immediately",
                "inputSchema": {
                    "type": "object",
                    "required": ["text"],
                    "properties": {
                        "text": { "type": "string", "description": "Conversation transcript or notes text" },
                        "yes": { "type": "boolean", "description": "Immediately save extracted candidates to the vault (default: false)" },
                        "dry_run": { "type": "boolean", "description": "Extract only without saving" },
                        "category": { "type": "string", "description": "Optional category override" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Optional tags to attach" },
                        "confidence": { "type": "number", "description": "Minimum confidence threshold (default: 0.85)" },
                        "reranker": { "type": "string", "description": "Optional extraction model override" }
                    }
                }
            }),
            json!({
                "name": "restore",
                "description": "Restore an archived/expired memory by ID, prefix, or exact title",
                "inputSchema": {
                    "type": "object",
                    "required": ["term"],
                    "properties": {
                        "term": { "type": "string", "description": "Memory ID, prefix, or exact title" }
                    }
                }
            }),
            json!({
                "name": "forget",
                "description": "Forget / soft-retire or permanently delete one or more memories by ID/title (or all with all:true & yes:true)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Single memory ID, title, or prefix" },
                        "ids": { "type": "array", "items": { "type": "string" }, "description": "Multiple memory IDs/titles to forget" },
                        "all": { "type": "boolean", "description": "Forget all memories in vault (requires yes: true)" },
                        "yes": { "type": "boolean", "description": "Explicit confirmation when all: true" },
                        "hard": { "type": "boolean", "description": "Permanently delete instead of soft-retiring (default: false)" }
                    }
                }
            }),
            json!({
                "name": "clear",
                "description": "Soft-retire or permanently wipe all memories in the active vault (requires yes: true)",
                "inputSchema": {
                    "type": "object",
                    "required": ["yes"],
                    "properties": {
                        "yes": { "type": "boolean", "description": "Explicit confirmation required to clear vault" },
                        "soft": { "type": "boolean", "description": "Soft-retire instead of permanently deleting (default: false)" }
                    }
                }
            }),
            json!({
                "name": "export",
                "description": "Export vault memories (optionally filtered) to lossless JSON backup or Open Knowledge Format (OKF) Markdown",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "output": { "type": "string", "description": "Output file path" },
                        "format": { "type": "string", "enum": ["json", "okf"], "description": "Export format (default: json)" },
                        "category": { "type": "string", "description": "Filter exported memories by category" },
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Filter by tags" },
                        "status": { "type": "string", "enum": ["active", "expired", "all"], "description": "Filter by status" },
                        "session": { "type": "string", "description": "Filter by session ID" },
                        "entity": { "type": "string", "description": "Filter by entity ('name' or 'name=val')" }
                    }
                }
            }),
            json!({
                "name": "import",
                "description": "Import memories from JSON, JSONL, Markdown/OKF file or directory",
                "inputSchema": {
                    "type": "object",
                    "required": ["file"],
                    "properties": {
                        "file": { "type": "string", "description": "Path to JSON, JSONL, OKF/Markdown file, or directory" },
                        "replace": { "type": "boolean", "description": "Overwrite conflicting IDs (default: false)" }
                    }
                }
            }),
            json!({
                "name": "bundle_export",
                "description": "Export the entire database and media attachments into a compressed .zip transfer bundle",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "output": { "type": "string", "description": "Target .zip/.lmbundle path" }
                    }
                }
            }),
            json!({
                "name": "bundle_import",
                "description": "Import a .zip/.lmbundle transfer bundle",
                "inputSchema": {
                    "type": "object",
                    "required": ["file"],
                    "properties": {
                        "file": { "type": "string", "description": "Path to .zip/.lmbundle file" },
                        "replace": { "type": "boolean", "description": "Replace existing database (default: false)" },
                        "merge": { "type": "boolean", "description": "Merge bundle into active vault (default: false)" }
                    }
                }
            }),
            json!({
                "name": "bundle_info",
                "description": "Inspect a transfer bundle manifest without unpacking",
                "inputSchema": {
                    "type": "object",
                    "required": ["file"],
                    "properties": {
                        "file": { "type": "string", "description": "Path to .zip/.lmbundle file" }
                    }
                }
            }),
            json!({
                "name": "history",
                "description": "Inspect recent CLI command execution history",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Filter by command/subcommand text" },
                        "limit": { "type": "integer", "description": "Max entries to return (default: 20)" },
                        "status": { "type": "string", "enum": ["success", "error"], "description": "Filter by execution status" }
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
        // Handle "config" before attempting to open the controller so broken config can be repaired via MCP.
        if name == "config" {
            let mut cfg = LightMemConfig::try_load().unwrap_or_default();
            let previous_identity = cfg.embedding_identity().ok();
            let mut updated = false;

            if let Some(preset) = args.get("preset") {
                let p = preset.as_str().context("preset must be a string")?;
                cfg.apply_preset(p)?;
                updated = true;
            }
            if let Some(backend) = args.get("backend") {
                let b = backend.as_str().context("backend must be a string")?;
                cfg.backend = LightMemConfig::parse_embedding_backend(b)?;
                updated = true;
            }
            if let Some(onnx_m) = args.get("onnx_model") {
                let m = onnx_m.as_str().context("onnx_model must be a string")?;
                if cfg.backend == "onnx" && args.get("embedding_model").is_none() {
                    cfg.embedding_model = m.to_string();
                }
                cfg.onnx_model = Some(m.to_string());
                updated = true;
            }
            if let Some(emb_m) = args.get("embedding_model") {
                let m = emb_m.as_str().context("embedding_model must be a string")?;
                cfg.set_field("embedding_model", m)?;
                updated = true;
            }
            if let Some(emb_ep) = args.get("embedding_endpoint") {
                let u = emb_ep
                    .as_str()
                    .context("embedding_endpoint must be a string")?;
                cfg.set_field("embedding_endpoint", u)?;
                updated = true;
            }
            if let Some(emb_key) = args.get("embedding_api_key") {
                let k = emb_key
                    .as_str()
                    .context("embedding_api_key must be a string")?;
                cfg.set_field("embedding_api_key", k)?;
                updated = true;
            }
            if let Some(url) = args.get("url") {
                let u = url.as_str().context("url must be a string")?;
                cfg.ollama_url = LightMemConfig::normalize_ollama_url(u);
                updated = true;
            }
            if let Some(r) = args.get("reranker") {
                let rk = r.as_str().context("reranker must be a string")?;
                let (_, prev_default_url, _) = cfg.active_provider();
                cfg.reranker = LightMemConfig::parse_reranker_setting(rk)?;
                let explicit_u = args.get("url").and_then(|v| v.as_str());
                cfg.align_provider_endpoint(Some(prev_default_url), explicit_u);
                updated = true;
            }
            if let Some(key) = args.get("ollama_api_key") {
                let k = key.as_str().context("ollama_api_key must be a string")?;
                cfg.set_field("ollama_api_key", k)?;
                updated = true;
            }
            if let Some(mode) = args.get("entity_extraction") {
                let m = mode
                    .as_str()
                    .context("entity_extraction must be a string")?;
                cfg.entity_extraction = LightMemConfig::parse_entity_extraction(m)?;
                updated = true;
            }
            if let Some(remote) = args.get("remote_url") {
                let r = remote.as_str().context("remote_url must be a string")?;
                cfg.set_field("remote_url", r)?;
                updated = true;
            }
            if let Some(api_key) = args.get("api_key") {
                let k = api_key.as_str().context("api_key must be a string")?;
                cfg.set_field("api_key", k)?;
                updated = true;
            }
            if args
                .get("clear_remote")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                cfg.remote_url = None;
                cfg.api_key = None;
                updated = true;
            }

            let active_db = if let Ok(lm) = self.get_controller() {
                if updated {
                    let new_id = cfg.embedding_identity().ok();
                    if previous_identity != new_id && lm.stats()?.total_memories > 0 {
                        anyhow::bail!("Changing embeddings for a populated vault requires migration. Use lmem config --yes to preview and migrate.");
                    }
                }
                lm.db_path().display().to_string()
            } else {
                self.db_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "default".to_string())
            };

            if updated {
                cfg.save()?;
                self.invalidate_controller();
            }

            let mut obj = cfg.redacted_json()?;
            if let Some(map) = obj.as_object_mut() {
                map.insert("updated".to_string(), json!(updated));
                map.insert("active_db".to_string(), json!(active_db));
            }
            return Ok(obj);
        }

        let mut connection = self.connection.clone();
        let mut fallback_session: Option<String> = None;
        if name == "remember" {
            if let Some(session) = args.get("session_id") {
                let session = session.as_str().context("session_id must be a string")?;
                crate::models::identity::validate_id(session)?;
                if let Some(context) = connection.as_mut() {
                    context.context.session_id = Some(session.into());
                } else {
                    fallback_session = Some(session.to_string());
                }
            }
        }
        let lm = match (&connection, &fallback_session) {
            (Some(connection), _) => Arc::new(LightMem::open_resolved(connection)?),
            (None, Some(session)) => {
                let config = self
                    .config_override
                    .clone()
                    .unwrap_or_else(|| LightMemConfig::try_load().unwrap_or_default());
                let db_path = self
                    .db_path
                    .clone()
                    .unwrap_or_else(|| LightMemConfig::resolve_db_path(self.global));
                let ctx = MemoryContext {
                    connection_id: "default".to_string(),
                    user_id: "default".to_string(),
                    agent_id: "default".to_string(),
                    harness_id: "mcp".to_string(),
                    project_id: None,
                    scope: MemoryScope::User,
                    session_id: Some(session.clone()),
                };
                Arc::new(LightMem::open_at_with_context(&db_path, config, ctx)?)
            }
            (None, None) => self.get_controller()?,
        };

        match name {
            "context" => Ok(match &self.connection {
                Some(c) => serde_json::to_value(c)?,
                None => {
                    json!({"connection_id": null, "db_path": lm.db_path()})
                }
            }),
            "remember" => {
                let content = args.get("content").and_then(|v| v.as_str());
                let category = if let Some(v) = args.get("category") {
                    let s = v.as_str().context("category must be a string")?;
                    Some(
                        s.parse::<MemoryType>()
                            .map_err(|e| anyhow::anyhow!("Invalid category '{}': {}", s, e))?,
                    )
                } else {
                    None
                };
                let title = args
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let mut tags = match args.get("tags") {
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
                if let Some(ref session) = fallback_session {
                    let s_tag = format!("session:{}", session);
                    if !tags.contains(&s_tag) {
                        tags.push(s_tag);
                    }
                }
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
                let extract_entities = args
                    .get("extract_entities")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let ttl = if let Some(v) = args.get("ttl") {
                    let s = v.as_str().context("ttl must be a string")?;
                    Some(
                        parse_ttl_duration(s)
                            .map_err(|e| anyhow::anyhow!("Invalid TTL '{}': {}", s, e))?,
                    )
                } else {
                    None
                };
                let supersede = args
                    .get("supersede")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let provenance = args
                    .get("provenance")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let (record, conflicts, warning) = lm.remember_with_attachments_and_entities(
                    content,
                    &files,
                    extract,
                    extract_entities,
                    None,
                    category,
                    title,
                    tags,
                    confidence,
                    ttl,
                    supersede,
                    provenance,
                )?;

                let mut resp_map = serde_json::Map::new();
                resp_map.insert("status".to_string(), json!("stored"));
                resp_map.insert("memory".to_string(), json!(record));
                resp_map.insert("conflicts_detected".to_string(), json!(conflicts.len()));
                resp_map.insert(
                    "superseded".to_string(),
                    json!(supersede && !conflicts.is_empty()),
                );
                if let Some(w) = warning {
                    resp_map.insert("warning".to_string(), json!(w));
                }
                if let Some(ref session) = fallback_session {
                    resp_map.insert("session_id".to_string(), json!(session));
                }

                Ok(serde_json::Value::Object(resp_map))
            }

            "recall" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: query")?;
                let category = parse_optional_category(&args)?;
                let tags = parse_tag_list_arg(&args, "tags");
                let session = args.get("session").and_then(|v| v.as_str());
                let entity = parse_optional_entity(&args)?;
                let media = args.get("media").and_then(|v| v.as_str());
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
                let (as_of, on_date) = parse_optional_dates(&args)?;
                let fetch_limit = if media.is_some() {
                    limit.max(100)
                } else {
                    limit
                };

                let mut results = lm.recall_filtered(
                    query,
                    category,
                    as_of,
                    on_date,
                    &tags,
                    session,
                    entity.as_ref(),
                    fetch_limit,
                    min_sim,
                    multi_hop,
                )?;
                if let Some(m) = media {
                    let k = m.trim().to_lowercase();
                    results.retain(|r| {
                        !r.memory.attachments.is_empty()
                            && (k == "any"
                                || k == "all"
                                || r.memory.attachments.iter().any(|a| {
                                    a.media_kind.eq_ignore_ascii_case(&k)
                                        || a.mime_type.to_lowercase().contains(&k)
                                }))
                    });
                    results.truncate(limit);
                }

                let ranked = if precision && !results.is_empty() {
                    let ans = lm.answer_with_filters(
                        query,
                        category,
                        as_of,
                        on_date,
                        &tags,
                        session,
                        entity.as_ref(),
                        limit,
                        Some("onnx"),
                    )?;
                    let mut res = results;
                    if let Some(ref selected) = ans.selected_memory {
                        if let Some(idx) = res.iter().position(|r| r.memory.id == selected.id) {
                            let item = res.remove(idx);
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
                let category = parse_optional_category(&args)?;
                let tags = parse_tag_list_arg(&args, "tags");
                let session = args.get("session").and_then(|v| v.as_str());
                let entity = parse_optional_entity(&args)?;
                let (as_of, on_date) = parse_optional_dates(&args)?;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                let reranker_override = args.get("reranker").and_then(|v| v.as_str());
                let precision = args
                    .get("precision")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let effective_r = reranker_override.or(if precision { Some("onnx") } else { None });
                let res = lm.answer_with_filters(
                    question,
                    category,
                    as_of,
                    on_date,
                    &tags,
                    session,
                    entity.as_ref(),
                    limit,
                    effective_r,
                )?;

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
                let reranker = args.get("reranker").and_then(|v| v.as_str());

                let conflicts = lm.find_conflicts_with_options(resolve, min_sim, reranker)?;
                Ok(json!({
                    "threshold": min_sim,
                    "resolved": resolve,
                    "count": conflicts.len(),
                    "conflicts": conflicts
                }))
            }

            "forget" => {
                let hard = args.get("hard").and_then(|v| v.as_bool()).unwrap_or(false);
                let all = args.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
                let yes = args.get("yes").and_then(|v| v.as_bool()).unwrap_or(false);
                if all {
                    anyhow::ensure!(
                        yes,
                        "Forgetting all memories requires explicit confirmation: pass yes=true"
                    );
                    let removed = lm.clear_all(hard)?;
                    return Ok(json!({
                        "all": true,
                        "forgotten": true,
                        "count": removed,
                        "hard_delete": hard
                    }));
                }
                if let Some(ids_arr) = args.get("ids").and_then(|v| v.as_array()) {
                    let mut items = Vec::new();
                    for raw_id in ids_arr {
                        if let Some(id) = raw_id.as_str() {
                            let ok = lm.forget(id, hard)?;
                            items.push(json!({ "id": id, "forgotten": ok, "hard_delete": hard }));
                        }
                    }
                    return Ok(json!({ "results": items }));
                }
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: id (or ids / all+yes)")?;

                let forgotten = lm.forget(id, hard)?;
                Ok(json!({
                    "id": id,
                    "forgotten": forgotten,
                    "hard_delete": hard
                }))
            }

            "clear" => {
                let yes = args.get("yes").and_then(|v| v.as_bool()).unwrap_or(false);
                anyhow::ensure!(
                    yes,
                    "Clearing the vault requires explicit confirmation: pass yes=true"
                );
                let soft = args.get("soft").and_then(|v| v.as_bool()).unwrap_or(false);
                let removed = lm.clear_all(!soft)?;
                Ok(json!({
                    "cleared": removed,
                    "soft": soft
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

            "list" => {
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let page = args
                    .get("page")
                    .and_then(|v| v.as_u64())
                    .map(|p| p as usize);
                let category = parse_optional_category(&args)?;
                let mut tags = parse_tag_list_arg(&args, "tags");
                if tags.is_empty() {
                    if let Some(single) = args.get("tag").and_then(|v| v.as_str()) {
                        let t = single.trim();
                        if !t.is_empty() {
                            tags.push(t.to_string());
                        }
                    }
                }
                let media = args.get("media").and_then(|v| v.as_str());
                let session = args.get("session").and_then(|v| v.as_str());
                let entity = parse_optional_entity(&args)?;
                let (as_of, on_date) = parse_optional_dates(&args)?;
                let status = match args.get("status").and_then(|v| v.as_str()) {
                    Some("all") => None,
                    Some("expired") => Some(MemoryStatus::Expired),
                    Some("active") | None => Some(MemoryStatus::Active),
                    Some(other) => anyhow::bail!(
                        "Invalid status '{}'. Expected active, expired, or all",
                        other
                    ),
                };
                let sort = args
                    .get("sort")
                    .and_then(|v| v.as_str())
                    .map(|s| s.parse::<ListSort>().map_err(|e| anyhow::anyhow!(e)))
                    .transpose()?
                    .unwrap_or(ListSort::Newest);
                let effective_offset = if let Some(p) = page {
                    anyhow::ensure!(p > 0, "Page must be positive");
                    (p - 1) * limit
                } else {
                    offset
                };
                let paginated = lm.list_filtered_full(
                    category,
                    status,
                    as_of,
                    on_date,
                    media,
                    &tags,
                    session,
                    entity.as_ref(),
                    sort,
                    limit,
                    effective_offset,
                )?;
                Ok(serde_json::to_value(&paginated)?)
            }

            "inspect" => {
                if let Some(terms_arr) = args.get("terms").and_then(|v| v.as_array()) {
                    let mut items = Vec::new();
                    for raw_term in terms_arr {
                        if let Some(t) = raw_term.as_str() {
                            if let Some(mem) = lm.get(t)? {
                                let links = lm.get_links(&mem.id).unwrap_or_default();
                                items.push(json!({ "memory": mem, "links": links }));
                            }
                        }
                    }
                    return Ok(json!(items));
                }
                let term = args
                    .get("term")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: term (or terms)")?;
                let memory = lm
                    .get(term)?
                    .with_context(|| format!("Memory '{}' not found", term))?;
                let links = lm.get_links(&memory.id).unwrap_or_default();
                Ok(json!({
                    "memory": memory,
                    "links": links
                }))
            }

            "update" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: id")?;
                let content = args.get("content").and_then(|v| v.as_str());
                let title = args.get("title").and_then(|v| v.as_str());
                let category = parse_optional_category(&args)?;
                let tags = if args.get("tags").is_some() {
                    Some(parse_tag_list_arg(&args, "tags"))
                } else {
                    None
                };
                let add_tags = parse_tag_list_arg(&args, "add_tags");
                let remove_tags = parse_tag_list_arg(&args, "remove_tags");
                let confidence = args
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32);
                let provenance = args.get("provenance").and_then(|v| v.as_str());
                let clear_ttl = args
                    .get("clear_ttl")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let new_expired_at = if clear_ttl {
                    Some(None)
                } else if let Some(ttl_raw) = args.get("ttl").and_then(|v| v.as_str()) {
                    let dur = parse_ttl_duration(ttl_raw)
                        .map_err(|e| anyhow::anyhow!("Invalid TTL '{}': {}", ttl_raw, e))?;
                    Some(Some(Utc::now() + dur))
                } else {
                    None
                };
                let files: Vec<PathBuf> = match args.get("files") {
                    Some(Value::Array(arr)) => arr
                        .iter()
                        .filter_map(|v| v.as_str().map(PathBuf::from))
                        .collect(),
                    _ => Vec::new(),
                };
                let mut attachments = Vec::with_capacity(files.len());
                for fpath in &files {
                    attachments.push(AttachmentInput::from_path(fpath)?);
                }
                let refresh_entities = args
                    .get("refresh_entities")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let updated = lm.update_memory_full(
                    id,
                    content,
                    title,
                    category,
                    tags,
                    add_tags,
                    remove_tags,
                    confidence,
                    provenance,
                    new_expired_at,
                    attachments,
                    refresh_entities,
                )?;
                let links = lm.get_links(&updated.id).unwrap_or_default();
                Ok(json!({
                    "status": "updated",
                    "memory": updated,
                    "links": links
                }))
            }

            "link" => {
                let source = args
                    .get("source")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: source")?;
                let target = args
                    .get("target")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: target")?;
                let relation = args.get("relation").and_then(|v| v.as_str());
                let weight = args
                    .get("weight")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32);

                let (src_id, dst_id) = lm.link(source, target, relation, weight)?;
                Ok(json!({
                    "status": "linked",
                    "source": src_id,
                    "target": dst_id,
                    "relation": relation.unwrap_or("relates_to"),
                    "weight": weight.unwrap_or(1.0)
                }))
            }

            "unlink" => {
                let source = args
                    .get("source")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: source")?;
                let target = args
                    .get("target")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: target")?;
                let relation = args.get("relation").and_then(|v| v.as_str());

                let removed = lm.unlink(source, target, relation)?;
                Ok(json!({
                    "source": source,
                    "target": target,
                    "removed": removed
                }))
            }

            "links" => {
                let id = args
                    .get("id")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: id")?;
                let links = lm.get_links(id)?;
                Ok(serde_json::to_value(&links)?)
            }

            "graph" => {
                let focus = args.get("focus").and_then(|v| v.as_str());
                let hops = args
                    .get("hops")
                    .and_then(|v| v.as_u64())
                    .map(|h| h as usize);

                let snapshot = lm.graph(focus, hops)?;
                Ok(serde_json::to_value(&snapshot)?)
            }

            "tags" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .map(|q| q.trim().to_lowercase());
                let mut tags = lm.get_all_tags()?;
                if let Some(ref q) = query {
                    if !q.is_empty() {
                        tags.retain(|(t, _)| t.contains(q));
                    }
                }
                let list: Vec<Value> = tags
                    .into_iter()
                    .map(|(t, c)| json!({ "tag": t, "count": c }))
                    .collect();
                Ok(json!(list))
            }

            "entities" => {
                let name = args.get("name").and_then(|v| v.as_str());
                let val = args.get("val").and_then(|v| v.as_str());
                let want_memories = args
                    .get("memories")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if val.is_some() || want_memories {
                    let target_name =
                        name.context("Entity 'name' is required when filtering memories or val")?;
                    let records = lm.find_by_entity(target_name, val)?;
                    Ok(serde_json::to_value(&records)?)
                } else {
                    let entities = lm.list_entities(name)?;
                    Ok(serde_json::to_value(&entities)?)
                }
            }

            "deduplicate" => {
                let merged = lm.deduplicate()?;
                Ok(json!({ "merged": merged }))
            }

            "autolink" => {
                let min_sim = args
                    .get("min_similarity")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32);
                let new_links = lm.autolink(min_sim)?;
                let snap = lm.graph(None, None)?;
                Ok(json!({
                    "new_links": new_links,
                    "nodes": snap.nodes.len(),
                    "edges": snap.edges.len(),
                }))
            }

            "observe" => {
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: text")?;
                let yes = args.get("yes").and_then(|v| v.as_bool()).unwrap_or(false);
                let dry_run = args
                    .get("dry_run")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(!yes);
                let category = parse_optional_category(&args)?;
                let tags = parse_tag_list_arg(&args, "tags");
                let tags_opt = if tags.is_empty() {
                    None
                } else {
                    Some(tags.as_slice())
                };
                let confidence = args
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .map(|f| f as f32);
                let reranker = args.get("reranker").and_then(|v| v.as_str());

                let candidates =
                    lm.observe_and_extract(text, category, tags_opt, confidence, reranker)?;
                if dry_run || !yes {
                    Ok(json!({
                        "dry_run": true,
                        "candidates": candidates,
                        "saved": Vec::<Value>::new(),
                    }))
                } else {
                    let saved = lm.save_observed_candidates(&candidates)?;
                    Ok(json!({
                        "dry_run": false,
                        "candidates": candidates,
                        "saved": saved,
                    }))
                }
            }

            "restore" => {
                let term = args
                    .get("term")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: term")?;
                let restored = lm.restore_memory(term)?;
                let links = lm.get_links(&restored.id).unwrap_or_default();
                Ok(json!({
                    "status": "restored",
                    "memory": restored,
                    "links": links
                }))
            }

            "export" => {
                let fmt = args
                    .get("format")
                    .and_then(|v| v.as_str())
                    .unwrap_or("json")
                    .to_lowercase();
                let category = parse_optional_category(&args)?;
                let tags = parse_tag_list_arg(&args, "tags");
                let session = args
                    .get("session")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string);
                let entity = parse_optional_entity(&args)?;
                let status = match args.get("status").and_then(|v| v.as_str()) {
                    Some("all") | None => None,
                    Some("active") => Some(MemoryStatus::Active),
                    Some("expired") => Some(MemoryStatus::Expired),
                    Some(other) => anyhow::bail!("Invalid status '{}'", other),
                };
                let filter = ExportFilter {
                    category,
                    status,
                    tags,
                    session_id: session,
                    entity,
                };
                let output = args
                    .get("output")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from);
                let exported_path = match fmt.as_str() {
                    "okf" => lm.export_okf_filtered(output.as_deref(), &filter)?,
                    "json" => {
                        let target =
                            output.unwrap_or_else(|| PathBuf::from("lightmem-backup.json"));
                        lm.export_json_filtered(&target, &filter)?
                    }
                    other => {
                        anyhow::bail!("Invalid export format '{}'. Use 'json' or 'okf'", other)
                    }
                };
                Ok(json!({
                    "format": fmt,
                    "output_path": exported_path,
                }))
            }

            "import" => {
                let file = args
                    .get("file")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: file")?;
                let replace = args
                    .get("replace")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let count = lm.import_file_with_options(&PathBuf::from(file), false, replace)?;
                Ok(json!({
                    "imported": count,
                    "file": file,
                    "replace": replace,
                }))
            }

            "bundle_export" => {
                let output = args
                    .get("output")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from);
                let exported = lm.export_bundle(output.as_deref())?;
                let manifest = lm.inspect_bundle(&exported)?;
                Ok(json!({
                    "output_path": exported,
                    "manifest": manifest,
                }))
            }

            "bundle_import" => {
                let file = args
                    .get("file")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: file")?;
                let replace = args
                    .get("replace")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let merge = args.get("merge").and_then(|v| v.as_bool()).unwrap_or(false);
                anyhow::ensure!(!(replace && merge), "replace and merge cannot both be true");
                let res = lm.import_bundle(&PathBuf::from(file), replace, merge)?;
                Ok(serde_json::to_value(&res)?)
            }

            "bundle_info" => {
                let file = args
                    .get("file")
                    .and_then(|v| v.as_str())
                    .context("Missing required parameter: file")?;
                let manifest = lm.inspect_bundle(&PathBuf::from(file))?;
                Ok(serde_json::to_value(&manifest)?)
            }

            "history" => {
                let query = args.get("query").and_then(|v| v.as_str());
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                let status = args.get("status").and_then(|v| v.as_str());
                let (entries, total) = HistoryService::list(query, status, limit)?;
                Ok(json!({
                    "total": total,
                    "entries": entries,
                }))
            }

            other => anyhow::bail!("Unknown tool: {}", other),
        }
    }
}

fn parse_tag_list_arg(args: &Value, key: &str) -> Vec<String> {
    match args.get(key) {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .flat_map(|s| s.split(','))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Some(Value::String(s)) => s
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_optional_category(args: &Value) -> Result<Option<MemoryType>> {
    if let Some(v) = args.get("category") {
        let s = v.as_str().context("category must be a string")?;
        Ok(Some(s.parse::<MemoryType>().map_err(|e| {
            anyhow::anyhow!("Invalid category '{}': {}", s, e)
        })?))
    } else {
        Ok(None)
    }
}

fn parse_optional_entity(args: &Value) -> Result<Option<EntityFilter>> {
    if let Some(v) = args.get("entity") {
        let s = v.as_str().context("entity must be a string")?;
        Ok(Some(
            EntityFilter::parse(s).map_err(|e| anyhow::anyhow!("{}", e))?,
        ))
    } else {
        Ok(None)
    }
}

fn parse_optional_dates(args: &Value) -> Result<(Option<DateTime<Utc>>, Option<NaiveDate>)> {
    let as_of = if let Some(v) = args.get("as_of") {
        let s = v.as_str().context("as_of must be a string")?;
        Some(parse_as_of_date(s).with_context(|| format!("Invalid as_of timestamp '{}'", s))?)
    } else {
        None
    };
    let on_date = if let Some(v) = args.get("date") {
        let s = v.as_str().context("date must be a string")?;
        Some(
            NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                .with_context(|| format!("Invalid date '{}', expected YYYY-MM-DD", s))?,
        )
    } else {
        None
    };
    Ok((as_of, on_date))
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
            method: Some(json!("initialize")),
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
            method: Some(json!("tools/list")),
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
        assert!(tool_names.contains(&"list"));
        assert!(tool_names.contains(&"inspect"));
        assert!(tool_names.contains(&"update"));
        assert!(tool_names.contains(&"link"));
        assert!(tool_names.contains(&"unlink"));
        assert!(tool_names.contains(&"graph"));
        assert!(tool_names.contains(&"tags"));
        assert!(tool_names.contains(&"restore"));
    }

    #[test]
    fn test_mcp_ping() {
        let server = McpServer::new(false, None);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!("ping-1")),
            method: Some(json!("ping")),
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
            method: Some(json!("notifications/initialized")),
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
            method: Some(json!("unknown/method")),
            params: None,
        };

        let resp = server.handle_request(req).expect("Response expected");
        assert!(resp.result.is_none());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn test_mcp_explicit_null_id_request() {
        let server = McpServer::new(false, None);
        let raw_json = r#"{"jsonrpc": "2.0", "id": null, "method": "ping"}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw_json).expect("Must parse");
        assert_eq!(req.id, Some(Value::Null));

        let resp = server
            .handle_request(req)
            .expect("Response expected for null id");
        assert_eq!(resp.id, Value::Null);
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_mcp_omitted_id_notification_deserialization() {
        let raw_json = r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#;
        let req: JsonRpcRequest = serde_json::from_str(raw_json).expect("Must parse");
        assert_eq!(req.id, None);

        let server = McpServer::new(false, None);
        let resp = server.handle_request(req);
        assert!(resp.is_none());
    }

    #[test]
    fn test_mcp_remember_session_without_connection() {
        let temp_db = tempfile::NamedTempFile::new().unwrap();
        let cfg = LightMemConfig {
            backend: "hash".to_string(),
            reranker: "top1".to_string(),
            entity_extraction: "off".to_string(),
            ..LightMemConfig::default()
        };
        let server = McpServer::new(false, Some(temp_db.path().to_path_buf())).with_config(cfg);
        let res = server
            .call_tool(
                "remember",
                json!({
                    "content": "Fact: LightMem gracefully records session",
                    "session_id": "session-xyz"
                }),
            )
            .expect("Must not crash when connection is None");

        assert_eq!(res["status"], "stored");
        assert_eq!(res["session_id"], "session-xyz");
        let tags = res["memory"]["tags"].as_array().unwrap();
        assert!(tags.iter().any(|t| t == "session:session-xyz"));
    }

    #[test]
    fn test_mcp_unknown_tool_returns_invalid_params() {
        let server = McpServer::new(false, None);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(10)),
            method: Some(json!("tools/call")),
            params: Some(json!({
                "name": "non_existent_tool",
                "arguments": {}
            })),
        };
        let resp = server.handle_request(req).expect("Response expected");
        assert!(resp.result.is_none());
        let err = resp.error.expect("Error expected");
        assert_eq!(err.code, -32602);
        assert!(err.message.contains("Unknown tool"));
    }

    #[test]
    fn test_mcp_notification_executes_tool_without_response() {
        let temp_db = tempfile::NamedTempFile::new().unwrap();
        let cfg = LightMemConfig {
            backend: "hash".to_string(),
            reranker: "top1".to_string(),
            entity_extraction: "off".to_string(),
            ..LightMemConfig::default()
        };
        let server = McpServer::new(false, Some(temp_db.path().to_path_buf())).with_config(cfg);
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some(json!("tools/call")),
            params: Some(json!({
                "name": "remember",
                "arguments": { "content": "Notification content" }
            })),
        };
        let resp = server.handle_request(req);
        assert!(resp.is_none());
    }

    #[test]
    fn test_mcp_invalid_category_returns_error() {
        let temp_db = tempfile::NamedTempFile::new().unwrap();
        let cfg = LightMemConfig {
            backend: "hash".to_string(),
            reranker: "top1".to_string(),
            entity_extraction: "off".to_string(),
            ..LightMemConfig::default()
        };
        let server = McpServer::new(false, Some(temp_db.path().to_path_buf())).with_config(cfg);
        let err = server
            .call_tool(
                "remember",
                json!({
                    "content": "Test content",
                    "category": "not_a_real_category"
                }),
            )
            .unwrap_err();
        assert!(err.to_string().contains("Invalid category"));
    }
}
