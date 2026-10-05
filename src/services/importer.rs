use crate::models::{MemoryRecord, MemoryStatus, MemoryType};
use anyhow::Result;
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
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expired_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub status: Option<MemoryStatus>,
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
        record.updated_at = self.updated_at.unwrap_or(record.updated_at);
        record.expired_at = self.expired_at;
        record.status = self.status.unwrap_or(MemoryStatus::Active);
        record
    }
}

impl From<MemoryRecord> for ImportCandidate {
    fn from(m: MemoryRecord) -> Self {
        Self {
            id: Some(m.id),
            category: Some(m.category),
            title: m.title,
            content: m.content,
            tags: m.tags,
            confidence: m.confidence,
            provenance: m.provenance,
            created_at: Some(m.created_at),
            updated_at: Some(m.updated_at),
            expired_at: m.expired_at,
            status: Some(m.status),
        }
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

fn try_parse_category_lenient(c: &str) -> Option<MemoryType> {
    let trimmed = c.trim().to_lowercase();
    if trimmed.is_empty() || matches!(trimmed.as_str(), "entity" | "node" | "item" | "record") {
        return None;
    }
    if let Ok(matched) = trimmed.parse::<MemoryType>() {
        return Some(matched);
    }
    if trimmed.contains("pass")
        || trimmed.contains("secret")
        || trimmed.contains("token")
        || trimmed.contains("cred")
        || trimmed == "key"
        || trimmed == "keys"
        || trimmed.contains("api_key")
        || trimmed.contains("apikey")
        || trimmed.contains("secret_key")
        || trimmed.contains("private_key")
        || trimmed.contains("passkey")
    {
        return Some(MemoryType::Password);
    }
    if trimmed.contains("instruct")
        || trimmed.contains("rule")
        || trimmed.contains("runbook")
        || trimmed.contains("procedure")
        || trimmed == "process"
    {
        return Some(MemoryType::Instruction);
    }
    if trimmed.contains("decis") || trimmed.contains("choice") {
        return Some(MemoryType::Decision);
    }
    if trimmed.contains("pref") || trimmed.contains("like") {
        return Some(MemoryType::Preference);
    }
    if trimmed.contains("learn") || trimmed.contains("lesson") || trimmed.contains("insight") {
        return Some(MemoryType::Learning);
    }
    if trimmed.contains("goal") || trimmed.contains("target") || trimmed.contains("objective") {
        return Some(MemoryType::Goal);
    }
    if trimmed.contains("commit") || trimmed.contains("todo") || trimmed.contains("task") {
        return Some(MemoryType::Commitment);
    }
    if trimmed.contains("artif")
        || trimmed.contains("code")
        || trimmed == "doc"
        || trimmed == "docs"
        || trimmed.contains("document")
    {
        return Some(MemoryType::Artifact);
    }
    if trimmed.contains("event") || trimmed.contains("incident") || trimmed.contains("meeting") {
        return Some(MemoryType::Event);
    }
    if trimmed.contains("relat") || trimmed.contains("team") || trimmed.contains("owner") {
        return Some(MemoryType::Relationship);
    }
    if trimmed.contains("obser") || trimmed.contains("metric") {
        return Some(MemoryType::Observation);
    }
    if trimmed.contains("error") || trimmed.contains("bug") || trimmed.contains("issue") {
        return Some(MemoryType::Error);
    }
    if trimmed.contains("context") || trimmed.contains("background") {
        return Some(MemoryType::Context);
    }
    None
}

/// Helper to parse categories leniently across all 14 categories
fn parse_category_lenient(cat_opt: Option<&str>, content: &str) -> MemoryType {
    cat_opt
        .and_then(try_parse_category_lenient)
        .unwrap_or_else(|| MemoryType::infer(content))
}

/// Parse a single JSON value into an ImportCandidate using Pure-Rust Heuristic
pub fn parse_single_json_value(val: &serde_json::Value) -> Option<ImportCandidate> {
    parse_json_candidate(val, false)
}

fn parse_json_candidate(val: &serde_json::Value, _enrich: bool) -> Option<ImportCandidate> {
    let mut heuristic_tags = Vec::new();
    let mut mcp_relation_title: Option<String> = None;
    let mut mcp_relation_default_cat: Option<MemoryType> = None;

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
    )
    .or_else(|| {
        let arr = val.get("observations")?.as_array()?;
        let items: Vec<&str> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::trim))
            .filter(|s| !s.is_empty())
            .collect();
        if items.is_empty() {
            return None;
        }
        if let Some(et) = get_str_field(val, &["entityType", "entity_type"]) {
            let tag = et.to_lowercase();
            if !heuristic_tags.contains(&tag) {
                heuristic_tags.push(tag);
            }
        }
        Some(items.join("; "))
    })
    .or_else(|| {
        let from = get_str_field(val, &["from"])?;
        let to = get_str_field(val, &["to"])?;
        let relation_type = get_str_field(val, &["relationType", "relation_type"])?;
        mcp_relation_title = Some(format!("{} -> {}", from, to));
        mcp_relation_default_cat = Some(MemoryType::Relationship);
        Some(format!("{} {} {}", from, relation_type, to))
    })
    .or_else(|| {
        // Extract the primary descriptive string field verbatim as content, and collect short string fields as tags.
        let obj = val.as_object()?;
        let reserved = [
            "id",
            "memory_id",
            "uuid",
            "_id",
            "title",
            "name",
            "summary",
            "heading",
            "subject",
            "category",
            "memory_type",
            "type",
            "kind",
            "type_name",
            "entityType",
            "entity_type",
            "relationType",
            "relation_type",
            "provenance",
            "source",
            "created_at",
            "timestamp",
            "date",
            "created",
        ];
        let mut best_content: Option<String> = None;
        for (k, v) in obj {
            if reserved.contains(&k.as_str()) {
                continue;
            }
            if let Some(s) = v.as_str() {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if best_content
                    .as_ref()
                    .map(|b| trimmed.len() > b.len())
                    .unwrap_or(true)
                {
                    if let Some(prev) = best_content.replace(trimmed.to_string()) {
                        if prev.len() <= 32 {
                            heuristic_tags.push(prev.to_lowercase());
                        }
                    }
                } else if trimmed.len() <= 32 {
                    heuristic_tags.push(trimmed.to_lowercase());
                }
            }
        }
        best_content
    })?;

    let explicit_title = get_str_field(val, &["title", "name", "summary", "heading", "subject"])
        .or(mcp_relation_title);
    let cat_str = get_str_field(
        val,
        &["category", "memory_type", "type", "kind", "type_name"],
    );
    let explicit_cat = cat_str.as_deref().and_then(try_parse_category_lenient);

    let inferred_rule_cat = MemoryType::infer(&content);
    let category = if let Some(explicit_c) = explicit_cat {
        Some(explicit_c)
    } else if let Some(rel_cat) = mcp_relation_default_cat {
        Some(rel_cat)
    } else if inferred_rule_cat != MemoryType::Fact {
        Some(inferred_rule_cat)
    } else {
        Some(MemoryType::Fact)
    };

    let title = explicit_title.unwrap_or_else(|| {
        content
            .lines()
            .next()
            .unwrap_or("Imported Memory")
            .chars()
            .take(80)
            .collect()
    });

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
    if tags.is_empty() && !heuristic_tags.is_empty() {
        tags = heuristic_tags;
    }

    let confidence = get_str_field(val, &["confidence", "score", "weight"])
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.9);

    let id = get_str_field(val, &["id", "memory_id", "uuid", "_id"]);
    let provenance =
        get_str_field(val, &["provenance", "source"]).unwrap_or_else(|| "imported".to_string());

    let created_at =
        get_str_field(val, &["created_at", "timestamp", "date", "created"]).and_then(|ts| {
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
        updated_at: None,
        expired_at: None,
        status: None,
    })
}

