use crate::models::{MemoryRecord, ScoredMemory};
use anyhow::Result;
use libc::{c_char, c_int, dlopen, dlsym, RTLD_LAZY};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerResult {
    pub answer: String,
    pub selected_memory: Option<MemoryRecord>,
    pub confidence: f32,
    pub reranker_used: String,
}

pub trait Reranker: Send + Sync {
    fn name(&self) -> &str;
    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult>;
}

/// Mode 1: Fast / Instant Top-1 Reranker (Default)
/// Returns Rank-1 candidate with 0ms latency and 0MB extra RAM
pub struct Top1Reranker;

impl Reranker for Top1Reranker {
    fn name(&self) -> &str {
        "top1"
    }

    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        if candidates.is_empty() {
            return Ok(AnswerResult {
                answer: "No relevant memories found to answer this question.".to_string(),
                selected_memory: None,
                confidence: 0.0,
                reranker_used: "top1".to_string(),
            });
        }

        let Some(best) = select_candidate(question, &candidates[..1]) else {
            return Ok(no_evidence("top1"));
        };
        Ok(AnswerResult {
            answer: best.memory.content.clone(),
            selected_memory: Some(best.memory.clone()),
            confidence: best.memory.confidence,
            reranker_used: "top1".to_string(),
        })
    }
}

type NeedleLoadFn = unsafe extern "C" fn(data: *const c_char, len: u64) -> c_int;
type NeedleInitFn = unsafe extern "C" fn(
    system: *const c_char,
    tools_json: *const c_char,
    tool_index_path: *const c_char,
) -> c_int;
type NeedleCompleteV30Fn = unsafe extern "C" fn(
    text: *const c_char,
    max_new_tokens: c_int,
    out_buf: *mut c_char,
    buf_len: c_int,
) -> c_int;
type NeedleCompleteV31Fn = unsafe extern "C" fn(
    text: *const c_char,
    audio_data: *const std::ffi::c_void,
    audio_len: u64,
    max_new_tokens: c_int,
    out_buf: *mut c_char,
    buf_len: c_int,
) -> c_int;

/// Mode 2: Native Needle 3 C-Engine Reranker & Slot Extractor (with pure-Rust regex fallback)
/// Loads `libneedle.dylib` + `needle3.cact` directly via C FFI (`dlopen`) with zero Python overhead.
#[derive(Default)]
pub struct NeedleReranker;

pub type PrecisionReranker = NeedleReranker;
pub type ExtractedImportRecord = (String, Option<String>, Option<String>, Vec<String>);

impl NeedleReranker {
    pub fn new() -> Self {
        Self
    }

    fn tokenize(text: &str) -> HashSet<String> {
        let stop_words: HashSet<&str> = [
            "what", "is", "our", "the", "a", "an", "on", "in", "to", "for", "with", "does", "do",
            "how", "why", "where", "when", "who", "which", "are", "was", "were",
        ]
        .into_iter()
        .collect();

        text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .map(|w| w.to_lowercase())
            .filter(|w| w.len() > 1 && !stop_words.contains(w.as_str()))
            .collect()
    }

    fn find_needle_assets() -> Option<(PathBuf, PathBuf)> {
        let home = dirs::home_dir()?;
        let base_dir = home.join(".cache").join("cactus-needle").join("v3");
        let lib_names = [
            "libneedle3.dylib",
            "libneedle.dylib",
            "libneedle3.so",
            "libneedle.so",
        ];

        // Check known version folders first, then scan any version directory inside ~/.cache/cactus-needle/v3/
        let mut candidate_dirs = vec![base_dir.join("3.1.0"), base_dir.join("3.0.1")];
        if let Ok(entries) = std::fs::read_dir(&base_dir) {
            for entry in entries.flatten() {
                candidate_dirs.push(entry.path());
            }
        }

        for dir in candidate_dirs {
            let w = dir.join("needle3.cact");
            if w.exists() {
                for lib_name in &lib_names {
                    let l = dir.join(lib_name);
                    if l.exists() {
                        return Some((l, w));
                    }
                }
            }
        }
        None
    }

