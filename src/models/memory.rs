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

        // 1. Password / Secret / Credential
        if lower.starts_with("ghp_")
            || lower.starts_with("github_pat_")
            || lower.starts_with("sk-")
            || lower.contains("sk_live_")
            || lower.contains("sk_test_")
            || lower.starts_with("glpat-")
            || lower.starts_with("xoxb-")
            || lower.starts_with("akia")
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
            || (lower.contains("://") && lower.contains('@') && lower.matches(':').count() >= 2)
        {
            return MemoryType::Password;
        }

        // 2. Error / Bug / Failure
        if lower.starts_with("error:")
            || lower.starts_with("bug:")
            || lower.contains("panic:")
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

        // 3. Preference / Style (checked before Instruction so "Always prefer..." maps to Preference)
        if lower.contains("prefers ")
            || lower.contains("prefer ")
            || lower.contains("user prefers")
            || lower.starts_with("preference:")
            || lower.contains("likes to use ")
            || lower.contains("favorite ")
            || lower.contains("dark mode")
            || lower.contains("keybindings")
        {
            return MemoryType::Preference;
        }

        // 4. Instruction / Rule / Runbook
        if lower.starts_with("always ")
            || lower.starts_with("never ")
            || lower.starts_with("do not ")
            || lower.starts_with("don't ")
            || lower.starts_with("how to ")
            || lower.starts_with("rule:")
            || lower.starts_with("instruction:")
            || lower.starts_with("runbook:")
            || lower.contains("must always ")
            || lower.contains("must never ")
            || lower.contains("make sure to ")
            || lower.contains("step 1")
        {
            return MemoryType::Instruction;
        }

        // 5. Decision / Architecture Choice
        if lower.contains("we decided")
            || lower.contains("decided to ")
            || lower.starts_with("decision:")
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
    pub slot: String,
    pub old_value: String,
    pub new_value: String,
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

fn extract_subject_and_slots(
    content: &str,
) -> (
    std::collections::HashSet<String>,
    std::collections::BTreeMap<String, String>,
) {
    use regex::Regex;
    use std::collections::{BTreeMap, HashSet};

    let mut slots = BTreeMap::new();
    let mut slot_tokens = HashSet::new();
    let mut prose_content = content.to_string();

    if let Ok(re_port) = Regex::new(r"(?i)\bport\s*[:=]?\s*(\d{1,5})\b") {
        let ports: Vec<String> = re_port
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        if ports.len() == 1 {
            slot_tokens.insert(ports[0].to_lowercase());
            slots.insert("port".to_string(), ports[0].clone());
        }
    }

    if let Ok(re_url) = Regex::new(r"(https?://[^\s,]+)") {
        let urls: Vec<String> = re_url
            .captures_iter(content)
            .filter_map(|c| {
                c.get(1)
                    .map(|m| m.as_str().trim_end_matches('.').to_string())
            })
            .collect();
        if urls.len() == 1 {
            prose_content = prose_content.replace(&urls[0], " ");
            slots.insert("url".to_string(), urls[0].clone());
        }
    }

    if let Ok(re_os) = Regex::new(
        r"(?i)\b(linux|macos|windows|ubuntu|debian|alpine|freebsd|fedora|arch|rhel|centos|rocky|alma|suse|opensuse|nixos|gentoo|solaris|openbsd|netbsd|dragonfly|ios|android)\b",
    ) {
        let mut os_list: Vec<String> = re_os
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_lowercase()))
            .collect();
        os_list.sort();
        os_list.dedup();
        if os_list.len() == 1 {
            slot_tokens.insert(os_list[0].clone());
            slots.insert("os".to_string(), os_list[0].clone());
        }
    }

    if slots.is_empty() {
        if let Ok(re_kv) =
            Regex::new(r"(?i)^\s*([a-z0-9_.\-/\s]{2,40}?)\s+(?:is|=)\s+([^\s.,;]+)")
        {
            if let Some(cap) = re_kv.captures(content.trim()) {
                if let (Some(lhs), Some(rhs)) = (cap.get(1), cap.get(2)) {
                    let val = rhs.as_str().trim().to_string();
                    let key_name = lhs.as_str().trim().to_lowercase();
                    slot_tokens.insert(val.to_lowercase());
                    slots.insert(format!("value({})", key_name), val);
                }
            }
        }
    }

    let stop_words: HashSet<&str> = [
        "the",
        "a",
        "an",
        "on",
        "in",
        "at",
        "to",
        "for",
        "of",
        "with",
        "by",
        "is",
        "are",
        "was",
        "were",
        "run",
        "runs",
        "running",
        "use",
        "uses",
        "used",
        "port",
        "url",
        "endpoint",
        "uri",
        "os",
        "operating",
        "system",
        "node",
        "host",
        "server",
        "service",
        "cluster",
        "engine",
        "app",
        "application",
    ]
    .into_iter()
    .collect();

    let mut subjects: HashSet<String> = prose_content
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 1 && !stop_words.contains(w.as_str()) && !slot_tokens.contains(w))
        .collect();

    if subjects.is_empty() {
        let infra_nouns: HashSet<&str> = [
            "server",
            "node",
            "host",
            "service",
            "cluster",
            "engine",
            "app",
            "application",
        ]
        .into_iter()
        .collect();
        subjects = prose_content
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .map(|w| w.to_lowercase())
            .filter(|w| infra_nouns.contains(w.as_str()) && !slot_tokens.contains(w))
            .collect();
    }

    (subjects, slots)
}

