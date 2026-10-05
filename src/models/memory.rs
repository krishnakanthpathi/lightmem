use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,
    Decision,
    Instruction,
    Preference,
    Learning,
    Goal,
    Commitment,
    Artifact,
    Event,
    Relationship,
    Observation,
    Error,
    Context,
    #[value(alias = "passwords")]
    Password,
}

impl MemoryType {
    pub const ALL: &'static [MemoryType] = &[
        MemoryType::Fact,
        MemoryType::Decision,
        MemoryType::Instruction,
        MemoryType::Preference,
        MemoryType::Learning,
        MemoryType::Goal,
        MemoryType::Commitment,
        MemoryType::Artifact,
        MemoryType::Event,
        MemoryType::Relationship,
        MemoryType::Observation,
        MemoryType::Error,
        MemoryType::Context,
        MemoryType::Password,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::Fact => "fact",
            MemoryType::Decision => "decision",
            MemoryType::Instruction => "instruction",
            MemoryType::Preference => "preference",
            MemoryType::Learning => "learning",
            MemoryType::Goal => "goal",
            MemoryType::Commitment => "commitment",
            MemoryType::Artifact => "artifact",
            MemoryType::Event => "event",
            MemoryType::Relationship => "relationship",
            MemoryType::Observation => "observation",
            MemoryType::Error => "error",
            MemoryType::Context => "context",
            MemoryType::Password => "password",
        }
    }

    /// Automatically infer the most appropriate MemoryType from raw text content across all 14 categories
    pub fn infer(content: &str) -> Self {
        let lower = content.trim().to_lowercase();

        // 0. Explicit start-of-line category prefixes take precedence over mid-sentence substrings
        if lower.starts_with("password:")
            || lower.starts_with("secret:")
            || lower.starts_with("credential:")
        {
            return MemoryType::Password;
        }
        if lower.starts_with("error:")
            || lower.starts_with("bug:")
            || lower.starts_with("panic:")
            || lower.starts_with("exception:")
        {
            return MemoryType::Error;
        }
        if lower.starts_with("preference:") {
            return MemoryType::Preference;
        }
        if lower.starts_with("rule:")
            || lower.starts_with("instruction:")
            || lower.starts_with("runbook:")
            || lower.starts_with("how to ")
        {
            return MemoryType::Instruction;
        }
        if lower.starts_with("decision:") {
            return MemoryType::Decision;
        }
        if lower.starts_with("goal:")
            || lower.starts_with("objective:")
            || lower.starts_with("okr:")
            || lower.starts_with("milestone:")
        {
            return MemoryType::Goal;
        }
        if lower.starts_with("todo:")
            || lower.starts_with("commitment:")
            || lower.starts_with("action item:")
        {
            return MemoryType::Commitment;
        }
        if lower.starts_with("learning:") || lower.starts_with("til:") {
            return MemoryType::Learning;
        }
        if lower.starts_with("event:") || lower.starts_with("incident:") {
            return MemoryType::Event;
        }
        if lower.starts_with("relationship:") {
            return MemoryType::Relationship;
        }
        if lower.starts_with("observation:") {
            return MemoryType::Observation;
        }
        if lower.starts_with("artifact:") {
            return MemoryType::Artifact;
        }
        if lower.starts_with("context:") || lower.starts_with("background:") {
            return MemoryType::Context;
        }
        if lower.starts_with("fact:") {
            return MemoryType::Fact;
        }

        // 1. Password / Secret / Credential
        let has_secret_token = lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .any(|t| {
                t.starts_with("ghp_")
                    || t.starts_with("github_pat_")
                    || t.starts_with("sk-")
                    || t.starts_with("glpat-")
                    || t.starts_with("xoxb-")
                    || (t.starts_with("akia")
                        && t.len() >= 16
                        && t.chars().all(|c| c.is_ascii_alphanumeric()))
            });
        let has_cred_url = lower
            .split_whitespace()
            .any(|tok| tok.contains("://") && tok.contains('@') && tok.matches(':').count() >= 2);

        if has_secret_token
            || has_cred_url
            || lower.contains("sk_live_")
            || lower.contains("sk_test_")
            || lower.contains("password:")
            || lower.contains("password is")
            || lower.contains("api_key")
            || lower.contains("api key")
            || lower.contains("secret_key")
            || lower.contains("secret key")
            || lower.contains("signing secret")
            || lower.contains("webhook secret")
            || lower.contains("whsec_")
            || lower.contains("auth token")
            || lower.contains("private key")
        {
            return MemoryType::Password;
        }

        // 2. Error / Bug / Failure
        if lower.contains("panic:")
            || lower.contains("panic in ")
            || lower.contains("fix panic")
            || lower.contains("panicked")
            || lower.contains("segfault")
            || lower.contains("stack trace")
            || lower.contains("failed with")
            || lower.contains("connection refused")
            || lower.contains("connection timeout")
            || lower.contains("pool exhausted")
            || lower.contains("deadlock")
            || lower.contains("timeout error")
            || lower.contains("out of memory")
            || lower.contains("exception:")
        {
            return MemoryType::Error;
        }

        // 3. Decision / Architecture Choice (checked before bare "dark mode" / Preference)
        if lower.contains("we decided")
            || lower.contains("decided to ")
            || lower.contains("agreed to ")
            || lower.contains("standardized on ")
            || lower.contains("switched from ")
            || lower.contains("migrating to ")
            || lower.contains("chosen over ")
            || lower.contains("is deprecated")
            || lower.contains("adopted ")
        {
            return MemoryType::Decision;
        }

        // 4. Security / engineering imperatives checked before generic "prefer "
        if lower.contains("prevent ")
            || lower.starts_with("never ")
            || lower.contains(" never ")
            || lower.contains("must always ")
            || lower.contains("must never ")
            || lower.starts_with("do not ")
            || lower.contains(" do not ")
            || lower.starts_with("don't ")
            || lower.contains(" don't ")
        {
            return MemoryType::Instruction;
        }

        // 5. Preference / Style (checked before "always " so "Always prefer..." maps to Preference)
        if lower.contains("prefers ")
            || lower.contains("prefer ")
            || lower.contains("user prefers")
            || lower.contains("likes to use ")
            || lower.contains("favorite ")
            || lower.contains("dark mode")
            || lower.contains("keybindings")
        {
            return MemoryType::Preference;
        }

        // 6. Instruction / Rule / Runbook
        if lower.starts_with("always ")
            || lower.contains("make sure to ")
            || lower.contains("step 1")
        {
            return MemoryType::Instruction;
        }

        // 6. Goal / Objective / Target
        if lower.starts_with("goal:")
            || lower.starts_with("objective:")
            || lower.contains("objective:")
            || lower.contains("roadmap ")
            || lower.contains("our goal is")
            || lower.contains("target is to ")
            || lower.contains("aiming to ")
            || lower.contains("okr:")
            || lower.contains("milestone:")
        {
            return MemoryType::Goal;
        }

        // 7. Commitment / Todo / Deadline
        if lower.starts_with("todo:")
            || lower.starts_with("commitment:")
            || lower.starts_with("action item:")
            || lower.contains("promised to ")
            || lower.contains("committed to ")
            || lower.contains("will deliver by ")
            || lower.contains("deadline is ")
            || lower.contains("due by ")
        {
            return MemoryType::Commitment;
        }

        // 8. Learning / Insight / Root Cause
        if lower.starts_with("learning:")
            || lower.starts_with("til:")
            || lower.contains("learned that ")
            || lower.contains("lesson learned")
            || lower.contains("discovered that ")
            || lower.contains("turns out that ")
            || lower.contains("root cause was ")
            || lower.contains("realized that ")
            || lower.contains("to avoid process freeze")
        {
            return MemoryType::Learning;
        }

        // 9. Event / Incident / Deployment
        if lower.starts_with("event:")
            || lower.starts_with("incident:")
            || lower.contains("incident occurred")
            || lower.contains("outage on ")
            || lower.contains("incident on ")
            || lower.contains("deployed v")
            || lower.contains("released v")
            || lower.contains("meeting with ")
            || lower.contains("postmortem")
        {
            return MemoryType::Event;
        }

        // 10. Relationship / Ownership / Team
        if lower.starts_with("relationship:")
            || lower.contains("reports to ")
            || lower.contains("is the maintainer of ")
            || lower.contains("is the owner of ")
            || lower.contains("tech lead for ")
            || lower.contains("manages the ")
            || lower.contains("works closely with ")
        {
            return MemoryType::Relationship;
        }

        // 11. Observation / Profiling / Telemetry Notice
        if lower.starts_with("observation:")
            || lower.contains("noticed that ")
            || lower.contains("observed that ")
            || lower.contains("profiling shows ")
            || lower.contains("metrics show ")
            || lower.contains("cpu spikes when ")
            || lower.contains("latency increases when ")
        {
            return MemoryType::Observation;
        }

        // 12. Artifact / File / Reference Spec
        if lower.starts_with("artifact:")
            || lower.contains("dockerfile")
            || lower.contains("openapi")
            || lower.contains("schema definition")
            || lower.contains("specification saved in ")
            || lower.contains("located at `")
            || lower.contains("stored in `src/")
        {
            return MemoryType::Artifact;
        }

        // 13. Context / Background / Scope
        if lower.starts_with("context:")
            || lower.starts_with("background:")
            || lower.contains("currently working on ")
            || lower.contains("for this sprint")
            || lower.contains("project scope ")
        {
            return MemoryType::Context;
        }

        // 14. Default to Fact
        MemoryType::Fact
    }
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for MemoryType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "fact" => Ok(MemoryType::Fact),
            "decision" => Ok(MemoryType::Decision),
            "instruction" => Ok(MemoryType::Instruction),
            "preference" => Ok(MemoryType::Preference),
            "learning" => Ok(MemoryType::Learning),
            "goal" => Ok(MemoryType::Goal),
            "commitment" => Ok(MemoryType::Commitment),
            "artifact" => Ok(MemoryType::Artifact),
            "event" => Ok(MemoryType::Event),
            "relationship" => Ok(MemoryType::Relationship),
            "observation" => Ok(MemoryType::Observation),
            "error" => Ok(MemoryType::Error),
            "context" => Ok(MemoryType::Context),
            "password" | "passwords" => Ok(MemoryType::Password),
            other => Err(format!(
                "Unknown memory type '{}'. Valid types: fact, decision, instruction, preference, learning, goal, commitment, artifact, event, relationship, observation, error, context, password",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Expired,
}

impl MemoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryStatus::Active => "active",
            MemoryStatus::Expired => "expired",
        }
    }
}

