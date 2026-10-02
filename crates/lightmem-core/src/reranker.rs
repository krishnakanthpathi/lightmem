use crate::models::{MemoryRecord, ScoredMemory};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};

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
        let answer_text = best.memory.content.clone();

        Ok(AnswerResult {
            answer: answer_text,
            selected_memory: Some(best.memory.clone()),
            confidence: best.memory.confidence,
            reranker_used: "top1".to_string(),
        })
    }
}

/// Mode 2: Needle 3 Precision Disambiguator & Slot Extractor
pub struct NeedleReranker {
    pub python_path: Option<String>,
    pub script_path: Option<String>,
}

#[derive(Serialize)]
struct NeedleInputItem<'a> {
    id: &'a str,
    title: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct NeedlePayload<'a> {
    question: &'a str,
    candidates: Vec<NeedleInputItem<'a>>,
}

#[derive(Deserialize)]
struct NeedleOutput {
    selected_id: Option<String>,
    answer: Option<String>,
    confidence: Option<f32>,
    model: Option<String>,
}

impl Default for NeedleReranker {
    fn default() -> Self {
        Self::new()
    }
}

const EMBEDDED_SCRIPT: &str = include_str!("../scripts/needle_picker.py");

impl NeedleReranker {
    pub fn new() -> Self {
        Self {
            python_path: None,
            script_path: None,
        }
    }

    pub fn resolve_python(&self) -> String {
        if let Ok(p) = std::env::var("NEEDLE_PYTHON") {
            if !p.trim().is_empty() {
                return p.trim().to_string();
            }
        }
        if let Some(p) = &self.python_path {
            return p.clone();
        }
        let needle_venv = "/Users/krishnakanth/Projects/needle3-mac-bench/.venv/bin/python";
        if std::path::Path::new(needle_venv).exists() {
            return needle_venv.to_string();
        }
        "python3".to_string()
    }

    pub fn resolve_script(&self) -> String {
        if let Ok(s) = std::env::var("NEEDLE_SCRIPT") {
            if !s.trim().is_empty() {
                return s.trim().to_string();
            }
        }
        if let Some(s) = &self.script_path {
            return s.clone();
        }
        let candidate = "crates/lightmem-core/scripts/needle_picker.py";
        if std::path::Path::new(candidate).exists() {
            return candidate.to_string();
        }
        let abs =
            "/Users/krishnakanth/Projects/lightmem/crates/lightmem-core/scripts/needle_picker.py";
        if std::path::Path::new(abs).exists() {
            return abs.to_string();
        }
        // Write embedded script to ~/.lightmem/scripts/needle_picker.py if not found
        let user_script_dir = crate::config::LightMemConfig::config_dir().join("scripts");
        let user_script = user_script_dir.join("needle_picker.py");
        if std::fs::create_dir_all(&user_script_dir).is_ok() {
            let _ = std::fs::write(&user_script, EMBEDDED_SCRIPT);
            if user_script.exists() {
                return user_script.to_string_lossy().to_string();
            }
        }
        candidate.to_string()
    }
}

impl Reranker for NeedleReranker {
    fn name(&self) -> &str {
        "needle"
    }

    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        if candidates.is_empty() {
            return Ok(AnswerResult {
                answer: "No relevant memories found to answer this question.".to_string(),
                selected_memory: None,
                confidence: 0.0,
                reranker_used: "needle".to_string(),
            });
        }

        let python = self.resolve_python();
        let script = self.resolve_script();

        let payload = NeedlePayload {
            question,
            candidates: candidates
                .iter()
                .take(5)
                .map(|c| NeedleInputItem {
                    id: &c.memory.id,
                    title: &c.memory.title,
                    content: &c.memory.content,
                })
                .collect(),
        };

        let json_input = serde_json::to_string(&payload)?;

        let child = Command::new(&python)
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();

        if let Ok(mut proc) = child {
            if let Some(mut stdin) = proc.stdin.take() {
                let _ = stdin.write_all(json_input.as_bytes());
            }

            if let Ok(output) = proc.wait_with_output() {
                if output.status.success() {
                    let out_str = String::from_utf8_lossy(&output.stdout);
                    if let Ok(parsed) = serde_json::from_str::<NeedleOutput>(out_str.trim()) {
                        let selected_mem = parsed
                            .selected_id
                            .as_deref()
                            .and_then(|sid| candidates.iter().find(|c| c.memory.id == sid))
                            .map(|c| c.memory.clone())
                            .or_else(|| Some(candidates[0].memory.clone()));

                        let answer_str = parsed
                            .answer
                            .unwrap_or_else(|| candidates[0].memory.content.clone());

                        return Ok(AnswerResult {
                            answer: answer_str,
                            selected_memory: selected_mem,
                            confidence: parsed.confidence.unwrap_or(0.9),
                            reranker_used: parsed.model.unwrap_or_else(|| "needle-3".to_string()),
                        });
                    }
                }
            }
        }

        // Graceful fallback to Top-1 if Needle process fails
        Top1Reranker.answer(question, candidates)
    }
}
