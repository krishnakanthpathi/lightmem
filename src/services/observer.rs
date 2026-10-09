use crate::models::{LightMemConfig, MemoryRecord, MemoryType};
use crate::services::reranker::OllamaReranker;
use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObservedCandidate {
    pub title: String,
    pub content: String,
    pub category: MemoryType,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub provenance: String,
}

impl ObservedCandidate {
    pub fn new(
        title: impl Into<String>,
        content: impl Into<String>,
        category: MemoryType,
        tags: Vec<String>,
        confidence: f32,
    ) -> Self {
        Self {
            title: title.into(),
            content: content.into(),
            category,
            tags,
            confidence: confidence.clamp(0.0, 1.0),
            provenance: "observed".to_string(),
        }
    }

    pub fn to_memory_record(&self) -> MemoryRecord {
        MemoryRecord::new(
            self.category,
            self.title.clone(),
            self.content.clone(),
            self.tags.clone(),
            self.confidence,
            Some(self.provenance.clone()),
        )
    }
}

pub struct ObserverService;

impl ObserverService {
    /// Ingests conversation or note text, extracts atomic candidate facts/observations,
    /// and auto-infers categories, titles, and tags.
    pub fn extract(
        raw_text: &str,
        category_override: Option<MemoryType>,
        extra_tags: Option<&[String]>,
        confidence_threshold: Option<f32>,
        reranker_spec: Option<&str>,
        config: Option<&LightMemConfig>,
    ) -> Result<Vec<ObservedCandidate>> {
        let trimmed = raw_text.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        let threshold = confidence_threshold.unwrap_or(0.85);

        // 1. If an LLM reranker/extractor is requested or configured, try LLM extraction first
        if let Some(spec) = reranker_spec {
            let lower = spec.trim().to_lowercase();
            if lower.starts_with("ollama")
                || lower.starts_with("openai")
                || lower.starts_with("llm")
            {
                if let Some(cfg) = config {
                    if let Ok(candidates) = Self::extract_via_llm(trimmed, spec, cfg) {
                        if !candidates.is_empty() {
                            return Ok(Self::post_process_candidates(
                                candidates,
                                category_override,
                                extra_tags,
                                threshold,
                            ));
                        }
                    }
                }
            }
        }

        // 2. Local deterministic NLP & regex heuristic extractor (100% offline, 0ms latency)
        let candidates = Self::extract_local(trimmed);
        Ok(Self::post_process_candidates(
            candidates,
            category_override,
            extra_tags,
            threshold,
        ))
    }

    /// Post-process candidates: apply category override, union tags, clamp confidence, filter threshold, deduplicate.
    fn post_process_candidates(
        mut candidates: Vec<ObservedCandidate>,
        category_override: Option<MemoryType>,
        extra_tags: Option<&[String]>,
        threshold: f32,
    ) -> Vec<ObservedCandidate> {
        let mut seen_keys = HashSet::new();
        let mut filtered = Vec::new();

        for mut c in candidates.drain(..) {
            if let Some(cat) = category_override {
                c.category = cat;
            }

            if let Some(tags) = extra_tags {
                for t in tags {
                    let clean = t.trim().to_lowercase();
                    if !clean.is_empty()
                        && !c
                            .tags
                            .iter()
                            .any(|existing| existing.eq_ignore_ascii_case(&clean))
                    {
                        c.tags.push(clean);
                    }
                }
            }

            if c.confidence < threshold {
                continue;
            }

            // Normalization key for deduplication
            let norm_key = format!(
                "{}:{}",
                c.category.as_str(),
                c.content.trim().to_lowercase()
            );

            if seen_keys.insert(norm_key) {
                filtered.push(c);
            }
        }

        filtered
    }

    /// Extract atomic candidate memories using local pattern matching and dialogue analysis.
    pub fn extract_local(input: &str) -> Vec<ObservedCandidate> {
        let turns = Self::parse_turns(input);
        let mut candidates = Vec::new();

        for turn in turns {
            let statement_candidates = Self::extract_from_statement(&turn);
            candidates.extend(statement_candidates);
        }

        candidates
    }