impl FromStr for MemoryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "active" => Ok(MemoryStatus::Active),
            "expired" => Ok(MemoryStatus::Expired),
            other => Err(format!(
                "Unknown status '{}'. Must be 'active' or 'expired'",
                other
            )),
        }
    }
}

#[cfg(test)]
mod infer_tests {
    use super::*;
    use crate::repositories::Storage;
    use chrono::Duration;

    #[test]
    fn explicit_category_prefixes_take_precedence_over_substrings() {
        assert_eq!(
            MemoryType::infer("TODO: fix panic in worker"),
            MemoryType::Commitment
        );
        assert_eq!(
            MemoryType::infer("Error: api_key expired"),
            MemoryType::Error
        );
        assert_eq!(
            MemoryType::infer("Artifact: we decided to store schema here"),
            MemoryType::Artifact
        );
        assert_eq!(
            MemoryType::infer("Context: currently working on migrating to Postgres"),
            MemoryType::Context
        );
        assert_eq!(
            MemoryType::infer("Fact: user prefers dark mode in some docs"),
            MemoryType::Fact
        );
        assert_eq!(
            MemoryType::infer("Decision: fix panic by using bounded channels"),
            MemoryType::Decision
        );
        assert_eq!(
            MemoryType::infer("Goal: fix panic rate to zero"),
            MemoryType::Goal
        );
        assert_eq!(
            MemoryType::infer("Learning: api_key rotation requires dual reads"),
            MemoryType::Learning
        );
        assert_eq!(
            MemoryType::infer("Event: panic in production cluster"),
            MemoryType::Event
        );
        assert_eq!(
            MemoryType::infer("Relationship: Alice decided to mentor Bob"),
            MemoryType::Relationship
        );
        assert_eq!(
            MemoryType::infer("Observation: connection timeout during deploy"),
            MemoryType::Observation
        );
        assert_eq!(
            MemoryType::infer("Secret: some random text"),
            MemoryType::Password
        );
        assert_eq!(
            MemoryType::infer("Credential: internal service token"),
            MemoryType::Password
        );
    }

