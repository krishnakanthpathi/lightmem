use crate::models::{MemoryRecord, MemoryType};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

/// Ingests JSON exports (Memanto, Mem0, Letta, or standard JSON arrays)
pub struct JsonMemoryImporter;

#[derive(Deserialize)]
struct RawJsonItem {
    id: Option<String>,
    content: Option<String>,
    text: Option<String>,
    title: Option<String>,
    #[serde(alias = "memory_type")]
    category: Option<String>,
    #[serde(alias = "type")]
    type_name: Option<String>,
    tags: Option<serde_json::Value>,
    confidence: Option<f64>,
    provenance: Option<String>,
    created_at: Option<String>,
}

impl MemoryImporter for JsonMemoryImporter {
    fn name(&self) -> &str {
        "json-importer"
    }

    fn parse(&self, raw: &str) -> Result<Vec<ImportCandidate>> {
        let items: Vec<RawJsonItem> =
            serde_json::from_str(raw).with_context(|| "Failed to parse JSON memory array")?;

        let mut out = Vec::new();
        for item in items {
            let content = item
                .content
                .or(item.text)
                .unwrap_or_default()
                .trim()
                .to_string();
            if content.is_empty() {
                continue;
            }

            let title = item.title.unwrap_or_else(|| {
                content
                    .lines()
                    .next()
                    .unwrap_or("Imported Memory")
                    .chars()
                    .take(80)
                    .collect()
            });

            let cat_str = item.category.or(item.type_name);
            let category = cat_str.and_then(|s| s.parse::<MemoryType>().ok());

            let mut tags = Vec::new();
            if let Some(val) = item.tags {
                if let Some(arr) = val.as_array() {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            tags.push(s.to_string());
                        }
                    }
                } else if let Some(s) = val.as_str() {
                    tags = s
                        .split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect();
                }
            }

            let confidence = item.confidence.unwrap_or(0.8) as f32;
            let provenance = item.provenance.unwrap_or_else(|| "imported".to_string());

            let created_at = item.created_at.as_deref().and_then(|ts| {
                DateTime::parse_from_rfc3339(ts)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            });

            out.push(ImportCandidate {
                id: item.id,
                category,
                title,
                content,
                tags,
                confidence,
                provenance,
                created_at,
            });
        }

        Ok(out)
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