    fn run_needle_query(tools_json: String, input_text: String) -> Option<String> {
        if std::env::var_os("LIGHTMEM_NEEDLE_DISABLE").is_some() {
            return None;
        }
        type Req = (String, String, std::sync::mpsc::Sender<Option<String>>);
        static NEEDLE_WORKER: std::sync::OnceLock<
            Option<std::sync::Mutex<std::sync::mpsc::Sender<Req>>>,
        > = std::sync::OnceLock::new();

        let tx_mutex = NEEDLE_WORKER
            .get_or_init(|| {
                let (lib_path, weights_path) = Self::find_needle_assets()?;
                let weights_data = std::fs::read(&weights_path).ok()?;
                let lib_cstr = CString::new(lib_path.to_string_lossy().as_bytes()).ok()?;

                let (req_tx, req_rx) = std::sync::mpsc::channel::<Req>();
                let (init_tx, init_rx) = std::sync::mpsc::channel::<bool>();

                std::thread::spawn(move || unsafe {
                    let handle = dlopen(lib_cstr.as_ptr(), RTLD_LAZY);
                    if handle.is_null() {
                        let _ = init_tx.send(false);
                        return;
                    }

                    let load_sym = CString::new("needle_load").unwrap();
                    let init_sym = CString::new("needle_init").unwrap();
                    let comp_sym = CString::new("needle_complete").unwrap();
                    let trans_sym = CString::new("needle_transcribe").unwrap();

                    let load_ptr = dlsym(handle, load_sym.as_ptr());
                    let init_ptr = dlsym(handle, init_sym.as_ptr());
                    let comp_ptr = dlsym(handle, comp_sym.as_ptr());
                    let is_v31 = !dlsym(handle, trans_sym.as_ptr()).is_null();

                    if load_ptr.is_null() || init_ptr.is_null() || comp_ptr.is_null() {
                        let _ = init_tx.send(false);
                        return;
                    }

                    let needle_load: NeedleLoadFn = std::mem::transmute(load_ptr);
                    let needle_init: NeedleInitFn = std::mem::transmute(init_ptr);
                    let needle_complete_v30: NeedleCompleteV30Fn = std::mem::transmute(comp_ptr);
                    let needle_complete_v31: NeedleCompleteV31Fn = std::mem::transmute(comp_ptr);

                    if needle_load(
                        weights_data.as_ptr() as *const c_char,
                        weights_data.len() as u64,
                    ) < 0
                    {
                        let _ = init_tx.send(false);
                        return;
                    }

                    let _ = init_tx.send(true);
                    let sys_cstr = CString::new("").unwrap();

                    while let Ok((tools_str, text_str, reply_tx)) = req_rx.recv() {
                        let res = (|| -> Option<String> {
                            let tools_cstr = CString::new(tools_str.as_str()).ok()?;
                            if needle_init(sys_cstr.as_ptr(), tools_cstr.as_ptr(), std::ptr::null())
                                < 0
                            {
                                return None;
                            }
                            let text_cstr = CString::new(text_str.as_str()).ok()?;
                            let mut out_buf = vec![0u8; 65536];
                            let rc = if is_v31 {
                                needle_complete_v31(
                                    text_cstr.as_ptr(),
                                    std::ptr::null(),
                                    0,
                                    256,
                                    out_buf.as_mut_ptr() as *mut c_char,
                                    out_buf.len() as c_int,
                                )
                            } else {
                                needle_complete_v30(
                                    text_cstr.as_ptr(),
                                    256,
                                    out_buf.as_mut_ptr() as *mut c_char,
                                    out_buf.len() as c_int,
                                )
                            };
                            if rc < 0 {
                                return None;
                            }
                            Some(
                                CStr::from_bytes_until_nul(&out_buf)
                                    .ok()?
                                    .to_string_lossy()
                                    .into_owned(),
                            )
                        })();
                        let _ = reply_tx.send(res);
                    }
                });

                if init_rx.recv_timeout(Duration::from_secs(60)).ok()? {
                    Some(std::sync::Mutex::new(req_tx))
                } else {
                    None
                }
            })
            .as_ref()?;

        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        tx_mutex
            .lock()
            .ok()?
            .send((tools_json, input_text, reply_tx))
            .ok()?;
        reply_rx.recv_timeout(Duration::from_secs(30)).ok()?
    }

    /// Use Native Needle 3 C-FFI to extract structured (content, title, category, tags) from an imported JSON or text record
    pub fn extract_import_record_via_needle(raw_input: &str) -> Option<ExtractedImportRecord> {
        let tools_schema = serde_json::json!([{
            "name": "extract_memory",
            "description": "Extract memory content, title, category, and tags from raw JSON or text",
            "parameters": {
                "type": "object",
                "properties": {
                    "content": {"type": "string", "description": "Primary text or fact of the memory"},
                    "title": {"type": "string", "description": "Short summary title"},
                    "category": {
                        "type": "string",
                        "enum": [
                            "fact", "decision", "instruction", "preference", "learning",
                            "goal", "commitment", "artifact", "event", "relationship",
                            "observation", "error", "context", "password"
                        ]
                    },
                    "tags": {"type": "string", "description": "Comma-separated tags"}
                },
                "required": ["content", "title", "category"]
            }
        }]);

        let raw_json_str = Self::run_needle_query(tools_schema.to_string(), raw_input.to_string())?;

        let envelope: serde_json::Value = serde_json::from_str(&raw_json_str).ok()?;
        let (args, _) = accepted_arguments(&envelope)?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let category = args
            .get("category")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let mut tags = Vec::new();
        if let Some(t_str) = args.get("tags").and_then(|v| v.as_str()) {
            for part in t_str.split(',') {
                let clean = part.trim();
                if !clean.is_empty() {
                    tags.push(clean.to_string());
                }
            }
        }

        Some((content, title, category, tags))
    }

