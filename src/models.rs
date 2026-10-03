use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
            || lower.contains("fix panic")
            || lower.contains("panicked at")
            || lower.contains("segfault")
            || lower.contains("stack trace")
            || lower.contains("failed with")
            || lower.contains("connection refused")
            || lower.contains("connection timeout")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

