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
type NeedleResetFn = unsafe extern "C" fn();
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

/// Mode 2: Native Needle 3 C-Engine Reranker & Slot Extractor
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
            "did", "how", "why", "where", "when", "who", "which", "are", "was", "were", "my", "me",
            "your", "user", "users", "use", "uses", "used", "run", "runs", "running", "please",
            "tell",
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
                    let reset_sym = CString::new("needle_reset").unwrap();
                    let comp_sym = CString::new("needle_complete").unwrap();
                    let trans_sym = CString::new("needle_transcribe").unwrap();

                    let load_ptr = dlsym(handle, load_sym.as_ptr());
                    let init_ptr = dlsym(handle, init_sym.as_ptr());
                    let reset_ptr = dlsym(handle, reset_sym.as_ptr());
                    let comp_ptr = dlsym(handle, comp_sym.as_ptr());
                    let is_v31 = !dlsym(handle, trans_sym.as_ptr()).is_null();

                    if load_ptr.is_null() || init_ptr.is_null() || comp_ptr.is_null() {
                        let _ = init_tx.send(false);
                        return;
                    }

                    let needle_load: NeedleLoadFn = std::mem::transmute(load_ptr);
                    let needle_init: NeedleInitFn = std::mem::transmute(init_ptr);
                    let needle_reset: Option<NeedleResetFn> = if reset_ptr.is_null() {
                        None
                    } else {
                        Some(std::mem::transmute::<*mut libc::c_void, NeedleResetFn>(
                            reset_ptr,
                        ))
                    };
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
                            if let Some(reset_fn) = needle_reset {
                                reset_fn();
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
        let tools_schema = r#"[{"name":"extract_memory","description":"Extract memory content, title, category, and tags from raw JSON or text","parameters":{"type":"object","properties":{"content":{"type":"string","description":"Primary text or fact of the memory"},"title":{"type":"string","description":"Short summary title"},"category":{"type":"string","enum":["fact","decision","instruction","preference","learning","goal","commitment","artifact","event","relationship","observation","error","context","password"]},"tags":{"type":"string","description":"Comma-separated tags"}},"required":["content","title","category"]}}]"#;

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
        let slot = requested_slot(question);
        let query_tokens = Self::tokenize(question);
        let is_core_triad = matches!(slot.as_str(), "port" | "os" | "service");
        let tools_json = if is_core_triad {
            r#"[{"name":"extract_facts","description":"Extract service name, port, and os","parameters":{"type":"object","properties":{"service":{"type":"string","description":"Service or application name (e.g. Redis, Postgres, Kokoro)"},"port":{"type":"integer","description":"Port number"},"os":{"type":"string","description":"Operating system (e.g. Linux, macOS, Windows)"}},"required":["service","port","os"]}}]"#.to_string()
        } else {
            format!(
                r#"[{{"name":"extract_fact","description":"Extract {slot} from text","parameters":{{"type":"object","properties":{{"{slot}":{{"type":"string","description":"The {slot}"}}}},"required":["{slot}"]}}}}]"#
            )
        };

        const NEGATION_OR_ABSENCE_TOKENS: &[&str] = &[
            "not",
            "none",
            "null",
            "unknown",
            "unspecified",
            "unrecorded",
            "recorded",
            "missing",
            "absent",
            "empty",
            "na",
        ];

        let extract_from_raw = |raw_json: &str| -> Option<(String, f32)> {
            let envelope: serde_json::Value = serde_json::from_str(raw_json).ok()?;
            let (args, confidence) = accepted_arguments_for_slot(&envelope, Some(&slot))?;
            let value = args.get(&slot)?;
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
            let ans_tokens = Self::tokenize(answer);
            if !ans_tokens.is_empty()
                && ans_tokens.iter().all(|t| {
                    doc_contains_token(&query_tokens, t)
                        || NEGATION_OR_ABSENCE_TOKENS.contains(&t.as_str())
                })
            {
                return None;
            }
            if matches!(slot.as_str(), "id" | "number" | "port" | "name") {
                let lower_ans = answer.to_lowercase();
                if lower_ans.ends_with(".pdf")
                    || lower_ans.ends_with(".png")
                    || lower_ans.ends_with(".jpg")
                    || lower_ans.ends_with(".jpeg")
                    || lower_ans.ends_with(".doc")
                    || lower_ans.ends_with(".docx")
                    || lower_ans.ends_with(".md")
                {
                    return None;
                }
            }
            if matches!(slot.as_str(), "id" | "number" | "port")
                && (answer.split_whitespace().count() > 4 || confidence < 0.20)
            {
                return None;
            }
            let content_tokens = Self::tokenize(content);
            if (content_tokens.contains("not") || content_tokens.contains("never"))
                && (content_tokens.contains("recorded")
                    || content_tokens.contains("known")
                    || content_tokens.contains("specified")
                    || content_tokens.contains("provided")
                    || content_tokens.contains("found")
                    || content_tokens.contains("available")
                    || content_tokens.contains("set"))
            {
                return None;
            }
            Some((answer.to_string(), confidence))
        };

        let mut inputs = Vec::with_capacity(7);
        if let Some(focused) = focus_clause_for_question(question, content) {
            if let Some((_, rhs)) = focused.split_once(':') {
                let rhs = rhs.trim().trim_end_matches('.');
                if !rhs.is_empty() {
                    inputs.push(format!("Extract fact: {} is {}", slot, rhs));
                }
            } else if let (Some(open), Some(close)) = (focused.find('('), focused.rfind(')')) {
                if open + 1 < close {
                    let inner = focused[open + 1..close].trim();
                    if !inner.is_empty() {
                        inputs.push(format!("Extract fact: {} is {}", slot, inner));
                    }
                }
            }
            inputs.push(format!("Extract fact: {}", focused));
            inputs.push(focused);
        }
        if is_core_triad {
            inputs.push(content.to_string());
            inputs.push(format!("Extract fact: {}", content));
        } else {
            inputs.push(format!("Extract fact: {}", content));
            inputs.push(content.to_string());
        }

        for input in inputs {
            if let Some(raw) = Self::run_needle_query(tools_json.clone(), input) {
                if let Some(res) = extract_from_raw(&raw) {
                    return Some(res);
                }
            }
        }
        None
    }
}

