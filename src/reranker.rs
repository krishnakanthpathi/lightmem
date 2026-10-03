use crate::models::{MemoryRecord, ScoredMemory};
use anyhow::Result;
use libc::{c_char, c_int, dlopen, dlsym, RTLD_LAZY};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::path::PathBuf;

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

    fn answer(&self, _question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        if candidates.is_empty() {
            return Ok(AnswerResult {
                answer: "No relevant memories found to answer this question.".to_string(),
                selected_memory: None,
                confidence: 0.0,
                reranker_used: "top1".to_string(),
            });
        }

        let best = &candidates[0];
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
type NeedleCompleteFn = unsafe extern "C" fn(
    text: *const c_char,
    max_new_tokens: c_int,
    out_buf: *mut c_char,
    buf_len: c_int,
) -> c_int;

/// Mode 2: Native Needle 3 C-Engine Reranker & Slot Extractor (with pure-Rust regex fallback)
/// Loads `libneedle.dylib` + `needle3.cact` directly via C FFI (`dlopen`) with zero Python overhead.
#[derive(Default)]
pub struct NeedleReranker;

pub type PrecisionReranker = NeedleReranker;

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
        let v301 = base_dir.join("3.0.1");
        let lib_path = v301.join("libneedle.dylib");
        let weights_path = v301.join("needle3.cact");
        if lib_path.exists() && weights_path.exists() {
            return Some((lib_path, weights_path));
        }

        // Scan any version directory inside ~/.cache/cactus-needle/v3/
        if let Ok(entries) = std::fs::read_dir(&base_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let l = p.join("libneedle.dylib");
                let w = p.join("needle3.cact");
                if l.exists() && w.exists() {
                    return Some((l, w));
                }
            }
        }
        None
    }

    /// Run native Needle 3 C library structured extraction directly from Rust
    fn extract_via_native_needle(question: &str, content: &str) -> Option<(String, f32)> {
        let (lib_path, weights_path) = Self::find_needle_assets()?;
        let weights_data = std::fs::read(&weights_path).ok()?;

        let tools_schema = serde_json::json!([{
            "name": "extract_facts",
            "description": "Extract structured entities and facts from the memory",
            "parameters": {
                "type": "object",
                "properties": {
                    "application": {"type": "string", "description": "Application, service, or tool name"},
                    "port": {"type": "integer", "description": "Network port number"},
                    "os": {"type": "string", "description": "Operating system (e.g. linux, windows, macos)"}
                },
                "required": ["application", "port", "os"]
            }
        }]);

        let lib_cstr = CString::new(lib_path.to_string_lossy().as_bytes()).ok()?;
        let sys_cstr = CString::new("").ok()?;
        let tools_cstr = CString::new(tools_schema.to_string()).ok()?;
        let text_cstr = CString::new(content).ok()?;

        let raw_json_str = unsafe {
            let handle = dlopen(lib_cstr.as_ptr(), RTLD_LAZY);
            if handle.is_null() {
                return None;
            }

            let load_sym = CString::new("needle_load").ok()?;
            let init_sym = CString::new("needle_init").ok()?;
            let comp_sym = CString::new("needle_complete").ok()?;

            let load_ptr = dlsym(handle, load_sym.as_ptr());
            let init_ptr = dlsym(handle, init_sym.as_ptr());
            let comp_ptr = dlsym(handle, comp_sym.as_ptr());

            if load_ptr.is_null() || init_ptr.is_null() || comp_ptr.is_null() {
                return None;
            }

            let needle_load: NeedleLoadFn = std::mem::transmute(load_ptr);
            let needle_init: NeedleInitFn = std::mem::transmute(init_ptr);
            let needle_complete: NeedleCompleteFn = std::mem::transmute(comp_ptr);

            if needle_load(
                weights_data.as_ptr() as *const c_char,
                weights_data.len() as u64,
            ) < 0
            {
                return None;
            }

            if needle_init(sys_cstr.as_ptr(), tools_cstr.as_ptr(), std::ptr::null()) < 0 {
                return None;
            }

            let mut out_buf = vec![0u8; 65536];
            let rc = needle_complete(
                text_cstr.as_ptr(),
                512,
                out_buf.as_mut_ptr() as *mut c_char,
                out_buf.len() as c_int,
            );
            if rc < 0 {
                return None;
            }

            CStr::from_ptr(out_buf.as_ptr() as *const c_char)
                .to_string_lossy()
                .into_owned()
        };

        let envelope: serde_json::Value = serde_json::from_str(&raw_json_str).ok()?;
        let ungrounded_empty = envelope
            .get("validation")
            .and_then(|v| v.get("ungrounded"))
            .and_then(|u| u.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true);
        let raw_conf = envelope
            .get("confidence")
            .and_then(|v| v.as_f64())
            .map(|c| c as f32)
            .unwrap_or(0.92);
        let conf = if ungrounded_empty {
            raw_conf.max(0.92)
        } else {
            raw_conf
        };

        let calls = envelope
            .get("function_calls")
            .and_then(|v| v.as_array())
            .or_else(|| envelope.get("suppressed_calls").and_then(|v| v.as_array()))?;
        let args = calls.first()?.get("arguments")?.as_object()?;

        let q_lower = question.to_lowercase();
        if q_lower.contains("port") {
            if let Some(p) = args.get("port") {
                let s = if let Some(n) = p.as_i64() {
                    n.to_string()
                } else {
                    p.as_str().unwrap_or("").to_string()
                };
                if !s.is_empty() && s != "0" {
                    return Some((s, conf));
                }
            }
        }

        if q_lower.contains("app")
            || q_lower.contains("service")
            || q_lower.contains("tool")
            || q_lower.contains("what is running")
        {
            if let Some(app) = args.get("application").and_then(|v| v.as_str()) {
                if !app.trim().is_empty() {
                    return Some((app.trim().to_string(), conf));
                }
            }
        }

        if q_lower.contains("os")
            || q_lower.contains("operating system")
            || q_lower.contains("platform")
            || q_lower.contains("where")
        {
            if let Some(os_val) = args.get("os").and_then(|v| v.as_str()) {
                if !os_val.trim().is_empty() {
                    return Some((os_val.trim().to_string(), conf));
                }
            }
        }

        None
    }

    /// Pure-Rust regex fallback for slots (ports, URLs, tokens)
    fn extract_slot_regex(question: &str, content: &str) -> String {
        let q_lower = question.to_lowercase();

        if q_lower.contains("port") {
            if let Ok(re) = Regex::new(r"(?i)(?:port\s*[:=]?\s*|:)(\d{2,5})\b") {
                if let Some(caps) = re.captures(content) {
                    if let Some(m) = caps.get(1) {
                        return m.as_str().to_string();
                    }
                }
            }
        }

        if q_lower.contains("url") || q_lower.contains("endpoint") || q_lower.contains("uri") {
            if let Ok(re) = Regex::new(r"(https?://[^\s]+|[a-zA-Z0-9+.-]+://[^\s]+)") {
                if let Some(m) = re.find(content) {
                    return m.as_str().to_string();
                }
            }
        }

        if q_lower.contains("token")
            || q_lower.contains("password")
            || q_lower.contains("secret")
            || q_lower.contains("api key")
        {
            if let Ok(re) = Regex::new(
                r"(?:=\s*|:\s*)(ghp_[A-Za-z0-9_]+|sk-[A-Za-z0-9_-]+|glpat-[A-Za-z0-9_-]+|[^\s]+)",
            ) {
                if let Some(caps) = re.captures(content) {
                    if let Some(m) = caps.get(1) {
                        return m.as_str().to_string();
                    }
                }
            }
        }

        content.to_string()
    }
}

