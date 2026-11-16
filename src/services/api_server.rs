use crate::controllers::LightMem;
use crate::models::{
    compute_sha256_hex, decode_base64, detect_mime_and_kind, parse_ttl_duration, AttachmentInput,
    AttachmentMetadata, MemoryStatus, MemoryType,
};
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentPayload {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
    pub confidence: Option<f32>,
    #[serde(default)]
    pub supersede: bool,
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
#[serde(deny_unknown_fields)]
pub struct ClearPayload {
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

fn url_decode(s: &str) -> Result<String> {
    let mut result = Vec::with_capacity(s.len());
    let mut bytes = s.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let h1 = bytes.next().context("Incomplete percent escape")?;
            let h2 = bytes.next().context("Incomplete percent escape")?;
            let hex = [h1, h2];
            result.push(
                u8::from_str_radix(std::str::from_utf8(&hex)?, 16)
                    .context("Invalid percent escape")?,
            );
        } else if b == b'+' {
            result.push(b' ');
        } else {
            result.push(b);
        }
    }
    String::from_utf8(result).context("Query parameter is not valid UTF-8")
}

fn parse_query_params(query: &str) -> Result<HashMap<String, String>> {
    let mut params = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        let key = url_decode(k)?;
        anyhow::ensure!(
            !params.contains_key(&key),
            "Duplicate query parameter: {}",
            key
        );
        params.insert(key, url_decode(v)?);
    }
    Ok(params)
}

fn query_value<T: std::str::FromStr>(
    query: &HashMap<String, String>,
    key: &str,
) -> Result<Option<T>> {
    query
        .get(key)
        .map(|value| {
            value
                .parse()
                .map_err(|_| anyhow::anyhow!("Invalid '{}' filter", key))
        })
        .transpose()
}

fn authorized(headers: &HashMap<String, String>, expected_key: Option<&str>) -> bool {
    let Some(expected) = expected_key else {
        return true;
    };
    headers
        .get("authorization")
        .map(|h| {
            let token = h.trim();
            let token = if token.to_ascii_lowercase().starts_with("bearer ") {
                token[7..].trim()
            } else {
                token
            };
            token == expected
        })
        .or_else(|| headers.get("x-api-key").map(|k| k.trim() == expected))
        .unwrap_or(false)
}

pub(crate) struct HttpRequest {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream, key: Option<&str>) -> Result<Option<HttpRequest>> {
    let mut reader = BufReader::new(stream);
    let mut req_line = String::new();
    if reader.by_ref().take(8193).read_line(&mut req_line)? == 0 {
        return Ok(None);
    }
    anyhow::ensure!(
        req_line.len() <= 8192 && req_line.ends_with('\n'),
        "Request line too large"
    );
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
    let query = query_str
        .map(parse_query_params)
        .transpose()?
        .unwrap_or_default();