    #[test]
    fn secret_token_prefixes_detected_mid_sentence() {
        assert_eq!(
            MemoryType::infer("GitHub personal token is ghp_1234567890abcdef"),
            MemoryType::Password
        );
        assert_eq!(
            MemoryType::infer("AWS access key is AKIAIOSFODNN7EXAMPLE"),
            MemoryType::Password
        );
        assert_eq!(
            MemoryType::infer("Fine-grained token: github_pat_11aabbccddeeff"),
            MemoryType::Password
        );
        assert_eq!(
            MemoryType::infer("Use sk-proj-1234567890 for staging"),
            MemoryType::Password
        );
        assert_eq!(
            MemoryType::infer("GitLab token is glpat-abcdef123456"),
            MemoryType::Password
        );
        assert_eq!(
            MemoryType::infer("Slack bot token is xoxb-12345-67890"),
            MemoryType::Password
        );
    }

    #[test]
    fn credential_url_heuristic_checks_per_token() {
        assert_eq!(
            MemoryType::infer(
                "Docs for @scope/pkg are at https://registry.npmjs.org/@scope/pkg (port: 443)"
            ),
            MemoryType::Fact
        );
        assert_eq!(
            MemoryType::infer("Database URI is postgres://admin:secret123@localhost:5432/mydb"),
            MemoryType::Password
        );
    }