    /// Parse raw text into individual speech turns, bullet points, or declarative sentences.
    fn parse_turns(input: &str) -> Vec<String> {
        // 1. Try parsing as JSON array of message objects: [{"role": "...", "content": "..."}]
        if input.trim_start().starts_with('[') {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(input) {
                if let Some(arr) = val.as_array() {
                    let mut turns = Vec::new();
                    for item in arr {
                        if let Some(content) = item.get("content").and_then(|c| c.as_str()) {
                            let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                            turns.push(format!("{}: {}", role, content));
                        } else if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            let speaker = item
                                .get("speaker")
                                .and_then(|s| s.as_str())
                                .unwrap_or("user");
                            turns.push(format!("{}: {}", speaker, text));
                        }
                    }
                    if !turns.is_empty() {
                        return turns;
                    }
                }
            }
        }

        // 2. Try JSONL lines
        let mut jsonl_turns = Vec::new();
        for line in input.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(content) = val.get("content").and_then(|c| c.as_str()) {
                        let role = val.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                        jsonl_turns.push(format!("{}: {}", role, content));
                        continue;
                    }
                }
            }
        }
        if !jsonl_turns.is_empty() {
            return jsonl_turns;
        }

        // 3. Fallback: line-by-line / turn-by-turn segmentation
        let mut turns = Vec::new();
        for line in input.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Strip markdown lists or bullet markers
            let cleaned = if let Some(stripped) = trimmed.strip_prefix("- ") {
                stripped
            } else if let Some(stripped) = trimmed.strip_prefix("* ") {
                stripped
            } else if let Some(stripped) = trimmed.strip_prefix("• ") {
                stripped
            } else {
                trimmed
            };

            if cleaned.len() >= 10 {
                turns.push(cleaned.to_string());
            }
        }

        turns
    }

    /// Extract atomic candidate facts from a single dialogue turn or statement.
    fn extract_from_statement(raw_stmt: &str) -> Vec<ObservedCandidate> {
        let (speaker, content_body) = Self::strip_speaker_prefix(raw_stmt);
        let cleaned = Self::clean_conversational_filler(content_body);
        if cleaned.len() < 12 {
            return Vec::new();
        }

        // Split into candidate sentences if compound
        let sentences = Self::split_into_sentences(&cleaned);
        let mut candidates = Vec::new();

        for s in sentences {
            let s_trimmed = s.trim();
            if s_trimmed.len() < 12 {
                continue;
            }

            if let Some(candidate) = Self::analyze_sentence(speaker.as_deref(), s_trimmed) {
                candidates.push(candidate);
            }
        }

        candidates
    }

    /// Strip speaker tags like "User:", "Assistant:", "Human:", "[User]:", "Alice:"
    fn strip_speaker_prefix(s: &str) -> (Option<String>, &str) {
        let re = Regex::new(r"^(?i)(?:\[?)(user|assistant|human|system|ai|bot|client|agent|person|[A-Z][a-z]+)(?:\]?):\s*").unwrap();
        if let Some(mat) = re.find(s) {
            let speaker = s[..mat.end()]
                .trim()
                .trim_end_matches(':')
                .trim_matches(|c| c == '[' || c == ']')
                .to_string();
            let rest = &s[mat.end()..];
            (Some(speaker), rest.trim())
        } else {
            (None, s.trim())
        }
    }

    /// Remove polite filler words and chat preambles.
    fn clean_conversational_filler(text: &str) -> String {
        let mut cur = text.trim();

        let filler_prefixes = [
            "sure thing,",
            "sure,",
            "okay,",
            "ok,",
            "understood,",
            "got it,",
            "yes,",
            "yeah,",
            "yep,",
            "hey,",
            "hi there,",
            "hello,",
            "thanks,",
            "thank you,",
            "certainly,",
            "absolutely,",
            "great,",
            "awesome,",
            "note that",
            "please note that",
            "remember that",
            "fyi,",
            "by the way,",
            "can you help me with",
            "could you please",
            "i wanted to let you know that",
            "i wanted to mention that",
            "i would like to note that",
            "just to let you know,",
        ];

        let mut changed = true;
        while changed {
            changed = false;
            let lower = cur.to_lowercase();
            for prefix in &filler_prefixes {
                if lower.starts_with(prefix) {
                    cur = cur[prefix.len()..].trim();
                    changed = true;
                    break;
                }
            }
            if let Some(stripped) =
                cur.strip_prefix(|c: char| c == '-' || c == '*' || c == ':' || c == '.')
            {
                cur = stripped.trim();
                changed = true;
            }
        }

        cur.to_string()
    }

    /// Split compound dialogue into distinct assertive sentences.
    fn split_into_sentences(text: &str) -> Vec<String> {
        let mut results = Vec::new();
        // Split on period, newline, semicolon, or exclamation mark followed by whitespace
        for chunk in text.split(['\n', ';']) {
            let c_trim = chunk.trim();
            if c_trim.is_empty() {
                continue;
            }
            // Split further by sentence boundaries ('. ' or '! ')
            let mut start = 0;
            let chars: Vec<char> = c_trim.chars().collect();
            let len = chars.len();

            for i in 0..len {
                if (chars[i] == '.' || chars[i] == '!')
                    && (i + 1 == len || chars[i + 1].is_whitespace())
                {
                    let sentence: String = chars[start..=i].iter().collect();
                    let s_clean = sentence
                        .trim()
                        .trim_end_matches('.')
                        .trim_end_matches('!')
                        .trim();
                    if s_clean.len() >= 12 {
                        results.push(s_clean.to_string());
                    }
                    start = i + 1;
                }
            }
            if start < len {
                let remainder: String = chars[start..len].iter().collect();
                let r_clean = remainder.trim();
                if r_clean.len() >= 12 {
                    results.push(r_clean.to_string());
                }
            }
        }

        if results.is_empty() && text.trim().len() >= 12 {
            results.push(text.trim().to_string());
        }

        results
    }

    /// Analyzes an atomic sentence to see if it qualifies as an observation/fact.
    fn analyze_sentence(speaker: Option<&str>, s: &str) -> Option<ObservedCandidate> {
        let (inline_speaker, raw_body) = Self::strip_speaker_prefix(s);
        let effective_speaker = inline_speaker.as_deref().or(speaker);
        let cleaned_body = Self::clean_conversational_filler(raw_body);
        let mut text = cleaned_body.trim();

        // Strip leading conjunctions like "Also ", "And ", "So "
        for conj in &["also ", "and ", "so ", "furthermore ", "additionally "] {
            if text.to_lowercase().starts_with(conj) {
                text = text[conj.len()..].trim();
            }
        }

        if text.len() < 10 {
            return None;
        }

        let lower = text.to_lowercase();

        // 1. Filter out pure questions
        if lower.ends_with('?')
            || lower.starts_with("what ")
            || lower.starts_with("how ")
            || lower.starts_with("where ")
            || lower.starts_with("why ")
            || lower.starts_with("when ")
            || lower.starts_with("who ")
            || lower.starts_with("is there ")
            || lower.starts_with("can you ")
            || lower.starts_with("could you ")
        {
            return None;
        }

        // 2. Filter out trivial acknowledgements
        if lower == "thank you"
            || lower == "thanks"
            || lower == "sounds good"
            || lower == "looks good"
            || lower == "perfect"
            || lower == "no problem"
            || lower == "you're welcome"
        {
            return None;
        }

        // 3. Match strong patterns:
        let mut category = MemoryType::Observation;
        let mut confidence = 0.88f32;
        let mut matched = false;

        // Preference detection
        if lower.contains("i prefer")
            || lower.contains("i like")
            || lower.contains("my favorite")
            || lower.contains("my default")
            || lower.contains("i always use")
            || lower.contains("user prefers")
            || lower.contains("i dislike")
            || lower.contains("i hate")
            || lower.contains("i want to use")
        {
            category = MemoryType::Preference;
            confidence = 0.94;
            matched = true;
        }
        // Decision detection
        else if lower.contains("we decided")
            || lower.contains("i decided")
            || lower.contains("decision is")
            || lower.contains("let's go with")
            || lower.contains("we chose")
            || lower.contains("i chose")
            || lower.contains("switched to")
            || lower.contains("switching to")
            || lower.contains("we will use")
        {
            category = MemoryType::Decision;
            confidence = 0.95;
            matched = true;
        }
        // Instruction / Rule detection
        else if lower.contains("always remember")
            || lower.contains("always ensure")
            || lower.contains("make sure to")
            || lower.starts_with("always ")
            || lower.starts_with("never ")
            || lower.contains("rule:")
            || lower.contains("instruction:")
            || lower.contains("do not ")
            || lower.contains("don't ")
        {
            category = MemoryType::Instruction;
            confidence = 0.93;
            matched = true;
        }
        // Goal / Commitment detection
        else if lower.contains("goal is")
            || lower.contains("objective is")
            || lower.contains("target date")
            || lower.contains("deadline is")
            || lower.contains("milestone:")
            || lower.contains("we need to deliver")
        {
            category = MemoryType::Goal;
            confidence = 0.90;
            matched = true;
        }
        // Error / Bug detection
        else if lower.contains("error:")
            || lower.contains("failed with")
            || lower.contains("exception:")
            || lower.contains("panic:")
            || lower.contains("bug in")
            || lower.contains("incident:")
        {
            category = MemoryType::Error;
            confidence = 0.92;
            matched = true;
        }
        // Explicit Learning / Takeaway
        else if lower.contains("learned that")
            || lower.contains("takeaway:")
            || lower.contains("realized that")
        {
            category = MemoryType::Learning;
            confidence = 0.90;
            matched = true;
        }
        // Direct Facts / Infrastructure / Technical specs
        else if lower.contains("runs on port")
            || lower.contains("port is")
            || lower.contains("hosted at")
            || lower.contains("located at")
            || lower.contains("path is")
            || lower.contains("version is")
            || lower.contains("my name is")
            || lower.contains("my pan")
            || lower.contains("my email")
            || lower.contains("password is")
            || lower.contains("secret is")
            || lower.contains("endpoint is")
            || lower.contains("configured with")
        {
            category = MemoryType::Fact;
            confidence = 0.92;
            matched = true;
        }
        // Explicit Observation
        else if lower.contains("observed that")
            || lower.contains("noticed that")
            || lower.contains("turns out")
            || lower.contains("found that")
            || lower.contains("observation:")
            || lower.contains("note:")
        {
            category = MemoryType::Observation;
            confidence = 0.89;
            matched = true;
        }
        // General assertive declarative statement if from user or assistant with enough substance
        else if text.split_whitespace().count() >= 4
            && (text.contains(" is ")
                || text.contains(" are ")
                || text.contains(" has ")
                || text.contains(" using "))
        {
            category = MemoryType::infer(text);
            confidence = 0.85;
            matched = true;
        }

        if !matched {
            return None;
        }

        // Formulate clear content
        let mut final_content = text.trim().to_string();
        if let Some(spk) = effective_speaker {
            if spk.eq_ignore_ascii_case("user") || spk.eq_ignore_ascii_case("human") {
                if final_content.to_lowercase().starts_with("i prefer ") {
                    final_content = format!("User prefers {}", &final_content[9..]);
                } else if final_content.to_lowercase().starts_with("i like ") {
                    final_content = format!("User likes {}", &final_content[7..]);
                } else if final_content.to_lowercase().starts_with("i want to ") {
                    final_content = format!("User wants to {}", &final_content[10..]);
                } else if final_content.to_lowercase().starts_with("i always use ") {
                    final_content = format!("User always uses {}", &final_content[13..]);
                } else if final_content.to_lowercase().starts_with("my ") {
                    final_content = format!("User's {}", &final_content[3..]);
                }
            }
        }

        let title = Self::generate_title(&final_content, category);
        let tags = Self::generate_tags(&final_content, category);

        Some(ObservedCandidate::new(
            title,
            final_content,
            category,
            tags,
            confidence,
        ))
    }

    /// Derive a concise, meaningful title from the statement content.
    fn generate_title(content: &str, category: MemoryType) -> String {
        let words: Vec<&str> = content.split_whitespace().collect();
        if words.is_empty() {
            return format!("Observed {}", category.as_str());
        }

        // If content starts with explicit label like "Note: ..."
        if let Some((prefix, rest)) = content.split_once(':') {
            if prefix.split_whitespace().count() <= 3 && !rest.trim().is_empty() {
                let rest_words: Vec<&str> = rest.split_whitespace().take(5).collect();
                return format!("{}: {}", prefix.trim(), rest_words.join(" "));
            }
        }

        let take_n = words.len().min(6);
        let mut candidate_title = words[..take_n].join(" ");
        if candidate_title.len() > 50 {
            candidate_title.truncate(47);
            candidate_title.push_str("...");
        }

        candidate_title
    }

    /// Extract domain-specific tags and category tags.
    fn generate_tags(content: &str, category: MemoryType) -> Vec<String> {
        let mut tags = Vec::new();
        tags.push(category.as_str().to_string());

        let lower = content.to_lowercase();
        let tech_keywords = [
            "postgres",
            "postgresql",
            "mysql",
            "sqlite",
            "redis",
            "mongodb",
            "docker",
            "kubernetes",
            "linux",
            "macos",
            "windows",
            "rust",
            "python",
            "javascript",
            "typescript",
            "react",
            "vue",
            "nextjs",
            "port",
            "api",
            "auth",
            "token",
            "jwt",
            "tls",
            "ssl",
            "ssh",
            "theme",
            "dark-mode",
            "light-mode",
            "font",
            "editor",
            "vim",
            "vscode",
            "terminal",
            "zsh",
            "bash",
            "cli",
            "git",
            "github",
            "ci",
            "cd",
            "test",
            "deploy",
            "server",
            "database",
            "ui",
            "ux",
        ];

        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '-')
            .filter(|w| !w.is_empty())
            .collect();

        for kw in &tech_keywords {
            let matched = if kw.len() <= 3 {
                words.contains(kw)
            } else {
                lower.contains(kw)
            };
            if matched && !tags.contains(&kw.to_string()) {
                tags.push(kw.to_string());
            }
        }

        tags
    }

    /// Optional LLM extraction via Ollama/OpenAI API.
    fn extract_via_llm(
        text: &str,
        reranker_spec: &str,
        config: &LightMemConfig,
    ) -> Result<Vec<ObservedCandidate>> {
        let model_spec = reranker_spec
            .strip_prefix("ollama:")
            .or_else(|| reranker_spec.strip_prefix("openai:"))
            .or_else(|| reranker_spec.strip_prefix("llm:"))
            .unwrap_or(reranker_spec);

        let _ollama = OllamaReranker::with_api_key(
            config.effective_ollama_url(),
            Some(model_spec.to_string()),
            config.effective_ollama_api_key(),
        );

        let system_prompt = "You are an expert AI agent memory extractor. Extract key atomic long-term memories (facts, preferences, decisions, rules, goals, observations) from the given conversation or notes.\n\
            Ignore conversational filler, greetings, and trivial chit-chat.\n\
            Return ONLY a valid JSON array of objects with schema:\n\
            [\n\
              {\n\
                \"title\": \"Short descriptive title\",\n\
                \"content\": \"Self-contained atomic memory statement\",\n\
                \"category\": \"fact\" | \"preference\" | \"decision\" | \"instruction\" | \"goal\" | \"observation\" | \"error\" | \"learning\",\n\
                \"tags\": [\"tag1\", \"tag2\"],\n\
                \"confidence\": 0.95\n\
              }\n\
            ]";

        let chat_url = format!("{}/v1/chat/completions", config.effective_ollama_url());
        let payload = serde_json::json!({
            "model": model_spec,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": text }
            ],
            "temperature": 0.0,
            "max_tokens": 1024
        });

        let mut req = ureq::post(&chat_url).timeout(Duration::from_secs(30));
        if let Some(ref key) = config.effective_ollama_api_key() {
            req = req.set(
                "Authorization",
                &crate::models::config::format_auth_header(key),
            );
        }

        let resp = req
            .send_json(payload)
            .context("Ollama LLM extraction request failed")?;
        let json_resp: serde_json::Value = resp.into_json()?;
        let raw_content = json_resp
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|m| m.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");

        Self::parse_llm_json_response(raw_content)
    }

    /// Parse LLM JSON array response into ObservedCandidates.
    pub fn parse_llm_json_response(raw: &str) -> Result<Vec<ObservedCandidate>> {
        let trimmed = raw.trim();
        // Handle markdown code blocks
        let clean_json = if let Some(start) = trimmed.find("```json") {
            let after = &trimmed[start + 7..];
            if let Some(end) = after.find("```") {
                &after[..end]
            } else {
                after
            }
        } else if let Some(start) = trimmed.find("```") {
            let after = &trimmed[start + 3..];
            if let Some(end) = after.find("```") {
                &after[..end]
            } else {
                after
            }
        } else {
            trimmed
        };

        let parsed: serde_json::Value =
            serde_json::from_str(clean_json.trim()).context("Failed parsing LLM output as JSON")?;

        let arr = parsed
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("LLM response is not a JSON array"))?;
        let mut results = Vec::new();

        for item in arr {
            let content = item
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if content.is_empty() {
                continue;
            }

            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .map(|t| t.trim().to_string())
                .unwrap_or_else(|| content.chars().take(40).collect());

            let cat_str = item
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("observation");
            let category = match cat_str.to_lowercase().as_str() {
                "fact" => MemoryType::Fact,
                "decision" => MemoryType::Decision,
                "instruction" | "rule" => MemoryType::Instruction,
                "preference" => MemoryType::Preference,
                "learning" => MemoryType::Learning,
                "goal" => MemoryType::Goal,
                "error" | "bug" => MemoryType::Error,
                _ => MemoryType::infer(content),
            };

            let tags = item
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_else(|| vec![category.as_str().to_string()]);

            let confidence = item
                .get("confidence")
                .and_then(|v| v.as_f64())
                .map(|f| f as f32)
                .unwrap_or(0.90);

            results.push(ObservedCandidate::new(
                title, content, category, tags, confidence,
            ));
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_local_dialogue_preferences_and_facts() {
        let chat = r#"
        User: Hi there!
        Assistant: Hello! How can I help you today?
        User: I prefer dark mode and high contrast.
        Assistant: Got it. I'll make sure dark mode is enabled.
        User: Also remember that PostgreSQL 16 runs on port 5433.
        Assistant: Understood, noted port 5433 for Postgres.
        "#;

        let candidates = ObserverService::extract_local(chat);
        assert!(
            !candidates.is_empty(),
            "Should extract candidate memories from chat"
        );

        let pref = candidates
            .iter()
            .find(|c| c.category == MemoryType::Preference);
        assert!(pref.is_some(), "Should extract dark mode preference");
        assert!(pref.unwrap().content.to_lowercase().contains("dark mode"));

        let fact = candidates.iter().find(|c| c.content.contains("5433"));
        assert!(fact.is_some(), "Should extract PostgreSQL 5433 fact");
        assert!(fact.unwrap().tags.contains(&"postgres".to_string()));
    }

    #[test]
    fn test_extract_from_json_turns() {
        let json_chat = r#"[
            {"role": "user", "content": "We decided to switch from Redis to SQLite for our agent memory."},
            {"role": "assistant", "content": "Great choice for local performance."},
            {"role": "user", "content": "Always ensure strict clippy checks pass in CI."}
        ]"#;

        let candidates = ObserverService::extract_local(json_chat);
        assert_eq!(candidates.len(), 2);

        let decision = candidates
            .iter()
            .find(|c| c.category == MemoryType::Decision);
        assert!(decision.is_some());
        assert!(decision.unwrap().content.contains("SQLite"));

        let instruction = candidates
            .iter()
            .find(|c| c.category == MemoryType::Instruction);
        assert!(instruction.is_some());
        assert!(instruction.unwrap().content.contains("clippy"));
    }

    #[test]
    fn test_clean_conversational_filler() {
        let dirty = "Sure thing, understood, we decided to deploy on AWS.";
        let cleaned = ObserverService::clean_conversational_filler(dirty);
        assert_eq!(cleaned, "we decided to deploy on AWS.");
    }
}
