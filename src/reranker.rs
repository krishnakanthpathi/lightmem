use crate::models::{MemoryRecord, ScoredMemory};
use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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

/// Mode 2: Pure-Rust Precision Disambiguator & Slot Extractor
/// Re-scores top candidates using token overlap + exact entity cues, and extracts specific factual slots (ports, URLs, tokens, versions).
#[derive(Default)]
pub struct PrecisionReranker;

impl PrecisionReranker {
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

    /// Extract a specific factual slot from content if the question asks for a narrow entity (port, url, version, token/key)
    fn extract_slot(question: &str, content: &str) -> String {
        let q_lower = question.to_lowercase();

        // Port extraction
        if q_lower.contains("port") {
            if let Ok(re) = Regex::new(r"(?i)(?:port\s*[:=]?\s*|:)(\d{2,5})\b") {
                if let Some(caps) = re.captures(content) {
                    if let Some(m) = caps.get(1) {
                        return m.as_str().to_string();
                    }
                }
            }
        }

        // URL / Endpoint extraction
        if q_lower.contains("url") || q_lower.contains("endpoint") || q_lower.contains("uri") {
            if let Ok(re) = Regex::new(r"(https?://[^\s]+|[a-zA-Z0-9+.-]+://[^\s]+)") {
                if let Some(m) = re.find(content) {
                    return m.as_str().to_string();
                }
            }
        }

        // Secret / Token / Key extraction
        if q_lower.contains("token")
            || q_lower.contains("password")
            || q_lower.contains("secret")
            || q_lower.contains("api key")
        {
            if let Ok(re) = Regex::new(r"(?:=\s*|:\s*)(ghp_[A-Za-z0-9_]+|sk-[A-Za-z0-9_-]+|glpat-[A-Za-z0-9_-]+|[^\s]+)") {
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

impl Reranker for PrecisionReranker {
    fn name(&self) -> &str {
        "precision-rust"
    }

    fn answer(&self, question: &str, candidates: &[ScoredMemory]) -> Result<AnswerResult> {
        if candidates.is_empty() {
            return Ok(AnswerResult {
                answer: "No relevant memories found to answer this question.".to_string(),
                selected_memory: None,
                confidence: 0.0,
                reranker_used: "precision-rust".to_string(),
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

            // Combine hybrid retrieval score with exact lexical overlap and rank prior
            let rank_bonus = 1.0 / ((idx + 1) as f32);
            let combined = (cand.score * 2.0) + (overlap * 1.5) + (rank_bonus * 0.25);

            if combined > highest_score {
                highest_score = combined;
                best_candidate = cand;
            }
        }

        let extracted = Self::extract_slot(question, &best_candidate.memory.content);

        Ok(AnswerResult {
            answer: extracted,
            selected_memory: Some(best_candidate.memory.clone()),
            confidence: best_candidate.memory.confidence,
            reranker_used: "precision-rust".to_string(),
        })
    }
}