    #[test]
    fn instruction_preference_and_decision_collisions_resolved() {
        assert_eq!(
            MemoryType::infer("We decided to support dark mode in v2"),
            MemoryType::Decision
        );
        assert_eq!(
            MemoryType::infer("Always prefer parameterized SQL queries to prevent injection"),
            MemoryType::Instruction
        );
        assert_eq!(
            MemoryType::infer("Always prefer dark mode"),
            MemoryType::Preference
        );
        assert_eq!(
            MemoryType::infer("Always prefer concise single-line answers with zero fluff"),
            MemoryType::Preference
        );
    }

    #[test]
    fn as_of_excludes_expired_memories_with_null_expired_at() {
        let storage = Storage::open_in_memory().expect("open_in_memory");
        let now = Utc::now();

        let mut expired_no_ts = MemoryRecord::new(
            MemoryType::Fact,
            "Expired No TS".to_string(),
            "Redis legacy cluster runs on port 6379".to_string(),
            vec![],
            0.9,
            None,
        );
        expired_no_ts.created_at = now - Duration::hours(2);
        expired_no_ts.updated_at = now - Duration::hours(1);
        expired_no_ts.status = MemoryStatus::Expired;
        expired_no_ts.expired_at = None;
        storage
            .insert_memory(&expired_no_ts, Some(&[1.0, 0.0, 0.0, 0.0]))
            .expect("insert expired_no_ts");

        let mut active_mem = MemoryRecord::new(
            MemoryType::Fact,
            "Active Mem".to_string(),
            "Redis primary cluster runs on port 6380".to_string(),
            vec![],
            0.9,
            None,
        );
        active_mem.created_at = now - Duration::hours(2);
        active_mem.updated_at = now - Duration::hours(2);
        active_mem.status = MemoryStatus::Active;
        active_mem.expired_at = None;
        storage
            .insert_memory(&active_mem, Some(&[0.0, 1.0, 0.0, 0.0]))
            .expect("insert active_mem");

        let bm25_hits = storage
            .search_bm25("Redis", None, None, Some(now), 10)
            .expect("search_bm25");
        assert_eq!(bm25_hits.len(), 1);
        assert_eq!(bm25_hits[0].0, active_mem.id);

        let vec_hits = storage
            .get_candidate_vectors_checked(None, None, Some(now), None)
            .expect("get_candidate_vectors_checked");
        assert_eq!(vec_hits.len(), 1);
        assert_eq!(vec_hits[0].0, active_mem.id);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub category: MemoryType,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub status: MemoryStatus,
    pub provenance: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expired_at: Option<DateTime<Utc>>,
}

impl MemoryRecord {
    pub fn new(
        category: MemoryType,
        title: String,
        content: String,
        tags: Vec<String>,
        confidence: f32,
        provenance: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            category,
            title,
            content,
            tags,
            confidence: confidence.clamp(0.0, 1.0),
            status: MemoryStatus::Active,
            provenance: provenance.unwrap_or_else(|| "explicit_statement".to_string()),
            created_at: now,
            updated_at: now,
            expired_at: None,
        }
    }