impl Reranker for NeedleReranker {
    fn name(&self) -> &str {
        "needle-3"
    }

    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        if candidates.is_empty() {
            return Ok(AnswerResult {
                answer: "No relevant memories found to answer this question.".to_string(),
                selected_memory: None,
                confidence: 0.0,
                reranker_used: "needle-3".to_string(),
            });
        }

        let q_tokens = Self::tokenize(question);
        let mut best_candidate = &candidates[0];
        let mut highest_score = f32::MIN;

        for (idx, cand) in candidates.iter().take(5).enumerate() {
            let doc_text = format!(
                "{} {} {}",
                cand.memory.title,
                cand.memory.content,
                cand.memory.tags.join(" ")
            );
            let doc_tokens = Self::tokenize(&doc_text);
            let overlap = q_tokens.intersection(&doc_tokens).count() as f32;

            let rank_bonus = 1.0 / ((idx + 1) as f32);
            let combined = (cand.score * 2.0) + (overlap * 1.5) + (rank_bonus * 0.25);

            if combined > highest_score {
                highest_score = combined;
                best_candidate = cand;
            }
        }

        // Try Native Needle 3 C-FFI extraction first; fallback to regex if unavailable
        if let Some((slot_ans, needle_conf)) =
            Self::extract_via_native_needle(question, &best_candidate.memory.content)
        {
            return Ok(AnswerResult {
                answer: slot_ans,
                selected_memory: Some(best_candidate.memory.clone()),
                confidence: needle_conf,
                reranker_used: "needle-3".to_string(),
            });
        }

        let extracted = Self::extract_slot_regex(question, &best_candidate.memory.content);
        Ok(AnswerResult {
            answer: extracted,
            selected_memory: Some(best_candidate.memory.clone()),
            confidence: best_candidate.memory.confidence,
            reranker_used: "needle-3".to_string(),
        })
    }
}