/// Check if two active memories represent a factual contradiction on the same subject entity
pub fn detect_memory_conflict(a: &MemoryRecord, b: &MemoryRecord) -> Option<MemoryConflict> {
    if a.id == b.id || a.category != b.category || a.content.trim() == b.content.trim() {
        return None;
    }

    let (subj_a, slots_a) = extract_subject_and_slots(&a.content);
    let (subj_b, slots_b) = extract_subject_and_slots(&b.content);

    if subj_a.is_empty() || subj_b.is_empty() || slots_a.is_empty() || slots_b.is_empty() {
        return None;
    }

    let overlap = subj_a.intersection(&subj_b).count();
    let min_len = subj_a.len().min(subj_b.len());
    if overlap == 0 || overlap * 2 < min_len {
        return None;
    }

    let (older, newer, older_slots, newer_slots) = if (a.created_at, &a.id) <= (b.created_at, &b.id)
    {
        (a, b, &slots_a, &slots_b)
    } else {
        (b, a, &slots_b, &slots_a)
    };

    for (slot, old_val) in older_slots {
        if let Some(new_val) = newer_slots.get(slot) {
            if !old_val.eq_ignore_ascii_case(new_val) {
                return Some(MemoryConflict {
                    slot: slot.clone(),
                    old_value: old_val.clone(),
                    new_value: new_val.clone(),
                    older_memory: older.clone(),
                    newer_memory: newer.clone(),
                });
            }
        }
    }

    None
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
    fn test_url_subject_cannibalization() {
        let m1 = make_fact("Atlas API endpoint is https://api.v1.atlas.io");
        let m2 = make_fact("Atlas API endpoint is https://api.v2.atlas.io");
        let conflict = detect_memory_conflict(&m1, &m2).expect("Should detect URL conflict");
        assert_eq!(conflict.slot, "url");
    }

    #[test]
    fn test_expanded_os_regex() {
        let m1 = make_fact("Worker node runs on Fedora");
        let m2 = make_fact("Worker node runs on Arch");
        let conflict = detect_memory_conflict(&m1, &m2).expect("Should detect Fedora vs Arch OS conflict");
        assert_eq!(conflict.slot, "os");

        let m3 = make_fact("Worker node OS is Ubuntu");
        let m4 = make_fact("Worker node OS is Fedora");
        let conflict2 = detect_memory_conflict(&m3, &m4).expect("Should detect Ubuntu vs Fedora OS conflict");
        assert_eq!(conflict2.slot, "os");
    }

    #[test]
    fn test_dotted_and_slashed_keys_in_re_kv() {
        let m1 = make_fact("db.max_connections = 100");
        let m2 = make_fact("db.max_connections = 500");
        let conflict = detect_memory_conflict(&m1, &m2).expect("Should detect dotted key KV conflict");
        assert_eq!(conflict.slot, "value(db.max_connections)");

        let m3 = make_fact("service/timeout_ms = 250");
        let m4 = make_fact("service/timeout_ms = 1000");
        let conflict2 = detect_memory_conflict(&m3, &m4).expect("Should detect slashed key KV conflict");
        assert_eq!(conflict2.slot, "value(service/timeout_ms)");
    }

    #[test]
    fn test_stopword_only_subject_fallback() {
        let m1 = make_fact("Server node runs on Ubuntu");
        let m2 = make_fact("Server node runs on Alpine");
        let conflict = detect_memory_conflict(&m1, &m2).expect("Should detect conflict using infrastructure noun fallback");
        assert_eq!(conflict.slot, "os");
    }
}