    /// Run native Needle 3 C library structured extraction directly from Rust
    fn extract_via_native_needle(question: &str, content: &str) -> Option<(String, f32)> {
        let slot = requested_slot(question)?;
        // Ask only for the requested field, avoiding invented mandatory app/port/OS values.
        let mut properties = serde_json::Map::new();
        let description = match slot {
            "port" => "Network port number",
            "os" => "Operating system (e.g. linux, windows, macos)",
            "application" => "Application, service, or tool name",
            "person_name" => "Full name or handle of the user or person",
            "url" => "Full URL or endpoint explicitly stated in the memory",
            _ => "Exact value explicitly stated in the memory",
        };
        properties.insert(slot.to_string(), serde_json::json!({"type": if slot == "port" { "integer" } else { "string" }, "description": description}));
        let tools = serde_json::json!([{"name":"extract_fact", "description":"Extract one grounded fact from the memory", "parameters":{"type":"object", "properties": properties, "required":[slot]}}]);
        let raw = Self::run_needle_query(tools.to_string(), content.to_string())?;
        let envelope: serde_json::Value = serde_json::from_str(&raw).ok()?;
        let (args, confidence) = accepted_arguments(&envelope)?;
        let value = args.get(slot)?;
        let answer = value
            .as_str()
            .map(str::to_owned)
            .or_else(|| value.as_i64().map(|n| n.to_string()))?;
        let answer = answer.trim();
        if answer.is_empty() || !grounded(answer, content) {
            return None;
        }
        if slot == "port" && !valid_port(answer) {
            return None;
        }
        Some((answer.to_string(), confidence))
    }

    /// Pure-Rust regex fallback for slots (ports, URLs, tokens)
    fn extract_slot_regex(question: &str, content: &str) -> Option<String> {
        let slot = requested_slot(question)?;
        let pattern = match slot {
            "port" => r"(?i)(?:port\s*[:=]?\s*|:)(\d{1,5})\b",
            "url" => r"(https?://[^\s]+)",
            "secret" => r"(?i)(?:password|token|secret|api key)\s*(?:is|:|=)\s*([^\s]+)",
            _ => return None,
        };
        let regex = Regex::new(pattern).ok()?;
        let mut values: Vec<String> = regex
            .captures_iter(content)
            .filter_map(|c| c.get(1))
            .map(|v| v.as_str().trim_end_matches('.').to_string())
            .collect();
        values.sort();
        values.dedup();
        if values.len() != 1 {
            return None;
        }
        let answer = values.remove(0);
        if slot == "port" && !valid_port(&answer) {
            return None;
        }
        Some(answer)
    }
}

impl Reranker for NeedleReranker {
    fn name(&self) -> &str {
        "needle-3"
    }

    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        let Some(best_candidate) = select_candidate(question, candidates) else {
            return Ok(no_evidence("none"));
        };
        if let Some((answer, confidence)) =
            Self::extract_via_native_needle(question, &best_candidate.memory.content)
        {
            return Ok(AnswerResult {
                answer,
                selected_memory: Some(best_candidate.memory.clone()),
                confidence,
                reranker_used: "needle-3".into(),
            });
        }
        if let Some(answer) = Self::extract_slot_regex(question, &best_candidate.memory.content) {
            return Ok(AnswerResult {
                answer,
                selected_memory: Some(best_candidate.memory.clone()),
                confidence: 0.0,
                reranker_used: "regex-fallback".into(),
            });
        }
        Ok(no_evidence("none"))
    }
}

fn no_evidence(provider: &str) -> AnswerResult {
    AnswerResult {
        answer: "Insufficient evidence in the retrieved memories to answer this question.".into(),
        selected_memory: None,
        confidence: 0.0,
        reranker_used: provider.into(),
    }
}

