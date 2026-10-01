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
        }
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
            other => Err(format!(
                "Unknown memory type '{}'. Valid types: fact, decision, instruction, preference, learning, goal, commitment, artifact, event, relationship, observation, error, context",
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
            other => Err(format!("Unknown status '{}'. Must be 'active' or 'expired'", other)),
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
        let mut out = format!("[{}] {}\n\n{}", self.category.as_str().to_uppercase(), self.title, self.content);
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
