use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use crate::controllers::LightMem;
use crate::models::{
    compute_sha256_hex, decode_base64, detect_mime_and_kind, parse_ttl_duration,
    AttachmentInput, AttachmentMetadata, MemoryStatus, MemoryType,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentPayload {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RememberRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<MemoryType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_entities: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<AttachmentPayload>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallRequest {
    #[serde(default)]
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<MemoryType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<NaiveDate>,
    #[serde(default = "default_recall_limit")]
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_similarity: Option<f32>,
    #[serde(default)]
    pub precision: bool,
    #[serde(default)]
    pub multi_hop: bool,
}

impl Default for RecallRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            category: None,
            media: None,
            as_of: None,
            date: None,
            limit: default_recall_limit(),
            min_similarity: None,
            precision: false,
            multi_hop: false,
        }
    }
}

fn default_recall_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerRequest {
    pub question: String,
    #[serde(default)]
    pub precision: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reranker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<MemoryType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<NaiveDate>,
    #[serde(default = "default_recall_limit")]
    pub limit: usize,
}

impl Default for AnswerRequest {
    fn default() -> Self {
        Self {
            question: String::new(),
            precision: false,
            reranker: None,
            category: None,
            as_of: None,
            date: None,
            limit: default_recall_limit(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ForgetPayload {
    pub id: Option<String>,
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub hard: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkRequest {
    pub source: String,
    pub target: String,
    #[serde(default = "default_relation")]
    pub relation: String,
    #[serde(default = "default_weight")]
    pub weight: f32,
}

fn default_relation() -> String {
    "relates_to".to_string()
}

fn default_weight() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlinkRequest {
    pub source: String,
    pub target: String,
    pub relation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClearPayload {
    #[serde(default)]
    pub soft: bool,
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub no_auth: bool,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            api_key: None,
            no_auth: false,
        }
    }
}

fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let h1 = bytes.next();
            let h2 = bytes.next();
            if let (Some(h1), Some(h2)) = (h1, h2) {
                if let Ok(hex_str) = std::str::from_utf8(&[h1, h2]) {
                    if let Ok(decoded_byte) = u8::from_str_radix(hex_str, 16) {
                        result.push(decoded_byte as char);
                        continue;
                    }
                }
                result.push('%');
                result.push(h1 as char);
                result.push(h2 as char);
            } else {
                result.push('%');
            }
        } else if b == b'+' {
            result.push(' ');
        } else {
            result.push(b as char);
        }
    }
    result
}

fn parse_query_params(query: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        params.insert(url_decode(k), url_decode(v));
    }
    params
}

struct HttpRequest {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> Result<Option<HttpRequest>> {
    let mut reader = BufReader::new(stream);
    let mut req_line = String::new();
    if reader.read_line(&mut req_line)? == 0 {
        return Ok(None);
    }
    let req_line = req_line.trim_end();
    if req_line.is_empty() {
        return Ok(None);
    }
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let full_path = parts.next().unwrap_or("/").to_string();

    let (path_str, query_str) = match full_path.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (full_path.as_str(), None),
    };
    let path = path_str.to_string();
    let query = query_str.map(parse_query_params).unwrap_or_default();

    let mut headers = HashMap::new();
    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim().to_string();
            if key == "content-length" {
                content_length = val.parse().unwrap_or(0);
            }
            headers.insert(key, val);
        }
    }

    if content_length > 64 * 1024 * 1024 {
        anyhow::bail!("Payload Too Large: content length exceeds 64MB");
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok(Some(HttpRequest {
        method,
        path,
        query,
        headers,
        body,
    }))
}