    /// Format as standardized memory card for embedding and display
    pub fn to_card_text(&self) -> String {
        let mut out = format!(
            "[{}] {}\n\n{}",
            self.category.as_str().to_uppercase(),
            self.title,
            self.content
        );
        if !self.tags.is_empty() {
            out.push_str(&format!("\n\nTags: {}", self.tags.join(", ")));
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub memory: MemoryRecord,
    pub score: f32,
    pub bm25_rank: Option<usize>,
    pub vector_rank: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedMemories {
    pub items: Vec<MemoryRecord>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub page: usize,
    pub total_pages: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConflict {
    pub similarity: f32,
    pub overlap_ratio: f32,
    pub older_memory: MemoryRecord,
    pub newer_memory: MemoryRecord,
}

/// Parse a human-friendly TTL duration string (e.g. "30s", "15m", "24h", "7d", "2w")
pub fn parse_ttl_duration(raw: &str) -> Result<chrono::Duration, String> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() {
        return Err("TTL duration cannot be empty (examples: 30s, 15m, 24h, 7d)".to_string());
    }
    let (num_str, unit) = if let Some(n) = s.strip_suffix("ms") {
        (n.trim(), "ms")
    } else if let Some(n) = s.strip_suffix('s') {
        (n.trim(), "s")
    } else if let Some(n) = s.strip_suffix('m') {
        (n.trim(), "m")
    } else if let Some(n) = s.strip_suffix('h') {
        (n.trim(), "h")
    } else if let Some(n) = s.strip_suffix('d') {
        (n.trim(), "d")
    } else if let Some(n) = s.strip_suffix('w') {
        (n.trim(), "w")
    } else {
        (s.as_str(), "s")
    };

    let val: i64 = num_str.parse().map_err(|_| {
        format!(
            "Invalid TTL '{}'. Expected positive integer with unit s, m, h, d, or w (e.g. 24h, 7d)",
            raw
        )
    })?;
    if val <= 0 {
        return Err("TTL duration must be greater than 0".to_string());
    }

    match unit {
        "ms" => Ok(chrono::Duration::milliseconds(val)),
        "s" => Ok(chrono::Duration::seconds(val)),
        "m" => Ok(chrono::Duration::minutes(val)),
        "h" => Ok(chrono::Duration::hours(val)),
        "d" => Ok(chrono::Duration::days(val)),
        "w" => Ok(chrono::Duration::weeks(val)),
        _ => Err(format!("Unsupported TTL unit '{}'", unit)),
    }
}

pub fn extract_meaningful_tokens(
    content: &str,
) -> (
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
) {
    use std::collections::HashSet;

    const STOP_WORDS: &[&str] = &[
        "the", "a", "an", "on", "in", "at", "to", "for", "of", "with", "by", "is", "are", "was",
        "were", "be", "been", "has", "have", "had", "and", "or", "as", "from", "that", "this",
    ];

    let all_tokens: HashSet<String> = content
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.')
        .map(|w| {
            w.trim_matches(|c: char| c == '.' || c == '-' || c == '_')
                .to_lowercase()
        })
        .filter(|w| w.len() > 1 && !STOP_WORDS.contains(&w.as_str()))
        .collect();

    let word_tokens: HashSet<String> = all_tokens
        .iter()
        .filter(|w| w.chars().any(|c| c.is_alphabetic()))
        .cloned()
        .collect();

    (word_tokens, all_tokens)
}

/// Evaluate whether two memories conflict or overlap based on vector cosine similarity and token overlap.
/// No hardcoded slot regexes — works for any near-duplicate or updated fact.
pub fn detect_memory_conflict_with_similarity(
    a: &MemoryRecord,
    b: &MemoryRecord,
    vec_similarity: Option<f32>,
    min_similarity: f32,
) -> Option<MemoryConflict> {
    if a.id == b.id {
        return None;
    }

    let (words_a, all_a) = extract_meaningful_tokens(&a.content);
    let (words_b, all_b) = extract_meaningful_tokens(&b.content);

    if words_a.is_empty() || words_b.is_empty() {
        return None;
    }

    let word_inter = words_a.intersection(&words_b).count();
    if word_inter == 0 {
        return None;
    }

    let word_min = words_a.len().min(words_b.len()).max(1);
    let word_overlap = word_inter as f32 / word_min as f32;

    let all_inter = all_a.intersection(&all_b).count();
    let all_min = all_a.len().min(all_b.len()).max(1);
    let overlap_ratio = all_inter as f32 / all_min as f32;

    // Combine semantic vector cosine similarity with lexical word overlap
    let raw_vec_sim = vec_similarity.unwrap_or(0.0);
    let effective_sim = if a.content.trim().eq_ignore_ascii_case(b.content.trim()) {
        1.0
    } else {
        raw_vec_sim.max(word_overlap * 0.92)
    };

    if effective_sim < min_similarity || word_overlap < 0.50 {
        return None;
    }

    let (older, newer) = if (a.created_at, &a.id) <= (b.created_at, &b.id) {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    };

    Some(MemoryConflict {
        similarity: effective_sim.clamp(0.0, 1.0),
        overlap_ratio: overlap_ratio.clamp(0.0, 1.0),
        older_memory: older,
        newer_memory: newer,
    })
}

/// Convenience wrapper when comparing two memories directly (uses lexical + optional vector similarity)
pub fn detect_memory_conflict(a: &MemoryRecord, b: &MemoryRecord) -> Option<MemoryConflict> {
    detect_memory_conflict_with_similarity(a, b, None, 0.72)
}

#[cfg(test)]
mod conflict_tests {
    use super::*;

    fn make_fact(content: &str) -> MemoryRecord {
        MemoryRecord::new(
            MemoryType::Fact,
            content.to_string(),
            content.to_string(),
            vec![],
            1.0,
            None,
        )
    }

    #[test]
    fn test_detects_overlapping_and_updated_memories_without_hardcoded_slots() {
        let m1 = make_fact("PostgreSQL primary database runs on port 5432");
        let m2 = make_fact("PostgreSQL primary database runs on port 5433");
        let conflict = detect_memory_conflict(&m1, &m2).expect("Should detect overlapping update");
        assert!(conflict.similarity >= 0.75);
        assert!(conflict.overlap_ratio >= 0.60);

        let d1 = make_fact("The vault stores sensitive personal documents including Aadhaar cards, PAN cards, educational certificates");
        let d2 = make_fact("The vault stores various personal documents including Aadhaar cards, PAN cards, educational certificates");
        let dup_conflict =
            detect_memory_conflict(&d1, &d2).expect("Should detect near-duplicate memories");
        assert!(dup_conflict.similarity >= 0.80);

        let unrelated1 = make_fact("PostgreSQL primary database runs on port 5432");
        let unrelated2 = make_fact("Mistri Venkata Annapurna Devi is my mother name");
        assert!(detect_memory_conflict(&unrelated1, &unrelated2).is_none());
    }
}