impl Reranker for NeedleReranker {
    fn name(&self) -> &str {
        "needle-3"
    }

    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        for candidate in rank_candidates(question, candidates) {
            if let Some((answer, confidence)) =
                Self::extract_via_native_needle(question, &candidate.memory.content)
            {
                return Ok(AnswerResult {
                    answer,
                    selected_memory: Some(candidate.memory.clone()),
                    confidence,
                    reranker_used: "needle-3".into(),
                });
            }
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

/// Dynamically infer the target slot noun from the question's syntax without hardcoded domain lists.
fn requested_slot(question: &str) -> String {
    let normalize_slot = |s: &str| -> String {
        match s {
            "ports" => "port".to_string(),
            "no" | "num" | "numbers" => "number".to_string(),
            "ids" => "id".to_string(),
            other => other.to_string(),
        }
    };
    let ordered: Vec<String> = question
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 1)
        .collect();

    match ordered.first().map(String::as_str) {
        Some("who") => return "name".to_string(),
        Some("where") => return "location".to_string(),
        Some("when") => return "time".to_string(),
        Some("what" | "which") if ordered.len() >= 2 => {
            const AUX_VERBS: &[&str] = &[
                "is", "are", "was", "were", "does", "do", "did", "has", "have", "had", "can",
                "will", "should",
            ];
            const MODIFIERS: &[&str] = &[
                "the", "a", "an", "my", "our", "your", "their", "its", "user", "users", "default",
                "primary", "current", "main", "active", "official", "exact", "standard",
            ];
            const ACTION_VERBS: &[&str] = &["runs", "run", "uses", "use"];
            let is_verb = |w: &str| AUX_VERBS.contains(&w) || ACTION_VERBS.contains(&w);

            let rest = &ordered[1..];
            let skip_idx = rest
                .iter()
                .position(|w| !AUX_VERBS.contains(&w.as_str()) && !MODIFIERS.contains(&w.as_str()))
                .unwrap_or(rest.len());
            let trimmed = &rest[skip_idx..];

            if let Some(prep_idx) = trimmed
                .iter()
                .position(|w| matches!(w.as_str(), "of" | "for"))
            {
                if prep_idx > 0
                    && prep_idx + 1 < trimmed.len()
                    && !trimmed[..prep_idx].iter().any(|w| is_verb(w.as_str()))
                {
                    if prep_idx >= 2
                        && trimmed[prep_idx - 2] == "operating"
                        && trimmed[prep_idx - 1] == "system"
                    {
                        return "os".to_string();
                    }
                    return normalize_slot(trimmed[prep_idx - 1].as_str());
                }
            }

            let mod_idx = rest
                .iter()
                .position(|w| !MODIFIERS.contains(&w.as_str()))
                .unwrap_or(rest.len());
            let after_mods = &rest[mod_idx..];
            if after_mods.len() >= 3
                && after_mods[0] == "operating"
                && after_mods[1] == "system"
                && is_verb(after_mods[2].as_str())
            {
                return "os".to_string();
            }
            if after_mods.len() >= 2
                && !is_verb(after_mods[0].as_str())
                && is_verb(after_mods[1].as_str())
            {
                return normalize_slot(after_mods[0].as_str());
            }
        }
        _ => {}
    }

    if ordered
        .iter()
        .any(|t| matches!(t.as_str(), "port" | "ports"))
        || ordered
            .windows(2)
            .any(|w| w[0] == "listening" && w[1] == "on")
    {
        return "port".to_string();
    }
    if ordered.iter().any(|t| t == "os")
        || ordered
            .windows(2)
            .any(|w| w[0] == "operating" && w[1] == "system")
    {
        return "os".to_string();
    }

    const TRAILING_IGNORE: &[&str] = &[
        "use",
        "uses",
        "used",
        "run",
        "runs",
        "running",
        "is",
        "are",
        "was",
        "were",
        "do",
        "does",
        "did",
        "on",
        "in",
        "at",
        "to",
        "for",
        "of",
        "with",
        "by",
        "from",
        "listening",
        "located",
        "stored",
        "configured",
    ];
    ordered
        .iter()
        .rev()
        .find(|t| !TRAILING_IGNORE.contains(&t.as_str()))
        .map(|s| normalize_slot(s.as_str()))
        .unwrap_or_else(|| "value".to_string())
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

/// Ungrounded calls are never usable evidence. Grounded suppressed_calls are accepted when function_calls is empty.
fn accepted_arguments(
    envelope: &serde_json::Value,
) -> Option<(&serde_json::Map<String, serde_json::Value>, f32)> {
    accepted_arguments_for_slot(envelope, None)
}

fn accepted_arguments_for_slot<'a>(
    envelope: &'a serde_json::Value,
    target_slot: Option<&str>,
) -> Option<(&'a serde_json::Map<String, serde_json::Value>, f32)> {
    if let Some(ungrounded) = envelope.pointer("/validation/ungrounded") {
        let arr = ungrounded.as_array()?;
        if let Some(slot) = target_slot {
            let dot_slot = format!(".{}", slot);
            if arr.iter().any(|v| {
                v.as_str()
                    .map(|s| s == slot || s.ends_with(&dot_slot))
                    .unwrap_or(false)
            }) {
                return None;
            }
        } else if !arr.is_empty() {
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
        })
        .or_else(|| {
            envelope
                .get("suppressed_calls")
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
const GENERIC_TOKENS: &[&str] = &[
    "service",
    "services",
    "server",
    "servers",
    "daemon",
    "proxy",
    "node",
    "host",
    "cluster",
    "engine",
    "app",
    "application",
    "system",
    "port",
    "ports",
    "os",
    "endpoint",
    "url",
    "uri",
    "default",
    "primary",
    "current",
    "main",
    "active",
    "located",
    "listening",
    "stored",
    "configured",
    "card",
    "cards",
    "id",
    "ids",
    "no",
    "num",
    "number",
    "numbers",
    "roll",
    "code",
    "document",
    "documents",
    "record",
    "records",
    "file",
    "files",
    "detail",
    "details",
];

fn token_stem(token: &str) -> &str {
    if token.len() >= 5
        && token.ends_with('s')
        && !token.ends_with("ss")
        && !token.ends_with("is")
        && !token.ends_with("us")
        && !token.ends_with("os")
    {
        &token[..token.len() - 1]
    } else {
        token
    }
}

fn doc_contains_token(doc: &HashSet<String>, token: &str) -> bool {
    if doc.contains(token) {
        return true;
    }
    let stem = token_stem(token);
    doc.iter().any(|d| token_stem(d) == stem)
}

fn is_target_slot_token(token: &str, dynamic_slot: &str) -> bool {
    let dynamic_slot_stem = token_stem(dynamic_slot);
    token_stem(token) == dynamic_slot_stem
        || (dynamic_slot == "number" && matches!(token, "no" | "num" | "number" | "numbers"))
}

fn focus_clause_for_question(question: &str, content: &str) -> Option<String> {
    let dynamic_slot = requested_slot(question);
    let query = NeedleReranker::tokenize(question);
    let anchor_tokens: HashSet<&str> = query
        .iter()
        .map(String::as_str)
        .filter(|&t| !is_target_slot_token(t, &dynamic_slot) && !GENERIC_TOKENS.contains(&t))
        .collect();
    if anchor_tokens.is_empty() {
        return None;
    }

    let splitter = Regex::new(r"(?i)\n|;|\.\s+|,\s+and\s+|\s+and\s+|\s+-\s+|,\s+").ok()?;
    let clauses: Vec<&str> = splitter
        .split(content)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if clauses.len() < 2 {
        return None;
    }

    let mut best_clause: Option<(&str, usize)> = None;
    let mut tied = false;

    for clause in clauses {
        let clause_tokens = NeedleReranker::tokenize(clause);
        let anchor_hits = anchor_tokens
            .iter()
            .filter(|&&t| doc_contains_token(&clause_tokens, t))
            .count();
        if anchor_hits == 0 || clause_tokens.len() <= anchor_hits {
            continue;
        }
        match best_clause {
            None => {
                best_clause = Some((clause, anchor_hits));
                tied = false;
            }
            Some((_, best_hits)) if anchor_hits > best_hits => {
                best_clause = Some((clause, anchor_hits));
                tied = false;
            }
            Some((_, best_hits)) if anchor_hits == best_hits => {
                tied = true;
            }
            _ => {}
        }
    }

    if tied {
        return None;
    }
    best_clause.map(|(clause, _)| clause.to_string())
}

fn rank_candidates<'a>(question: &str, candidates: &'a [ScoredMemory]) -> Vec<&'a ScoredMemory> {
    const RELATIVE_TOKENS: &[&str] = &[
        "dad", "father", "mom", "mother", "brother", "sister", "annaya",
    ];
    let dynamic_slot = requested_slot(question);
    let query = NeedleReranker::tokenize(question);
    let specific: HashSet<&str> = query
        .iter()
        .map(String::as_str)
        .filter(|&t| !is_target_slot_token(t, &dynamic_slot))
        .collect();
    let (generic_tokens, anchor_tokens): (HashSet<&str>, HashSet<&str>) = specific
        .iter()
        .copied()
        .partition(|t| GENERIC_TOKENS.contains(t));
    let query_mentions_relative = query
        .iter()
        .any(|q| RELATIVE_TOKENS.iter().any(|r| token_stem(q) == *r));

    let mut scored: Vec<(&'a ScoredMemory, f32)> = candidates
        .iter()
        .filter_map(|candidate| {
            let doc = NeedleReranker::tokenize(&candidate.memory.to_card_text());
            let anchor_hits = anchor_tokens
                .iter()
                .filter(|&&t| doc_contains_token(&doc, t))
                .count();
            let generic_hits = generic_tokens
                .iter()
                .filter(|&&t| doc_contains_token(&doc, t))
                .count();
            let hits = anchor_hits + generic_hits;
            let any_query_hit = query.iter().any(|q| doc_contains_token(&doc, q));
            // Require an anchor entity/content match when present; generic or target slot words alone are not evidence.
            if (!anchor_tokens.is_empty() && anchor_hits == 0)
                || (!specific.is_empty() && hits == 0)
                || !any_query_hit
            {
                return None;
            }
            let fact_bonus = if candidate.memory.category == crate::models::MemoryType::Fact {
                0.8
            } else {
                0.0
            };
            let relative_penalty = if !query_mentions_relative
                && RELATIVE_TOKENS.iter().any(|r| doc_contains_token(&doc, r))
            {
                2.0
            } else {
                0.0
            };
            let score = anchor_hits as f32 * 5.0 + generic_hits as f32 * 0.5 + fact_bonus
                - relative_penalty
                + candidate.score;
            Some((candidate, score))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then_with(|| a.0.memory.id.cmp(&b.0.memory.id))
    });
    scored.into_iter().map(|(c, _)| c).collect()
}

fn select_candidate<'a>(
    question: &str,
    candidates: &'a [ScoredMemory],
) -> Option<&'a ScoredMemory> {
    rank_candidates(question, candidates).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_grounded_suppressed_rejects_ungrounded_and_does_not_inflate_confidence() {
        assert!(accepted_arguments(
            &serde_json::json!({"suppressed_calls":[{"arguments":{"port":"1234"}}],"confidence":0.25})
        )
        .is_some());
        assert!(accepted_arguments(&serde_json::json!({"function_calls":[{"arguments":{"port":"1234"}}],"validation":{"ungrounded":["port"]}})).is_none());
        let raw =
            serde_json::json!({"function_calls":[{"arguments":{"port":"1234"}}],"confidence":0.2});
        assert_eq!(accepted_arguments(&raw).unwrap().1, 0.2);
    }
    #[test]
    fn slots_are_grounded_and_ports_validated() {
        assert!(!grounded("80", "server runs on 8080"));
        assert!(grounded("8080", "server runs on 8080."));
        assert_eq!(
            requested_slot("what port does postgres service use?"),
            "port"
        );
        assert_eq!(requested_slot("what service runs on port 8880?"), "service");
        assert_eq!(
            requested_slot("what are the users codeforces profile"),
            "profile"
        );
        assert_eq!(requested_slot("What is the user's name?"), "name");
        assert_eq!(requested_slot("What is the Atlas endpoint?"), "endpoint");
        assert_eq!(requested_slot("What is the port of Redis?"), "port");
        assert_eq!(requested_slot("What is the OS of Helios?"), "os");
        assert_eq!(
            requested_slot("What is the operating system of Helios?"),
            "os"
        );
        assert_eq!(requested_slot("What default port does Orion use?"), "port");
        assert_eq!(requested_slot("What is Nexus listening on?"), "port");
        assert_eq!(requested_slot("Which port is Redis listening on?"), "port");
        assert_eq!(
            requested_slot("What is the endpoint for Atlas?"),
            "endpoint"
        );
        assert_eq!(requested_slot("Who is the team lead?"), "name");
        assert_eq!(requested_slot("Where is the backup stored?"), "location");
        assert_eq!(requested_slot("When does the job run?"), "time");
        assert!(valid_port("6379"));
        assert!(!valid_port("99999"));
    }

    fn make_candidate(id: &str, content: &str, score: f32) -> ScoredMemory {
        let mut memory = MemoryRecord::new(
            crate::models::MemoryType::Fact,
            "Test fact".to_string(),
            content.to_string(),
            vec![],
            0.9,
            None,
        );
        memory.id = id.to_string();
        ScoredMemory {
            memory,
            score,
            bm25_rank: Some(1),
            vector_rank: Some(1),
        }
    }

    #[test]
    fn select_candidate_abstains_when_only_generic_nouns_match() {
        let helios = make_candidate("1", "Helios service runs on port 7654 on macOS.", 0.85);
        let haproxy = make_candidate("2", "haproxy proxy runs on port 8404 on Linux.", 0.82);
        let nexus = make_candidate("3", "Nexus service runs on port 6543 on Linux.", 0.80);
        let candidates = vec![helios, haproxy, nexus];

        assert!(
            select_candidate("What port does Kafka service use?", &candidates).is_none(),
            "Should abstain when only generic noun service matches"
        );
        assert!(
            select_candidate("What port does Cassandra proxy use?", &candidates).is_none(),
            "Should abstain when only generic noun proxy matches"
        );
        assert!(
            select_candidate("What service runs on port 80?", &candidates).is_none(),
            "Should abstain when only generic noun port matches"
        );

        let selected_helios =
            select_candidate("What port does Helios service use?", &candidates).unwrap();
        assert_eq!(selected_helios.memory.id, "1");

        let selected_port =
            select_candidate("What service runs on port 6543?", &candidates).unwrap();
        assert_eq!(selected_port.memory.id, "3");
    }

    #[test]
    fn focus_clause_selects_matching_entity_clause_in_multi_entity_memory() {
        let content = "haproxy runs on port 8404 on Linux and envoy runs on port 9901 on Linux.";
        let focused_envoy =
            focus_clause_for_question("What port does envoy use?", content).unwrap();
        assert!(focused_envoy.contains("envoy"));
        assert!(focused_envoy.contains("9901"));
        assert!(!focused_envoy.contains("haproxy"));

        let focused_haproxy =
            focus_clause_for_question("What port does haproxy use?", content).unwrap();
        assert!(focused_haproxy.contains("haproxy"));
        assert!(focused_haproxy.contains("8404"));
        assert!(!focused_haproxy.contains("envoy"));

        assert!(focus_clause_for_question(
            "What port does Redis use?",
            "Redis runs on port 6379 on Linux."
        )
        .is_none());

        if std::env::var_os("LIGHTMEM_NEEDLE_DISABLE").is_none()
            && NeedleReranker::find_needle_assets().is_some()
        {
            let (envoy_port, _) =
                NeedleReranker::extract_via_native_needle("What port does envoy use?", content)
                    .expect("Should extract envoy port");
            assert_eq!(envoy_port, "9901");

            let (haproxy_port, _) =
                NeedleReranker::extract_via_native_needle("What port does haproxy use?", content)
                    .expect("Should extract haproxy port");
            assert_eq!(haproxy_port, "8404");
        }
    }

    #[test]
    fn candidate_loop_matches_plural_stems_and_falls_back_when_first_candidate_is_insufficient() {
        let c1_insufficient = make_candidate("1", "My fathers name has not been recorded.", 0.90);
        let c2_valid = make_candidate("2", "Pathi Srinivas is my fathers name", 0.85);
        let c3_family_list = make_candidate(
            "3",
            "User's Family Details: - Father: Pathi Srinivas - Mother: Mistri Venkata Annapurna Devi",
            0.80,
        );
        let candidates = vec![c1_insufficient, c2_valid.clone(), c3_family_list.clone()];

        // 1. "father" in query must match "fathers" in c1 & c2 via stem/prefix normalization
        let selected = select_candidate("what is my father name", &candidates)
            .expect("Singular 'father' should match 'fathers' in candidate");
        assert_eq!(selected.memory.id, "1");

        // 2. When Native Needle is active, NeedleReranker::answer must try candidate #1,
        // see it is insufficient, move to candidate #2, and extract "Pathi Srinivas" (not "Father")!
        if std::env::var_os("LIGHTMEM_NEEDLE_DISABLE").is_none()
            && NeedleReranker::find_needle_assets().is_some()
        {
            let res = NeedleReranker
                .answer("what is my father name", &candidates)
                .unwrap();
            assert_eq!(res.selected_memory.as_ref().unwrap().id, "2");
            assert_eq!(res.answer, "Pathi Srinivas");

            let res_family = NeedleReranker
                .answer("what is my father name", &[c3_family_list])
                .unwrap();
            assert_eq!(res_family.answer, "Pathi Srinivas");
        }
    }

    #[test]
    fn pan_card_no_and_college_id_queries_extract_exact_identifiers() {
        assert_eq!(requested_slot("what is my pan card no"), "number");
        assert_eq!(requested_slot("what is my college roll no"), "number");
        assert_eq!(requested_slot("what is my college id"), "id");

        let mut dad_pan = make_candidate(
            "1",
            "Dad's PAN card is stored at /home/krishnakanth/Documents/Family Vault/files/dad/dad_PAN.pdf",
            0.48,
        );
        dad_pan.memory.category = crate::models::MemoryType::Artifact;
        dad_pan.memory.title = "Dad's PAN Card Location".to_string();

        let user_pan = make_candidate("2", "User's PAN ID is HAQPP8118D.", 0.44);

        let mut family_docs = make_candidate(
            "6",
            "The vault stores sensitive personal documents including Aadhaar cards, PAN cards, educational certificates (10th, Inter, BTech), payslips, and vehicle registration certificates (RCs) for various family members.",
            0.41,
        );
        family_docs.memory.category = crate::models::MemoryType::Context;

        let pan_candidates = vec![dad_pan, user_pan, family_docs];
        let selected_pan = select_candidate("what is my pan card no", &pan_candidates).unwrap();
        assert_eq!(selected_pan.memory.id, "2");

        let mut removal_decision = make_candidate(
            "10",
            "The user decided to remove 'krishna_CollegeID.pdf' from their files and registry, keeping 'krishna_CollegeID_22A31A05I7.pdf' instead.",
            0.90,
        );
        removal_decision.memory.category = crate::models::MemoryType::Decision;
        removal_decision.memory.title = "Removal of College ID".to_string();

        let user_identity_docs = make_candidate(
            "11",
            "The user (krishna) has various academic and identity documents stored, including BTech marks memos, 10th and Inter certificates, Aadhaar, PAN (HAQPP8118D), and College ID (22A31A05I7).",
            0.89,
        );

        let college_candidates = vec![removal_decision.clone(), user_identity_docs.clone()];
        let selected_college =
            select_candidate("what is my college id", &college_candidates).unwrap();
        assert_eq!(selected_college.memory.id, "11");

        if std::env::var_os("LIGHTMEM_NEEDLE_DISABLE").is_none()
            && NeedleReranker::find_needle_assets().is_some()
        {
            let res_pan = NeedleReranker
                .answer("what is my pan card no", &pan_candidates)
                .unwrap();
            assert_eq!(res_pan.answer, "HAQPP8118D");

            let res_college = NeedleReranker
                .answer("what is my college id", &college_candidates)
                .unwrap();
            assert_eq!(res_college.answer, "22A31A05I7");

            let inter_record = make_candidate(
                "12",
                "The user (or the subject of the provided document) is Pathi Krishna Kanth, who completed their Intermediate education from the Board of Intermediate Education, Andhra Pradesh, India. Registered No: 2203226828. They graduated in May 2022 from KSN Junior College, Samalkot, with an A Grade and a total score of 945.",
                0.41,
            );
            let roll_candidates = vec![removal_decision, inter_record, user_identity_docs];
            let res_roll = NeedleReranker
                .answer("what is my college roll no", &roll_candidates)
                .unwrap();
            assert!(res_roll.answer == "22A31A05I7" || res_roll.answer == "2203226828");
        }
    }
}