/// Extract all JSON values from raw text (handles arrays, wrapped objects, JSONL, and single items)
pub fn extract_raw_json_values(raw: &str) -> Result<Vec<serde_json::Value>> {
    anyhow::ensure!(!raw.trim().is_empty(), "Import file is empty");
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(arr) = parsed.as_array() {
            return Ok(arr.clone());
        }
        if let Some(obj) = parsed.as_object() {
            if obj.get("entities").and_then(|v| v.as_array()).is_some()
                || obj.get("relations").and_then(|v| v.as_array()).is_some()
            {
                let mut combined = Vec::new();
                if let Some(entities) = obj.get("entities").and_then(|v| v.as_array()) {
                    combined.extend(entities.iter().cloned());
                }
                if let Some(relations) = obj.get("relations").and_then(|v| v.as_array()) {
                    combined.extend(relations.iter().cloned());
                }
                return Ok(combined);
            }
            for wrapper in [
                "memories",
                "data",
                "items",
                "results",
                "documents",
                "records",
                "messages",
            ] {
                if let Some(arr) = obj.get(wrapper).and_then(|v| v.as_array()) {
                    return Ok(arr.clone());
                }
            }
            return Ok(vec![parsed]);
        }
        anyhow::bail!("Import expects JSON objects, an array, or JSONL");
    }
    let mut out = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        if !line.trim().is_empty() {
            out.push(
                serde_json::from_str(line)
                    .map_err(|e| anyhow::anyhow!("Invalid JSON at line {}: {}", index + 1, e))?,
            );
        }
    }
    Ok(out)
}