fn send_response(
    mut stream: TcpStream,
    status_code: u16,
    status_text: &str,
    content_type: &str,
    body: &[u8],
) {
    let header = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\
         Access-Control-Allow-Headers: Authorization, Content-Type, X-API-Key\r\n\
         Connection: close\r\n\r\n",
        status_code,
        status_text,
        content_type,
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn send_json<T: Serialize>(stream: TcpStream, status_code: u16, status_text: &str, data: &T) {
    match serde_json::to_vec(data) {
        Ok(bytes) => send_response(
            stream,
            status_code,
            status_text,
            "application/json; charset=utf-8",
            &bytes,
        ),
        Err(e) => {
            let err = serde_json::json!({ "error": format!("Serialization error: {}", e) });
            let bytes = serde_json::to_vec(&err).unwrap_or_default();
            send_response(
                stream,
                500,
                "Internal Server Error",
                "application/json; charset=utf-8",
                &bytes,
            );
        }
    }
}

fn send_error(stream: TcpStream, status_code: u16, status_text: &str, message: &str) {
    let err = serde_json::json!({ "error": message });
    send_json(stream, status_code, status_text, &err);
}

fn handle_connection(
    mut stream: TcpStream,
    controller: &Arc<LightMem>,
    effective_api_key: Option<&str>,
) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(15)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(30)));

    let req = match read_request(&mut stream) {
        Ok(Some(req)) => req,
        Ok(None) => return,
        Err(e) => {
            send_error(stream, 400, "Bad Request", &e.to_string());
            return;
        }
    };

    if req.method == "OPTIONS" {
        send_response(stream, 204, "No Content", "text/plain", b"");
        return;
    }

    // Health check endpoint is accessible without authentication
    let is_health = req.method == "GET" && (req.path == "/v1/health" || req.path == "/health");

    if let Some(expected_key) = effective_api_key {
        let auth_header = req.headers.get("authorization").map(String::as_str);
        let x_api_key = req.headers.get("x-api-key").map(String::as_str);

        let is_authorized = if let Some(h) = auth_header {
            let token = h.trim();
            let token = if token.to_lowercase().starts_with("bearer ") {
                token[7..].trim()
            } else {
                token
            };
            token == expected_key
        } else if let Some(k) = x_api_key {
            k.trim() == expected_key
        } else {
            false
        };

        if !is_authorized && !is_health {
            send_error(
                stream,
                401,
                "Unauthorized",
                "Unauthorized: invalid or missing API key",
            );
            return;
        }
    }

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/v1/health") | ("GET", "/health") => {
            let res = serde_json::json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
                "vault": controller.db_path().display().to_string(),
            });
            send_json(stream, 200, "OK", &res);
        }

        ("GET", "/v1/stats") | ("GET", "/stats") => match controller.stats() {
            Ok(stats) => send_json(stream, 200, "OK", &stats),
            Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
        },

        ("POST", "/v1/remember") | ("POST", "/remember") => {
            let payload: RememberRequest = match serde_json::from_slice(&req.body) {
                Ok(p) => p,
                Err(e) => {
                    send_error(stream, 400, "Bad Request", &format!("Invalid JSON: {}", e));
                    return;
                }
            };

            let mut attachments = Vec::new();
            if let Some(atts) = payload.attachments {
                for att in atts {
                    let bytes = match decode_base64(&att.data_base64) {
                        Ok(b) => b,
                        Err(e) => {
                            send_error(
                                stream,
                                400,
                                "Bad Request",
                                &format!("Invalid base64 attachment: {}", e),
                            );
                            return;
                        }
                    };
                    let (inferred_mime, inferred_kind) =
                        detect_mime_and_kind(&att.name, &bytes);
                    let mime_type = att.mime.unwrap_or(inferred_mime);
                    let sha256 = compute_sha256_hex(&bytes);
                    let meta = AttachmentMetadata {
                        id: uuid::Uuid::new_v4().to_string(),
                        memory_id: String::new(),
                        filename: att.name,
                        mime_type,
                        media_kind: inferred_kind,
                        size_bytes: bytes.len() as u64,
                        sha256,
                        dimensions: None,
                        duration_secs: None,
                        created_at: Utc::now(),
                        data_base64: None,
                    };
                    attachments.push(AttachmentInput {
                        metadata: meta,
                        data: bytes,
                    });
                }
            }

            let duration = match payload.ttl.as_deref().map(parse_ttl_duration).transpose() {
                Ok(d) => d,
                Err(e) => {
                    send_error(stream, 400, "Bad Request", &format!("Invalid TTL: {}", e));
                    return;
                }
            };

            let extract_entities = !payload.no_entities.unwrap_or(true);

            match controller.remember_with_attachment_inputs(
                payload.content.as_deref(),
                attachments,
                false,
                extract_entities,
                None,
                payload.category,
                payload.title,
                payload.tags.unwrap_or_default(),
                None,
                duration,
                false,
                payload.source,
            ) {
                Ok((memory, _, _)) => send_json(stream, 200, "OK", &memory),
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("POST", "/v1/recall") | ("POST", "/recall") => {
            let payload: RecallRequest = match serde_json::from_slice(&req.body) {
                Ok(p) => p,
                Err(e) => {
                    send_error(stream, 400, "Bad Request", &format!("Invalid JSON: {}", e));
                    return;
                }
            };

            let req_limit = if payload.limit == 0 { 10 } else { payload.limit };
            let fetch_limit = if payload.media.is_some() {
                req_limit.max(100)
            } else {
                req_limit
            };

            match controller.recall_expanded(
                &payload.query,
                payload.category,
                payload.as_of,
                payload.date,
                fetch_limit,
                payload.min_similarity,
                payload.multi_hop,
            ) {
                Ok(mut results) => {
                    if let Some(ref media) = payload.media {
                        let k = media.trim().to_lowercase();
                        results.retain(|r| {
                            !r.memory.attachments.is_empty()
                                && (k == "any"
                                    || k == "all"
                                    || r.memory.attachments.iter().any(|a| {
                                        a.media_kind.eq_ignore_ascii_case(&k)
                                            || a.mime_type.eq_ignore_ascii_case(&k)
                                    }))
                        });
                    }
                    results.truncate(req_limit);
                    send_json(stream, 200, "OK", &results);
                }
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("POST", "/v1/answer") | ("POST", "/answer") => {
            let payload: AnswerRequest = match serde_json::from_slice(&req.body) {
                Ok(p) => p,
                Err(e) => {
                    send_error(stream, 400, "Bad Request", &format!("Invalid JSON: {}", e));
                    return;
                }
            };

            let reranker = if payload.precision {
                payload.reranker.as_deref().or(Some("onnx"))
            } else {
                payload.reranker.as_deref()
            };

            match controller.answer_with_options(
                &payload.question,
                payload.category,
                payload.as_of,
                payload.date,
                payload.limit,
                reranker,
            ) {
                Ok(result) => send_json(stream, 200, "OK", &result),
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("GET", "/v1/list") | ("GET", "/list") => {
            let limit = req
                .query
                .get("limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(20);
            let offset = req
                .query
                .get("offset")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let page: Option<usize> = req.query.get("page").and_then(|v| v.parse().ok());
            let offset = page.map(|p| (p.saturating_sub(1)) * limit).unwrap_or(offset);

            let category = req
                .query
                .get("category")
                .or_else(|| req.query.get("type"))
                .and_then(|v| v.parse().ok());
            let status = req
                .query
                .get("status")
                .and_then(|v| v.parse().ok())
                .or(Some(MemoryStatus::Active));
            let date = req
                .query
                .get("date")
                .and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok());
            let as_of = req
                .query
                .get("as_of")
                .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
                .map(|dt| dt.with_timezone(&Utc));

            match controller.list_paginated_with_date(category, status, as_of, date, limit, offset)
            {
                Ok(paginated) => send_json(stream, 200, "OK", &paginated),
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("GET", "/v1/get")
        | ("GET", "/get")
        | ("GET", "/v1/inspect")
        | ("GET", "/inspect") => {
            let term = req
                .query
                .get("id")
                .or_else(|| req.query.get("term"))
                .map(String::as_str);
            let Some(term) = term else {
                send_error(stream, 400, "Bad Request", "Missing 'id' or 'term' query parameter");
                return;
            };

            match controller.get(term) {
                Ok(Some(mem)) => send_json(stream, 200, "OK", &mem),
                Ok(None) => send_error(stream, 404, "Not Found", "Memory record not found"),
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("DELETE", "/v1/forget")
        | ("POST", "/v1/forget")
        | ("DELETE", "/forget")
        | ("POST", "/forget") => {
            let payload: ForgetPayload = if !req.body.is_empty() {
                serde_json::from_slice(&req.body).unwrap_or_default()
            } else {
                ForgetPayload {
                    id: req.query.get("id").cloned(),
                    all: req
                        .query
                        .get("all")
                        .map(|v| v == "true" || v == "1")
                        .unwrap_or(false),
                    hard: req
                        .query
                        .get("hard")
                        .map(|v| v == "true" || v == "1")
                        .unwrap_or(false),
                }
            };

            if payload.all {
                match controller.clear_all(payload.hard) {
                    Ok(count) => {
                        let res = serde_json::json!({ "success": true, "deleted": count });
                        send_json(stream, 200, "OK", &res);
                    }
                    Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
                }
            } else if let Some(ref id) = payload.id {
                match controller.forget(id, payload.hard) {
                    Ok(ok) => {
                        let res = serde_json::json!({ "success": ok, "deleted": if ok { 1 } else { 0 } });
                        send_json(stream, 200, "OK", &res);
                    }
                    Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
                }
            } else {
                send_error(
                    stream,
                    400,
                    "Bad Request",
                    "Missing 'id' or 'all' in forget request",
                );
            }
        }

        ("POST", "/v1/link") | ("POST", "/link") => {
            let payload: LinkRequest = match serde_json::from_slice(&req.body) {
                Ok(p) => p,
                Err(e) => {
                    send_error(stream, 400, "Bad Request", &format!("Invalid JSON: {}", e));
                    return;
                }
            };

            match controller.link(
                &payload.source,
                &payload.target,
                Some(&payload.relation),
                Some(payload.weight),
            ) {
                Ok((src_id, dst_id)) => {
                    let res = serde_json::json!({
                        "source_id": src_id,
                        "target_id": dst_id,
                        "relation": payload.relation,
                        "weight": payload.weight,
                    });
                    send_json(stream, 200, "OK", &res);
                }
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("POST", "/v1/unlink") | ("POST", "/unlink") => {
            let payload: UnlinkRequest = match serde_json::from_slice(&req.body) {
                Ok(p) => p,
                Err(e) => {
                    send_error(stream, 400, "Bad Request", &format!("Invalid JSON: {}", e));
                    return;
                }
            };

            match controller.unlink(&payload.source, &payload.target, payload.relation.as_deref()) {
                Ok(unlinked) => {
                    let res = serde_json::json!({ "unlinked": unlinked });
                    send_json(stream, 200, "OK", &res);
                }
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("GET", "/v1/related") | ("GET", "/related") => {
            let id = req.query.get("id").map(String::as_str);
            let hops = req
                .query
                .get("hops")
                .and_then(|v| v.parse().ok())
                .unwrap_or(2);
            let Some(id) = id else {
                send_error(stream, 400, "Bad Request", "Missing 'id' query parameter");
                return;
            };

            match controller.related(id, hops) {
                Ok(related) => send_json(stream, 200, "OK", &related),
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("GET", "/v1/graph") | ("GET", "/graph") => {
            let focus = req.query.get("focus").map(String::as_str);
            let hops = req.query.get("hops").and_then(|v| v.parse().ok());

            match controller.graph(focus, hops) {
                Ok(graph) => send_json(stream, 200, "OK", &graph),
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        ("POST", "/v1/clear") | ("POST", "/clear") => {
            let payload: ClearPayload = if !req.body.is_empty() {
                serde_json::from_slice(&req.body).unwrap_or_default()
            } else {
                ClearPayload::default()
            };

            match controller.clear_all(!payload.soft) {
                Ok(cleared) => {
                    let res = serde_json::json!({ "cleared": cleared });
                    send_json(stream, 200, "OK", &res);
                }
                Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
            }
        }

        _ => {
            send_error(
                stream,
                404,
                "Not Found",
                &format!("Endpoint not found: {} {}", req.method, req.path),
            );
        }
    }
}

pub fn serve_api(controller: Arc<LightMem>, options: ServerOptions) -> Result<()> {
    let bind_addr = format!("{}:{}", options.host, options.port);
    let listener = TcpListener::bind(&bind_addr)
        .with_context(|| format!("Failed to bind LightMem backend server to {}", bind_addr))?;

    let effective_key = if options.no_auth {
        None
    } else if let Some(k) = options.api_key {
        let trimmed = k.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    } else {
        Some(format!("lmem_{}", uuid::Uuid::new_v4().simple()))
    };

    println!();
    println!("  \x1b[1;38;2;220;38;38m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M\x1b[0m  \x1b[38;2;113;113;122mPersistent Backend Server\x1b[0m");
    println!("  \x1b[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    println!(
        "  \x1b[1;38;2;16;185;129m▸ Listening:\x1b[0m     \x1b[38;2;96;165;250mhttp://{}:{}\x1b[0m",
        options.host, options.port
    );
    println!(
        "  \x1b[1;38;2;16;185;129m▸ Vault DB:\x1b[0m      \x1b[38;2;148;163;184m{}\x1b[0m",
        controller.db_path().display()
    );
    if let Some(ref key) = effective_key {
        println!(
            "  \x1b[1;38;2;16;185;129m▸ API Key:\x1b[0m       \x1b[1;38;2;234;179;8m{}\x1b[0m  \x1b[38;2;113;113;122m(Keep this secret!)\x1b[0m",
            key
        );
        let client_host = if options.host == "0.0.0.0" {
            "127.0.0.1"
        } else {
            &options.host
        };
        println!(
            "  \x1b[1;38;2;16;185;129m▸ Client Setup:\x1b[0m  \x1b[38;2;248;250;252mlmem config --remote http://{}:{} --api-key \"{}\"\x1b[0m",
            client_host, options.port, key
        );
    } else {
        println!(
            "  \x1b[1;38;2;239;68;68m▸ Auth:\x1b[0m          \x1b[38;2;239;68;68mDisabled (--no-auth). Open to any network client.\x1b[0m"
        );
    }
    let probe_host = if options.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &options.host
    };
    println!(
        "  \x1b[1;38;2;16;185;129m▸ Health Probe:\x1b[0m  \x1b[38;2;96;165;250mhttp://{}:{}/v1/health\x1b[0m",
        probe_host, options.port
    );
    println!("  \x1b[38;2;148;163;184mServing across devices. Press Ctrl+C to stop.\x1b[0m\n");

    serve_loop(listener, controller, effective_key)
}

pub fn serve_loop(
    listener: TcpListener,
    controller: Arc<LightMem>,
    effective_key: Option<String>,
) -> Result<()> {
    let key_arc = effective_key.map(Arc::new);
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let ctrl = Arc::clone(&controller);
                let key = key_arc.clone();
                std::thread::spawn(move || {
                    handle_connection(stream, &ctrl, key.as_deref().map(|s| s.as_str()));
                });
            }
            Err(e) => {
                eprintln!("Warning: failed to accept TCP connection: {}", e);
            }
        }
    }
    Ok(())
}