    let mut headers = HashMap::new();
    let mut content_length: usize = 0;
    let mut header_bytes = 0usize;
    loop {
        let mut line = String::new();
        let n = reader.by_ref().take(8193).read_line(&mut line)?;
        header_bytes += n;
        anyhow::ensure!(
            line.len() <= 8192 && header_bytes <= 65536,
            "Request headers too large"
        );
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim().to_string();
            anyhow::ensure!(!headers.contains_key(&key), "Duplicate request header");
            anyhow::ensure!(
                key != "transfer-encoding",
                "Transfer-Encoding is not supported"
            );
            if key == "content-length" {
                content_length = val.parse().context("Invalid Content-Length")?;
            }
            headers.insert(key, val);
        }
    }

    if content_length > 64 * 1024 * 1024 {
        anyhow::bail!("Payload Too Large: content length exceeds 64MB");
    }

    // Authenticate before allocating or reading potentially large request bodies.
    let should_read = authorized(&headers, key);
    let mut body = vec![0u8; if should_read { content_length } else { 0 }];
    if !body.is_empty() {
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

pub(crate) fn send_response(
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
         X-Content-Type-Options: nosniff\r\n\
         Cache-Control: no-store\r\n\
         Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' blob: data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'\r\n\
         Referrer-Policy: no-referrer\r\n\
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

pub(crate) fn send_json<T: Serialize>(
    stream: TcpStream,
    status_code: u16,
    status_text: &str,
    data: &T,
) {
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

pub(crate) fn send_error(stream: TcpStream, status_code: u16, status_text: &str, message: &str) {
    let err = serde_json::json!({ "error": message });
    send_json(stream, status_code, status_text, &err);
}

fn handle_connection(
    mut stream: TcpStream,
    state: &Arc<super::workspace::WorkspaceState>,
    effective_api_key: Option<&str>,
) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(15)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(30)));

    let req = match read_request(&mut stream, effective_api_key) {
        Ok(Some(req)) => req,
        Ok(None) => return,
        Err(e) => {
            send_error(stream, 400, "Bad Request", &e.to_string());
            return;
        }
    };

    if let Some(origin) = req.headers.get("origin") {
        let expected = req
            .headers
            .get("host")
            .map(|host| format!("http://{}", host));
        if expected.as_deref() != Some(origin.as_str()) {
            send_error(
                stream,
                403,
                "Forbidden",
                "Cross-origin requests are not allowed",
            );
            return;
        }
    }
    if let Ok(address) = stream.local_addr() {
        if address.ip().is_loopback() {
            let allowed = [
                format!("127.0.0.1:{}", address.port()),
                format!("localhost:{}", address.port()),
                format!("[::1]:{}", address.port()),
            ];
            if req
                .headers
                .get("host")
                .is_some_and(|host| !allowed.contains(host))
            {
                send_error(stream, 403, "Forbidden", "Invalid Host for loopback server");
                return;
            }
        }
    }

    if req.method == "OPTIONS" {
        send_response(stream, 204, "No Content", "text/plain", b"");
        return;
    }

    // Health check endpoint is accessible without authentication
    let is_health = req.method == "GET" && (req.path == "/v1/health" || req.path == "/health");

    if req.method == "GET" {
        if let Some((bytes, content_type)) = super::workspace::asset(&req.path) {
            send_response(stream, 200, "OK", content_type, bytes);
            return;
        }
    }

    if !authorized(&req.headers, effective_api_key) && !is_health {
        send_error(
            stream,
            401,
            "Unauthorized",
            "Unauthorized: invalid or missing API key",
        );
        return;
    }

    let _mutation = if req.method != "GET" {
        match state.mutation.lock() {
            Ok(guard) => Some(guard),
            Err(_) => {
                send_error(
                    stream,
                    500,
                    "Internal Server Error",
                    "Workspace operation lock unavailable",
                );
                return;
            }
        }
    } else {
        None
    };
    if req.method != "GET" && state.busy.load(std::sync::atomic::Ordering::SeqCst) {
        send_error(
            stream,
            409,
            "Conflict",
            "A model or restore operation is running; retry when it finishes",
        );
        return;
    }
    let controller = match state.controller() {
        Ok(controller) => controller,
        Err(e) => {
            send_error(stream, 500, "Internal Server Error", &e.to_string());
            return;
        }
    };

    if let Some(expected) = req.headers.get("x-lightmem-vault") {
        if url_decode(expected).ok().as_deref()
            != Some(controller.db_path().to_string_lossy().as_ref())
        {
            send_error(stream, 409, "Conflict", "The selected vault changed in another tab. Refresh the workspace before continuing.");
            return;
        }
    }
    if req.path.starts_with("/v1/workspace/") {
        super::workspace::handle(stream, &req, state);
        return;
    }

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/v1/health") | ("GET", "/health") => {
            let res = serde_json::json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
            });
            send_json(stream, 200, "OK", &res);
        }

        ("GET", "/v1/stats") | ("GET", "/stats") => match controller.stats() {
            Ok(stats) => send_json(stream, 200, "OK", &stats),
            Err(e) => send_error(stream, 500, "Internal Server Error", &e.to_string()),
        },

        ("GET", "/v1/links") => {
            let result = req
                .query
                .get("id")
                .context("Missing memory id")
                .and_then(|id| controller.get(id)?.context("Memory not found"))
                .and_then(|m| controller.get_links(&m.id));
            match result {
                Ok(links) => send_json(stream, 200, "OK", &links),
                Err(e) => send_error(stream, 400, "Bad Request", &e.to_string()),
            }
        }

        ("GET", "/v1/attachment") => {
            let result = (|| -> Result<_> {
                let memory = req.query.get("memory").context("Missing memory id")?;
                let id = req.query.get("id").context("Missing attachment id")?;
                controller
                    .get_attachments_data(memory)?
                    .into_iter()
                    .find(|(m, _)| &m.id == id)
                    .context("Attachment not found")
            })();
            match result {
                Ok((_, data)) => {
                    send_response(stream, 200, "OK", "application/octet-stream", &data)
                }
                Err(e) => send_error(stream, 404, "Not Found", &e.to_string()),
            }
        }

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
                    let (inferred_mime, inferred_kind) = detect_mime_and_kind(&att.name, &bytes);
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

            let extract_entities = !payload.no_entities.unwrap_or(false);

            match controller.remember_with_attachment_inputs(
                payload.content.as_deref(),
                attachments,
                false,
                extract_entities,
                None,
                payload.category,
                payload.title,
                payload.tags.unwrap_or_default(),
                payload.confidence,
                duration,
                payload.supersede,
                payload.source,
            ) {
                Ok((memory, conflicts, warning)) => {
                    let mut value = serde_json::to_value(memory).unwrap_or_default();
                    value["conflicts"] = serde_json::json!(conflicts);
                    value["superseded"] = serde_json::json!(payload.supersede);
                    if let Some(warning) = warning {
                        value["warning"] = serde_json::json!(warning);
                        value["warnings"] = serde_json::json!([warning]);
                    }
                    send_json(stream, 200, "OK", &value);
                }
                Err(e) => send_error(stream, 400, "Bad Request", &e.to_string()),
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

            let req_limit = if payload.limit == 0 {
                10
            } else {
                payload.limit
            };
            if payload.precision || req_limit > 1000 {
                send_error(stream, 400, "Bad Request", "Recall returns ranked memories. Use /v1/answer for precision answers; limit must not exceed 1000.");
                return;
            }
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
            let result = (|| -> Result<_> {
                let limit = query_value::<usize>(&req.query, "limit")?.unwrap_or(20);
                let mut offset = query_value::<usize>(&req.query, "offset")?.unwrap_or(0);
                anyhow::ensure!(
                    (1..=1000).contains(&limit),
                    "Limit must be between 1 and 1000"
                );
                if let Some(page) = query_value::<usize>(&req.query, "page")? {
                    anyhow::ensure!(page > 0, "Page must be positive");
                    offset = (page - 1)
                        .checked_mul(limit)
                        .context("Pagination overflow")?;
                }
                anyhow::ensure!(offset <= i64::MAX as usize, "Pagination offset too large");
                let category =
                    query_value(&req.query, "category")?.or(query_value(&req.query, "type")?);
                let status = if req.query.get("status").is_some_and(|s| s == "all") {
                    None
                } else {
                    Some(query_value(&req.query, "status")?.unwrap_or(MemoryStatus::Active))
                };
                let date = query_value::<NaiveDate>(&req.query, "date")?;
                let as_of = query_value::<DateTime<Utc>>(&req.query, "as_of")?;
                anyhow::ensure!(
                    date.is_none() || as_of.is_none(),
                    "Choose date or as_of, not both"
                );
                controller.list_filtered(
                    category,
                    status,
                    as_of,
                    date,
                    req.query.get("media").map(String::as_str),
                    limit,
                    offset,
                )
            })();
            match result {
                Ok(page) => send_json(stream, 200, "OK", &page),
                Err(e) => send_error(stream, 400, "Bad Request", &e.to_string()),
            }
        }

        ("GET", "/v1/get") | ("GET", "/get") | ("GET", "/v1/inspect") | ("GET", "/inspect") => {
            let term = req
                .query
                .get("id")
                .or_else(|| req.query.get("term"))
                .map(String::as_str);
            let Some(term) = term else {
                send_error(
                    stream,
                    400,
                    "Bad Request",
                    "Missing 'id' or 'term' query parameter",
                );
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
                match serde_json::from_slice(&req.body) {
                    Ok(payload) => payload,
                    Err(_) => {
                        send_error(stream, 400, "Bad Request", "Invalid forget request");
                        return;
                    }
                }
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
                        let res =
                            serde_json::json!({ "success": ok, "deleted": if ok { 1 } else { 0 } });
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

            match controller.unlink(
                &payload.source,
                &payload.target,
                payload.relation.as_deref(),
            ) {
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
            if !req
                .body
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| *b == b'{')
            {
                send_error(
                    stream,
                    400,
                    "Bad Request",
                    "Clear requires an explicit JSON object",
                );
                return;
            }
            let payload: ClearPayload = match serde_json::from_slice(&req.body) {
                Ok(payload) => payload,
                Err(_) => {
                    send_error(stream, 400, "Bad Request", "Supply an explicit {\"soft\":true} to archive or {\"soft\":false} to permanently clear this vault");
                    return;
                }
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

#[derive(Debug, Clone)]
pub struct NetworkEndpoint {
    pub label: &'static str,
    pub ip: std::net::Ipv4Addr,
    pub if_name: String,
}

#[cfg(unix)]
pub fn discover_host_endpoints() -> Vec<NetworkEndpoint> {
    let mut endpoints = Vec::new();
    let mut seen_ips = std::collections::HashSet::new();

    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 || ifap.is_null() {
            return endpoints;
        }
        let mut curr = ifap;
        while !curr.is_null() {
            let ifa = *curr;
            if !ifa.ifa_addr.is_null() && (*ifa.ifa_addr).sa_family as i32 == libc::AF_INET {
                let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                let ip_u32 = u32::from_be(sin.sin_addr.s_addr);
                let ip = std::net::Ipv4Addr::from(ip_u32);
                let name = if !ifa.ifa_name.is_null() {
                    std::ffi::CStr::from_ptr(ifa.ifa_name)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    String::new()
                };

                let lower_name = name.to_lowercase();
                if !lower_name.starts_with("docker")
                    && !lower_name.starts_with("br-")
                    && !lower_name.starts_with("veth")
                    && seen_ips.insert(ip)
                {
                    let octets = ip.octets();
                    let is_tailscale = (octets[0] == 100 && (64..=127).contains(&octets[1]))
                        || lower_name.contains("tailscale");

                    let label = if ip.is_loopback() {
                        "Localhost"
                    } else if is_tailscale {
                        "Tailscale"
                    } else if ip.is_private() {
                        "Local LAN"
                    } else {
                        "Public / WAN"
                    };

                    endpoints.push(NetworkEndpoint {
                        label,
                        ip,
                        if_name: name,
                    });
                }
            }
            curr = ifa.ifa_next;
        }
        libc::freeifaddrs(ifap);
    }

    endpoints.sort_by_key(|e| match e.label {
        "Localhost" => 0,
        "Tailscale" => 1,
        "Local LAN" => 2,
        _ => 3,
    });

    endpoints
}

#[cfg(not(unix))]
pub fn discover_host_endpoints() -> Vec<NetworkEndpoint> {
    vec![NetworkEndpoint {
        label: "Localhost",
        ip: std::net::Ipv4Addr::new(127, 0, 0, 1),
        if_name: "lo".to_string(),
    }]
}

pub fn serve_api(controller: Arc<LightMem>, options: ServerOptions) -> Result<()> {
    let bind_addr = format!("{}:{}", options.host, options.port);
    let listener = TcpListener::bind(&bind_addr)
        .with_context(|| format!("Failed to bind LightMem backend server to {}", bind_addr))?;

    let effective_key = if options.no_auth {
        None
    } else if let Some(k) = options.api_key {
        let trimmed = k.trim().to_string();
        anyhow::ensure!(
            !trimmed.is_empty(),
            "API key cannot be blank. Omit it to generate one, or explicitly use --no-auth."
        );
        Some(trimmed)
    } else {
        Some(format!("lmem_{}", uuid::Uuid::new_v4().simple()))
    };

    println!();
    println!("  \x1b[1;38;2;220;38;38m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M\x1b[0m  \x1b[38;2;113;113;122mPersistent Backend Server\x1b[0m");
    println!("  \x1b[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    if options.host == "0.0.0.0" {
        println!(
            "  \x1b[1;38;2;16;185;129m▸ Listening:\x1b[0m         \x1b[38;2;96;165;250m0.0.0.0:{}\x1b[0m \x1b[38;2;113;113;122m(all network interfaces)\x1b[0m",
            options.port
        );
        println!("  \x1b[1;38;2;16;185;129m▸ Network Endpoints:\x1b[0m");
        let endpoints = discover_host_endpoints();
        for ep in &endpoints {
            println!(
                "    \x1b[38;2;113;113;122m•\x1b[0m \x1b[1m{:<12}\x1b[0m \x1b[38;2;96;165;250mhttp://{}:{}\x1b[0m \x1b[38;2;113;113;122m({})\x1b[0m",
                ep.label, ep.ip, options.port, ep.if_name
            );
        }
    } else {
        println!(
            "  \x1b[1;38;2;16;185;129m▸ Listening:\x1b[0m         \x1b[38;2;96;165;250mhttp://{}:{}\x1b[0m",
            options.host, options.port
        );
    }
    println!(
        "  \x1b[1;38;2;16;185;129m▸ Vault DB:\x1b[0m          \x1b[38;2;148;163;184m{}\x1b[0m",
        controller.db_path().display()
    );
    if let Some(ref key) = effective_key {
        println!(
            "  \x1b[1;38;2;16;185;129m▸ API Key:\x1b[0m           \x1b[1;38;2;234;179;8m{}\x1b[0m  \x1b[38;2;113;113;122m(Keep this secret!)\x1b[0m",
            key
        );
        println!("  \x1b[1;38;2;16;185;129m▸ Client Setup (Examples):\x1b[0m");
        if options.host == "0.0.0.0" {
            let endpoints = discover_host_endpoints();
            for ep in &endpoints {
                println!(
                    "    \x1b[38;2;113;113;122m•\x1b[0m {:<12} \x1b[38;2;248;250;252mlmem remote http://{}:{} --api-key \"{}\"\x1b[0m",
                    ep.label, ep.ip, options.port, key
                );
            }
        } else {
            println!(
                "    \x1b[38;2;113;113;122m•\x1b[0m {:<12} \x1b[38;2;248;250;252mlmem remote http://{}:{} --api-key \"{}\"\x1b[0m",
                "Remote", options.host, options.port, key
            );
        }
    } else {
        println!(
            "  \x1b[1;38;2;239;68;68m▸ Auth:\x1b[0m              \x1b[38;2;239;68;68mDisabled (--no-auth). Open to any network client.\x1b[0m"
        );
    }
    let probe_host = if options.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        &options.host
    };
    println!(
        "  \x1b[1;38;2;16;185;129m▸ Health Probe:\x1b[0m      \x1b[38;2;96;165;250mhttp://{}:{}/v1/health\x1b[0m",
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
    let state = Arc::new(super::workspace::WorkspaceState::new(controller));
    let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    struct RequestGuard(Arc<std::sync::atomic::AtomicUsize>);
    impl Drop for RequestGuard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if active
                    .try_update(
                        std::sync::atomic::Ordering::SeqCst,
                        std::sync::atomic::Ordering::SeqCst,
                        |n| (n < 32).then_some(n + 1),
                    )
                    .is_err()
                {
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(1)));
                    send_error(
                        stream,
                        503,
                        "Service Unavailable",
                        "Server is busy; retry shortly",
                    );
                    continue;
                }
                let guard = RequestGuard(Arc::clone(&active));
                let ctrl = Arc::clone(&state);
                let key = key_arc.clone();
                std::thread::spawn(move || {
                    let _guard = guard;
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