/// Ingests JSON exports (Memanto, Mem0, Letta, LangChain, raw logs, JSONL, or wrapped objects)
pub struct JsonMemoryImporter;

impl JsonMemoryImporter {
    pub fn parse_flexible(&self, raw: &str) -> Result<Vec<ImportCandidate>> {
        self.parse_with_enrichment(raw, false)
    }

    pub fn parse_with_enrichment(&self, raw: &str, enrich: bool) -> Result<Vec<ImportCandidate>> {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
            if let Some(format) = value.get("format").and_then(|v| v.as_str()) {
                let is_backup_envelope = format == "lightmem-backup-v1"
                    || format.starts_with("lightmem-backup")
                    || value.get("memories").and_then(|v| v.as_array()).is_some();
                if is_backup_envelope {
                    anyhow::ensure!(
                        format == "lightmem-backup-v1",
                        "Unsupported backup format: {}",
                        format
                    );
                    let records: Vec<MemoryRecord> = serde_json::from_value(
                        value
                            .get("memories")
                            .cloned()
                            .ok_or_else(|| anyhow::anyhow!("Backup is missing memories"))?,
                    )?;
                    return Ok(records.into_iter().map(ImportCandidate::from).collect());
                }
            }
        }
        let raw_values = extract_raw_json_values(raw)?;
        raw_values
            .iter()
            .enumerate()
            .map(|(index, val)| {
                let candidate = parse_json_candidate(val, enrich).ok_or_else(|| {
                    anyhow::anyhow!("Import record {} has no usable content", index + 1)
                })?;
                anyhow::ensure!(
                    candidate.confidence.is_finite() && (0.0..=1.0).contains(&candidate.confidence),
                    "Record {} has invalid confidence",
                    index + 1
                );
                Ok(candidate)
            })
            .collect()
    }
}

impl MemoryImporter for JsonMemoryImporter {
    fn name(&self) -> &str {
        "json-importer"
    }

    fn parse(&self, raw: &str) -> Result<Vec<ImportCandidate>> {
        self.parse_flexible(raw)
    }
}

/// Ingests OKF (Open Knowledge Format) markdown bundles, Memanto `memory.md` exports, and YAML-frontmatter `.md` files
pub struct OkfMemoryImporter;

