use crate::models::{MemoryRecord, MemoryType};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCandidate {
    pub id: Option<String>,
    pub category: Option<MemoryType>,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub provenance: String,
    pub created_at: Option<DateTime<Utc>>,
}

impl ImportCandidate {
    pub fn to_memory_record(self) -> MemoryRecord {
        let cat = self.category.unwrap_or(MemoryType::Fact);
        let mut record = MemoryRecord::new(
            cat,
            self.title,
            self.content,
            self.tags,
            self.confidence,
            Some(self.provenance),
        );
        if let Some(id) = self.id {
            record.id = id;
        }
        if let Some(ts) = self.created_at {
            record.created_at = ts;
            record.updated_at = ts;
        }
        record
    }
}

pub trait MemoryImporter: Send + Sync {
    fn name(&self) -> &str;
    fn parse(&self, raw: &str) -> Result<Vec<ImportCandidate>>;
}

/// Helper to extract string values from root or nested metadata
fn get_str_field(val: &serde_json::Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(v) = val.get(*key) {
            if let Some(s) = v.as_str() {
                if !s.trim().is_empty() {
                    return Some(s.trim().to_string());
                }
            } else if v.is_number() || v.is_boolean() {
                return Some(v.to_string());
            }
        }
    }
    for meta_key in &["metadata", "payload", "meta", "attributes", "info"] {
        if let Some(nested) = val.get(*meta_key).and_then(|v| v.as_object()) {
            for key in keys {
                if let Some(v) = nested.get(*key) {
                    if let Some(s) = v.as_str() {
                        if !s.trim().is_empty() {
                            return Some(s.trim().to_string());
                        }
                    } else if v.is_number() || v.is_boolean() {
                        return Some(v.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Helper to parse categories leniently across all 14 categories
fn parse_category_lenient(cat_opt: Option<&str>, content: &str) -> MemoryType {
    if let Some(c) = cat_opt {
        let trimmed = c.trim().to_lowercase();
        if let Ok(matched) = trimmed.parse::<MemoryType>() {
            return matched;
        }
        if trimmed.contains("pass") || trimmed.contains("secret") || trimmed.contains("token") {
            return MemoryType::Password;
        }
        if trimmed.contains("decis") {
            return MemoryType::Decision;
        }
        if trimmed.contains("pref") {
            return MemoryType::Preference;
        }
        if trimmed.contains("learn") {
            return MemoryType::Learning;
        }
        if trimmed.contains("goal") {
            return MemoryType::Goal;
        }
        if trimmed.contains("commit") {
            return MemoryType::Commitment;
        }
        if trimmed.contains("artif") {
            return MemoryType::Artifact;
        }
        if trimmed.contains("event") {
            return MemoryType::Event;
        }
        if trimmed.contains("relat") {
            return MemoryType::Relationship;
        }
        if trimmed.contains("obser") {
            return MemoryType::Observation;
        }
        if trimmed.contains("error") {
            return MemoryType::Error;
        }
        if trimmed.contains("context") {
            return MemoryType::Context;
        }
    }

    // Inspect content keywords if category wasn't explicit
    let lower_content = content.to_lowercase();
    if lower_content.starts_with("ghp_")
        || lower_content.starts_with("sk-")
        || lower_content.starts_with("glpat-")
        || lower_content.contains("password:")
        || lower_content.contains("token:")
        || lower_content.contains("api_key:")
    {
        return MemoryType::Password;
    }
    if lower_content.contains("we decided") || lower_content.contains("decision:") {
        return MemoryType::Decision;
    }
    if lower_content.contains("user prefers") || lower_content.contains("preference:") {
        return MemoryType::Preference;
    }

    MemoryType::Fact
}

/// Parse a single JSON value into an ImportCandidate using the Fallback Ladder
pub fn parse_single_json_value(val: &serde_json::Value) -> Option<ImportCandidate> {
    let content = get_str_field(
        val,
        &[
            "content",
            "memory",
            "text",
            "value",
            "document",
            "body",
            "message",
            "data",
            "statement",
        ],
    )?;

    let title = get_str_field(val, &["title", "name", "summary", "heading", "subject"])
        .unwrap_or_else(|| {
            content
                .lines()
                .next()
                .unwrap_or("Imported Memory")
                .chars()
                .take(80)
                .collect()
        });

    let cat_str = get_str_field(
        val,
        &["category", "memory_type", "type", "kind", "type_name"],
    );
    let category = Some(parse_category_lenient(cat_str.as_deref(), &content));

    let mut tags = Vec::new();
    // Check tags at root or metadata
    for tag_key in &["tags", "labels", "keywords", "categories"] {
        if let Some(tag_val) = val.get(*tag_key) {
            if let Some(arr) = tag_val.as_array() {
                for item in arr {
                    if let Some(s) = item.as_str() {
                        tags.push(s.to_string());
                    }
                }
            } else if let Some(s) = tag_val.as_str() {
                tags.extend(
                    s.split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty()),
                );
            }
        }
    }
    if tags.is_empty() {
        if let Some(meta) = val.get("metadata").and_then(|m| m.as_object()) {
            if let Some(tag_val) = meta.get("tags") {
                if let Some(arr) = tag_val.as_array() {
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            tags.push(s.to_string());
                        }
                    }
                } else if let Some(s) = tag_val.as_str() {
                    tags.extend(
                        s.split(',')
                            .map(|t| t.trim().to_string())
                            .filter(|t| !t.is_empty()),
                    );
                }
            }
        }
    }

    let confidence = get_str_field(val, &["confidence", "score", "weight"])
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.9);

    let id = get_str_field(val, &["id", "memory_id", "uuid", "_id"]);
    let provenance =
        get_str_field(val, &["provenance", "source"]).unwrap_or_else(|| "imported".to_string());

    let created_at = get_str_field(val, &["created_at", "timestamp", "date", "created"])
        .and_then(|ts| {
            DateTime::parse_from_rfc3339(&ts)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        });

    Some(ImportCandidate {
        id,
        category,
        title,
        content,
        tags,
        confidence,
        provenance,
        created_at,
    })
}

/// Extract all JSON values from raw text (handles arrays, wrapped objects, JSONL, and single items)
pub fn extract_raw_json_values(raw: &str) -> Vec<serde_json::Value> {
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(arr) = parsed.as_array() {
            return arr.clone();
        }
        if let Some(obj) = parsed.as_object() {
            for wrapper in &[
                "memories",
                "data",
                "items",
                "results",
                "documents",
                "records",
                "messages",
            ] {
                if let Some(arr) = obj.get(*wrapper).and_then(|v| v.as_array()) {
                    return arr.clone();
                }
            }
            return vec![parsed];
        }
    }

    // Try parsing line-by-line as JSONL
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                out.push(v);
            }
        }
    }
    out
}

