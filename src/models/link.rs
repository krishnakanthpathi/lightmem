use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryLink {
    pub source_id: String,
    pub target_id: String,
    pub relation: String,
    pub weight: f32,
    pub created_at: DateTime<Utc>,
}

impl MemoryLink {
    pub fn new(
        source_id: impl Into<String>,
        target_id: impl Into<String>,
        relation: Option<String>,
        weight: Option<f32>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            target_id: target_id.into(),
            relation: relation.unwrap_or_else(|| "relates_to".to_string()),
            weight: weight.unwrap_or(1.0),
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedMemory {
    pub memory: super::MemoryRecord,
    pub distance: usize,
    pub relation_path: Vec<String>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub category: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub degree: usize,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Extracts wikilink targets from markdown text: `[[target]]` or `[[target|display text]]`
pub fn extract_wikilinks(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("]]") {
            let mut inner = &after_start[..end];
            if let Some(last_open) = inner.rfind("[[") {
                inner = &inner[last_open + 2..];
            }
            // If piped [[target|label]], extract target
            let target = match inner.split_once('|') {
                Some((tgt, _)) => tgt.trim(),
                None => inner.trim(),
            };
            if !target.is_empty() {
                targets.push(target.to_string());
            }
            rest = &after_start[end + 2..];
        } else {
            break;
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_wikilinks() {
        let text = "See [[server-config]] and [[db-primary|Main Database]] for details. Also [[nested [[broken]] test.";
        let links = extract_wikilinks(text);
        assert_eq!(links, vec!["server-config", "db-primary", "broken"]);

        let empty = extract_wikilinks("No links here");
        assert!(empty.is_empty());

        let spaces = extract_wikilinks("Link with spaces: [[ User Preferences ]]");
        assert_eq!(spaces, vec!["User Preferences"]);
    }
}