impl OkfMemoryImporter {
    /// Parse a single YAML-frontmatter OKF markdown file (e.g. Memanto `--okf` directory item)
    fn parse_yaml_frontmatter_file(raw: &str) -> Option<ImportCandidate> {
        let trimmed = raw.trim_start();
        if !trimmed.starts_with("---") {
            return None;
        }
        let after_first = &trimmed[3..];
        let end_idx = after_first.find("\n---")?;
        let frontmatter = &after_first[..end_idx];
        let body = after_first[end_idx + 4..].trim();
        if body.is_empty() || body.starts_with("# ") && body.contains(" — OKF bundle") {
            return None;
        }

        let mut cat_str: Option<String> = None;
        let mut title: Option<String> = None;
        let mut id: Option<String> = None;
        let mut confidence = 0.9f32;
        let mut provenance = "imported:okf".to_string();
        let mut tags = Vec::new();
        let mut created_at: Option<DateTime<Utc>> = None;
        let mut in_tags = false;

        for line in frontmatter.lines() {
            let t = line.trim();
            if t.starts_with("- ") && in_tags {
                let tag = t
                    .trim_start_matches("- ")
                    .trim()
                    .trim_matches('\'')
                    .trim_matches('"');
                if !tag.is_empty() {
                    tags.push(tag.to_string());
                }
                continue;
            } else if !line.starts_with(' ') && !line.starts_with('-') {
                in_tags = false;
            }

            if let Some(rest) = t.strip_prefix("type:") {
                cat_str = Some(rest.trim().trim_matches('\'').trim_matches('"').to_string());
            } else if let Some(rest) = t.strip_prefix("title:") {
                title = Some(rest.trim().trim_matches('\'').trim_matches('"').to_string());
            } else if t == "tags:" {
                in_tags = true;
            } else if let Some(rest) = t.strip_prefix("id:") {
                id = Some(rest.trim().trim_matches('\'').trim_matches('"').to_string());
            } else if let Some(rest) = t.strip_prefix("confidence:") {
                if let Ok(c) = rest.trim().parse::<f32>() {
                    confidence = c;
                }
            } else if let Some(rest) = t.strip_prefix("provenance:") {
                let p = rest.trim().trim_matches('\'').trim_matches('"');
                if !p.is_empty() {
                    provenance = p.to_string();
                }
            } else if let Some(rest) = t.strip_prefix("at:") {
                let ts = rest.trim().trim_matches('\'').trim_matches('"');
                if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                    created_at = Some(dt.with_timezone(&Utc));
                }
            }
        }

        // Only treat as a YAML-frontmatter memory card if it had `type:` or `title:` in frontmatter
        if cat_str.is_none() && title.is_none() {
            return None;
        }

        let content = body.to_string();
        let resolved_cat = parse_category_lenient(cat_str.as_deref(), &content);
        let resolved_title = title.unwrap_or_else(|| {
            content
                .lines()
                .next()
                .unwrap_or("Imported Memory")
                .chars()
                .take(80)
                .collect()
        });

        Some(ImportCandidate {
            id,
            category: Some(resolved_cat),
            title: resolved_title,
            content,
            tags,
            confidence,
            provenance,
            created_at,
            updated_at: None,
            expired_at: None,
            status: None,
        })
    }
}

impl MemoryImporter for OkfMemoryImporter {
    fn name(&self) -> &str {
        "okf-importer"
    }