/// Needle 3 Action SLM Entity Extractor for intelligent JSON ingestion
pub struct NeedleEntityExtractor;

impl NeedleEntityExtractor {
    pub fn extract(items: &[serde_json::Value]) -> Result<Vec<ImportCandidate>> {
        let reranker = crate::reranker::NeedleReranker::new();
        let python = reranker.resolve_python();
        let script = reranker.resolve_script();

        let payload = serde_json::json!({
            "action": "extract",
            "objects": items,
        });

        let mut child = Command::new(&python)
            .arg(&script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn Needle extractor via {}", python))?;

        if let Some(mut stdin) = child.stdin.take() {
            let json_bytes = serde_json::to_vec(&payload)?;
            stdin.write_all(&json_bytes)?;
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            anyhow::bail!("Needle extraction script failed with status {}", output.status);
        }

        let out_json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        let mut candidates = Vec::new();

        if let Some(results) = out_json.get("results").and_then(|v| v.as_array()) {
            for (idx, res) in results.iter().enumerate() {
                if let Some(c) = parse_single_json_value(res) {
                    candidates.push(c);
                } else if idx < items.len() {
                    if let Some(fallback) = parse_single_json_value(&items[idx]) {
                        candidates.push(fallback);
                    }
                }
            }
        }

        Ok(candidates)
    }
}

/// Ingests JSON exports (Memanto, Mem0, Letta, LangChain, raw logs, JSONL, or wrapped objects)
pub struct JsonMemoryImporter;

impl JsonMemoryImporter {
    pub fn parse_flexible(&self, raw: &str, use_needle: bool) -> Result<Vec<ImportCandidate>> {
        let raw_values = extract_raw_json_values(raw);
        if raw_values.is_empty() {
            return Ok(Vec::new());
        }

        // If Needle extraction is requested, invoke Needle 3
        if use_needle {
            if let Ok(needle_candidates) = NeedleEntityExtractor::extract(&raw_values) {
                if !needle_candidates.is_empty() {
                    return Ok(needle_candidates);
                }
            }
        }

        // Universal Fallback Ladder
        let mut out = Vec::with_capacity(raw_values.len());
        for val in &raw_values {
            if let Some(candidate) = parse_single_json_value(val) {
                out.push(candidate);
            }
        }
        Ok(out)
    }
}

impl MemoryImporter for JsonMemoryImporter {
    fn name(&self) -> &str {
        "json-importer"
    }