fn requested_slot(question: &str) -> Option<&'static str> {
    let q = question.to_lowercase();
    let tokens = NeedleReranker::tokenize(question);
    if tokens.contains("port")
        && !(q.starts_with("what service")
            || q.starts_with("which service")
            || q.starts_with("what application")
            || q.starts_with("which application")
            || q.starts_with("what runs"))
    {
        return Some("port");
    }
    if tokens.contains("url") || tokens.contains("endpoint") || tokens.contains("uri") {
        return Some("url");
    }
    if tokens.contains("password")
        || tokens.contains("token")
        || tokens.contains("secret")
        || q.contains("api key")
    {
        return Some("secret");
    }
    if tokens.contains("os") || q.contains("operating system") || tokens.contains("platform") {
        return Some("os");
    }
    if q.starts_with("who ") || tokens.contains("name") {
        return Some("person_name");
    }
    if tokens.contains("service")
        || tokens.contains("app")
        || tokens.contains("application")
        || tokens.contains("tool")
        || q.starts_with("what runs")
    {
        return Some("application");
    }
    None
}

fn valid_port(value: &str) -> bool {
    value.parse::<u16>().map(|port| port > 0).unwrap_or(false)
}

fn grounded(answer: &str, content: &str) -> bool {
    let pattern = format!(
        r"(?i)(?:^|[^\p{{L}}\p{{N}}_]){}(?:$|[^\p{{L}}\p{{N}}_])",
        regex::escape(answer)
    );
    Regex::new(&pattern)
        .map(|r| r.is_match(content))
        .unwrap_or(false)
}

/// Suppressed or ungrounded calls are never usable evidence. Missing confidence is unknown (0).
fn accepted_arguments(
    envelope: &serde_json::Value,
) -> Option<(&serde_json::Map<String, serde_json::Value>, f32)> {
    if let Some(ungrounded) = envelope.pointer("/validation/ungrounded") {
        if !ungrounded.as_array()?.is_empty() {
            return None;
        }
    }
    let calls = envelope
        .get("function_calls")
        .and_then(|v| v.as_array())
        .filter(|a| !a.is_empty())
        .or_else(|| {
            envelope
                .get("tool_calls")
                .and_then(|v| v.as_array())
                .filter(|a| !a.is_empty())
        })?;
    let confidence = envelope
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
        return None;
    }
    Some((calls.first()?.get("arguments")?.as_object()?, confidence))
}

/// Deterministic entity-aware candidate selection, separate from native extraction.
fn select_candidate<'a>(
    question: &str,
    candidates: &'a [ScoredMemory],
) -> Option<&'a ScoredMemory> {
    let generic: HashSet<&str> = [
        "port",
        "operating",
        "system",
        "os",
        "application",
        "app",
        "service",
        "tool",
        "url",
        "endpoint",
        "run",
        "runs",
        "running",
        "use",
        "uses",
        "used",
        "my",
        "me",
        "please",
        "tell",
    ]
    .into_iter()
    .collect();
    let query = NeedleReranker::tokenize(question);
    let specific: HashSet<_> = query
        .iter()
        .filter(|t| !generic.contains(t.as_str()))
        .collect();
    candidates
        .iter()
        .filter_map(|candidate| {
            let doc = NeedleReranker::tokenize(&candidate.memory.to_card_text());
            let hits = specific.iter().filter(|t| doc.contains(t.as_str())).count();
            // Require an entity/content match; generic words such as "port" alone are not evidence.
            if (!specific.is_empty() && hits == 0) || query.is_disjoint(&doc) {
                return None;
            }
            Some((candidate, hits as f32 * 3.5 + candidate.score))
        })
        .max_by(|a, b| {
            a.1.total_cmp(&b.1)
                .then_with(|| b.0.memory.id.cmp(&a.0.memory.id))
        })
        .map(|(candidate, _)| candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_suppressed_ungrounded_and_does_not_inflate_confidence() {
        assert!(accepted_arguments(
            &serde_json::json!({"suppressed_calls":[{"arguments":{"port":"1234"}}]})
        )
        .is_none());
        assert!(accepted_arguments(&serde_json::json!({"function_calls":[{"arguments":{"port":"1234"}}],"validation":{"ungrounded":["port"]}})).is_none());
        let raw =
            serde_json::json!({"function_calls":[{"arguments":{"port":"1234"}}],"confidence":0.2});
        assert_eq!(accepted_arguments(&raw).unwrap().1, 0.2);
    }
    #[test]
    fn slots_are_grounded_and_ambiguous_ports_abstain() {
        assert!(!grounded("80", "server runs on 8080"));
        assert!(grounded("8080", "server runs on 8080."));
        assert_eq!(
            requested_slot("what port does postgres service use?"),
            Some("port")
        );
        assert_eq!(
            NeedleReranker::extract_slot_regex("what port?", "port 1234 and port 4567"),
            None
        );
        assert_eq!(
            NeedleReranker::extract_slot_regex("what port?", "port 99999"),
            None
        );
    }
}