    fn parse(&self, raw: &str) -> Result<Vec<ImportCandidate>> {
        if raw
            .lines()
            .take(8)
            .any(|line| line.trim() == "format: \"okf-bundle-v2\"")
        {
            return raw
                .lines()
                .filter_map(|line| line.strip_prefix("<!-- lightmem-record "))
                .map(|line| {
                    let json = line
                        .strip_suffix(" -->")
                        .ok_or_else(|| anyhow::anyhow!("Malformed OKF record"))?;
                    let record: MemoryRecord = serde_json::from_str(json)?;
                    Ok(ImportCandidate::from(record))
                })
                .collect();
        }
        // 1. Check if this is a single YAML-frontmatter OKF file (from `memanto memory export --okf`)
        if !raw.contains("\n### ") {
            if let Some(single) = Self::parse_yaml_frontmatter_file(raw) {
                return Ok(vec![single]);
            }
        }

        // 2. Parse multi-section Markdown / OKF bundle (handles both LightMem OKF and Memanto `memory.md` exports)
        let mut candidates = Vec::new();
        let mut current_category = MemoryType::Fact;

        let mut active_title: Option<String> = None;
        let mut active_id: Option<String> = None;
        let mut active_tags: Vec<String> = Vec::new();
        let mut active_conf: f32 = 0.9;
        let mut active_created: Option<DateTime<Utc>> = None;
        let mut active_lines: Vec<String> = Vec::new();

        let flush_active = |candidates: &mut Vec<ImportCandidate>,
                            cat: MemoryType,
                            title: &mut Option<String>,
                            id: &mut Option<String>,
                            tags: &mut Vec<String>,
                            conf: &mut f32,
                            created: &mut Option<DateTime<Utc>>,
                            lines: &mut Vec<String>| {
            if let Some(t) = title.take() {
                let content = lines.join("\n").trim().to_string();
                if !content.is_empty() {
                    candidates.push(ImportCandidate {
                        id: id.take(),
                        category: Some(cat),
                        title: t,
                        content,
                        tags: std::mem::take(tags),
                        confidence: *conf,
                        provenance: "imported:okf".to_string(),
                        created_at: created.take(),
                        updated_at: None,
                        expired_at: None,
                        status: None,
                    });
                }
            }
            *id = None;
            tags.clear();
            *conf = 0.9;
            *created = None;
            lines.clear();
        };

        for line in raw.lines() {
            let trimmed = line.trim();

            // Category header (e.g. `## DECISION` or `## Decisions` or `## Instructions`)
            if let Some(h2) = trimmed.strip_prefix("## ") {
                flush_active(
                    &mut candidates,
                    current_category,
                    &mut active_title,
                    &mut active_id,
                    &mut active_tags,
                    &mut active_conf,
                    &mut active_created,
                    &mut active_lines,
                );
                let h2_clean = h2.trim().to_lowercase();
                current_category = match h2_clean.as_str() {
                    "instructions" | "instruction" => MemoryType::Instruction,
                    "facts" | "fact" => MemoryType::Fact,
                    "decisions" | "decision" => MemoryType::Decision,
                    "goals" | "goal" => MemoryType::Goal,
                    "commitments" | "commitment" => MemoryType::Commitment,
                    "preferences" | "preference" => MemoryType::Preference,
                    "context" | "contexts" => MemoryType::Context,
                    "events" | "event" => MemoryType::Event,
                    "learnings" | "learning" => MemoryType::Learning,
                    "observations" | "observation" => MemoryType::Observation,
                    "artifacts" | "artifact" => MemoryType::Artifact,
                    "errors" | "error" => MemoryType::Error,
                    "relationships" | "relationship" => MemoryType::Relationship,
                    "passwords" | "password" => MemoryType::Password,
                    other => parse_category_lenient(Some(other), ""),
                };
                continue;
            }

            // Memory card header (`### Title`)
            if let Some(h3) = trimmed.strip_prefix("### ") {
                flush_active(
                    &mut candidates,
                    current_category,
                    &mut active_title,
                    &mut active_id,
                    &mut active_tags,
                    &mut active_conf,
                    &mut active_created,
                    &mut active_lines,
                );
                let title_clean = h3.trim();
                if !title_clean.is_empty() {
                    active_title = Some(title_clean.to_string());
                }
                continue;
            }

            if active_title.is_none() {
                continue;
            }

            if trimmed == "---" {
                flush_active(
                    &mut candidates,
                    current_category,
                    &mut active_title,
                    &mut active_id,
                    &mut active_tags,
                    &mut active_conf,
                    &mut active_created,
                    &mut active_lines,
                );
                continue;
            }

            // LightMem OKF metadata bullets
            if let Some(rest) = trimmed.strip_prefix("- **ID:**") {
                active_id = Some(rest.replace('`', "").trim().to_string());
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("- **Tags:**") {
                active_tags = rest
                    .replace('`', "")
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("- **Confidence:**") {
                if let Ok(c) = rest.replace('`', "").trim().parse::<f32>() {
                    active_conf = c;
                }
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("- **Created:**") {
                let ts_str = rest.replace('`', "").trim().to_string();
                if let Ok(dt) = DateTime::parse_from_rfc3339(&ts_str) {
                    active_created = Some(dt.with_timezone(&Utc));
                }
                continue;
            }
            if trimmed.starts_with("- **") {
                continue;
            }

            // Memanto `memory.md` italic metadata footer:
            // `*Confidence: 1.0 | Status: active | Created: 2026-09-21T07:07:16 | Tags: `youtube`, `transcription`*`
            if trimmed.starts_with("*Confidence:") && trimmed.ends_with('*') {
                let inner = trimmed.trim_matches('*');
                for part in inner.split('|') {
                    let p = part.trim();
                    if let Some(c_str) = p.strip_prefix("Confidence:") {
                        if let Ok(c) = c_str.trim().parse::<f32>() {
                            active_conf = c;
                        }
                    } else if let Some(cr_str) = p.strip_prefix("Created:") {
                        let ts = cr_str.trim();
                        if let Ok(dt) = DateTime::parse_from_rfc3339(ts) {
                            active_created = Some(dt.with_timezone(&Utc));
                        } else if let Ok(ndt) =
                            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S")
                        {
                            active_created = Some(ndt.and_utc());
                        }
                    } else if let Some(t_str) = p.strip_prefix("Tags:") {
                        active_tags = t_str
                            .replace('`', "")
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                }
                continue;
            }

            // Strip Memanto `> ` blockquote prefix from content lines
            if let Some(quoted) = line.strip_prefix("> ") {
                active_lines.push(quoted.to_string());
            } else if trimmed == ">" {
                active_lines.push(String::new());
            } else {
                active_lines.push(line.to_string());
            }
        }

        flush_active(
            &mut candidates,
            current_category,
            &mut active_title,
            &mut active_id,
            &mut active_tags,
            &mut active_conf,
            &mut active_created,
            &mut active_lines,
        );

        Ok(candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_format_key_on_regular_memory_does_not_collide_with_backup_envelope() {
        let importer = JsonMemoryImporter;
        let raw = r#"{"format": "parquet", "content": "Analytics table uses Apache Parquet"}"#;
        let candidates = importer
            .parse(raw)
            .expect("Regular memory with 'format' key should not fail as unsupported backup");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].content, "Analytics table uses Apache Parquet");

        // Actual backup envelopes with unsupported versions must still error
        let bad_backup_1 = r#"{"format": "lightmem-backup-v999", "memories": []}"#;
        assert!(importer
            .parse(bad_backup_1)
            .unwrap_err()
            .to_string()
            .contains("Unsupported backup format"));

        let bad_backup_2 = r#"{"format": "custom-backup", "memories": []}"#;
        assert!(importer
            .parse(bad_backup_2)
            .unwrap_err()
            .to_string()
            .contains("Unsupported backup format"));
    }

    #[test]
    fn parses_anthropic_mcp_knowledge_graph_entities_and_relations() {
        let importer = JsonMemoryImporter;
        let raw = r#"{
            "entities": [
                {
                    "type": "entity",
                    "name": "Postgres",
                    "entityType": "database",
                    "observations": ["Runs on port 5432", "Uses WAL"]
                },
                {
                    "type": "entity",
                    "name": "Auth Service",
                    "entityType": "microservice",
                    "observations": ["We decided to use Ed25519 JWT tokens"]
                }
            ],
            "relations": [
                {
                    "type": "relation",
                    "from": "API Gateway",
                    "to": "Postgres",
                    "relationType": "connects_to"
                },
                {
                    "from": "Worker",
                    "to": "Redis",
                    "relationType": "reads_from"
                }
            ]
        }"#;

        let candidates = importer
            .parse(raw)
            .expect("Should parse Anthropic MCP knowledge graph");
        assert_eq!(candidates.len(), 4);

        assert_eq!(candidates[0].title, "Postgres");
        assert_eq!(candidates[0].content, "Runs on port 5432; Uses WAL");
        assert!(candidates[0].tags.contains(&"database".to_string()));
        assert_eq!(candidates[0].category, Some(MemoryType::Fact));

        assert_eq!(candidates[1].title, "Auth Service");
        assert_eq!(
            candidates[1].content,
            "We decided to use Ed25519 JWT tokens"
        );
        assert!(candidates[1].tags.contains(&"microservice".to_string()));
        assert_eq!(candidates[1].category, Some(MemoryType::Decision));

        assert_eq!(candidates[2].title, "API Gateway -> Postgres");
        assert_eq!(candidates[2].content, "API Gateway connects_to Postgres");
        assert_eq!(candidates[2].category, Some(MemoryType::Relationship));

        assert_eq!(candidates[3].title, "Worker -> Redis");
        assert_eq!(candidates[3].content, "Worker reads_from Redis");
        assert_eq!(candidates[3].category, Some(MemoryType::Relationship));
    }

    #[test]
    fn parse_category_lenient_avoids_overbroad_substring_collisions() {
        // False-positive substrings should fall through to MemoryType::infer(content)
        assert_eq!(
            parse_category_lenient(Some("primary_key"), "Users table uses UUIDv7"),
            MemoryType::Fact
        );
        assert_eq!(
            parse_category_lenient(Some("hotkey"), "Cmd+K opens the command palette"),
            MemoryType::Fact
        );
        assert_eq!(
            parse_category_lenient(Some("docker"), "Kokoro TTS runs in a container"),
            MemoryType::Fact
        );
        assert_eq!(
            parse_category_lenient(Some("multiprocess"), "Worker pool spawns 4 OS processes"),
            MemoryType::Fact
        );

        // True-positive category keywords must still map to their intended categories
        for k in [
            "key",
            "keys",
            "api_key",
            "apikey",
            "secret_key",
            "private_key",
            "passkey",
        ] {
            assert_eq!(
                parse_category_lenient(Some(k), "some value"),
                MemoryType::Password,
                "Expected Password for category '{}'",
                k
            );
        }
        for k in ["procedure", "process", "deployment_procedure"] {
            assert_eq!(
                parse_category_lenient(Some(k), "some value"),
                MemoryType::Instruction,
                "Expected Instruction for category '{}'",
                k
            );
        }
        for k in ["doc", "docs", "document", "documentation"] {
            assert_eq!(
                parse_category_lenient(Some(k), "some value"),
                MemoryType::Artifact,
                "Expected Artifact for category '{}'",
                k
            );
        }

        // Generic container types ("entity", "node", "item", "record") must allow content inference
        for container in ["entity", "node", "item", "record"] {
            assert_eq!(
                parse_category_lenient(Some(container), "We decided to adopt Rust 2024"),
                MemoryType::Decision,
                "Container type '{}' should allow inferring Decision from content",
                container
            );
            assert_eq!(
                parse_category_lenient(Some(container), "Always run cargo test before commit"),
                MemoryType::Instruction,
                "Container type '{}' should allow inferring Instruction from content",
                container
            );
        }
    }
}