    fn parse(&self, raw: &str) -> Result<Vec<ImportCandidate>> {
        self.parse_flexible(raw, false)
    }
}

/// Ingests OKF (Open Knowledge Format) markdown bundles
pub struct OkfMemoryImporter;

impl MemoryImporter for OkfMemoryImporter {
    fn name(&self) -> &str {
        "okf-importer"
    }

    fn parse(&self, raw: &str) -> Result<Vec<ImportCandidate>> {
        let mut candidates = Vec::new();
        let mut current_category = MemoryType::Fact;

        let sections = raw.split("---");
        for section in sections {
            let trimmed = section.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Check if section defines a category header e.g. ## DECISION
            for line in trimmed.lines() {
                if line.starts_with("## ") {
                    let cat_str = line.trim_start_matches("## ").trim().to_lowercase();
                    if let Ok(cat) = cat_str.parse::<MemoryType>() {
                        current_category = cat;
                    }
                }
            }

            // Parse ### Title blocks
            if let Some(idx) = trimmed.find("### ") {
                let block = &trimmed[idx..];
                let mut lines = block.lines();
                let title_line = lines.next().unwrap_or("").trim_start_matches("### ").trim();
                if title_line.is_empty() {
                    continue;
                }

                let mut id = None;
                let mut tags = Vec::new();
                let mut confidence = 0.9f32;
                let mut created_at = None;
                let mut content_lines = Vec::new();

                for line in lines {
                    if line.starts_with("- **ID:**") {
                        id = Some(
                            line.trim_start_matches("- **ID:**")
                                .replace('`', "")
                                .trim()
                                .to_string(),
                        );
                    } else if line.starts_with("- **Tags:**") {
                        let tag_part = line.trim_start_matches("- **Tags:**").replace('`', "");
                        tags = tag_part
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    } else if line.starts_with("- **Confidence:**") {
                        let conf_str = line
                            .trim_start_matches("- **Confidence:**")
                            .replace('`', "")
                            .trim()
                            .to_string();
                        if let Ok(c) = conf_str.parse::<f32>() {
                            confidence = c;
                        }
                    } else if line.starts_with("- **Created:**") {
                        let ts_str = line
                            .trim_start_matches("- **Created:**")
                            .replace('`', "")
                            .trim()
                            .to_string();
                        if let Ok(dt) = DateTime::parse_from_rfc3339(&ts_str) {
                            created_at = Some(dt.with_timezone(&Utc));
                        }
                    } else if !line.starts_with("- **") {
                        content_lines.push(line);
                    }
                }

                let content = content_lines.join("\n").trim().to_string();
                if !content.is_empty() {
                    candidates.push(ImportCandidate {
                        id,
                        category: Some(current_category),
                        title: title_line.to_string(),
                        content,
                        tags,
                        confidence,
                        provenance: "imported:okf".to_string(),
                        created_at,
                    });
                }
            }
        }

        Ok(candidates)
    }
}
