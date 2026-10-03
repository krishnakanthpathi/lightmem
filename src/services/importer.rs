use crate::models::{MemoryRecord, MemoryType};
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
        if trimmed.contains("pass")
            || trimmed.contains("secret")
            || trimmed.contains("token")
            || trimmed.contains("cred")
            || trimmed.contains("key")
        {
            return MemoryType::Password;
        }
        if trimmed.contains("instruct")
            || trimmed.contains("rule")
            || trimmed.contains("runbook")
            || trimmed.contains("proc")
        {
            return MemoryType::Instruction;
        }
        if trimmed.contains("decis") || trimmed.contains("choice") {
            return MemoryType::Decision;
        }
        if trimmed.contains("pref") || trimmed.contains("like") {
            return MemoryType::Preference;
        }
        if trimmed.contains("learn") || trimmed.contains("lesson") || trimmed.contains("insight") {
            return MemoryType::Learning;
        }
        if trimmed.contains("goal") || trimmed.contains("target") || trimmed.contains("objective") {
            return MemoryType::Goal;
        }
        if trimmed.contains("commit") || trimmed.contains("todo") || trimmed.contains("task") {
            return MemoryType::Commitment;
        }
        if trimmed.contains("artif") || trimmed.contains("code") || trimmed.contains("doc") {
            return MemoryType::Artifact;
        }
        if trimmed.contains("event") || trimmed.contains("incident") || trimmed.contains("meeting")
        {
            return MemoryType::Event;
        }
        if trimmed.contains("relat") || trimmed.contains("team") || trimmed.contains("owner") {
            return MemoryType::Relationship;
        }
        if trimmed.contains("obser") || trimmed.contains("metric") {
            return MemoryType::Observation;
        }
        if trimmed.contains("error") || trimmed.contains("bug") || trimmed.contains("issue") {
            return MemoryType::Error;
        }
        if trimmed.contains("context") || trimmed.contains("background") {
            return MemoryType::Context;
        }
    }

    MemoryType::infer(content)
}

/// Parse a single JSON value into an ImportCandidate using the Fallback Ladder + Native Needle 3 C-FFI + Pure-Rust Heuristic
pub fn parse_single_json_value(val: &serde_json::Value) -> Option<ImportCandidate> {
    let mut heuristic_tags = Vec::new();
    let mut needle_title: Option<String> = None;
    let mut needle_cat: Option<String> = None;

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
        // Extract the primary descriptive string field verbatim as content, and collect short string fields as tags.
        let obj = val.as_object()?;
        let reserved = [
            "id", "memory_id", "uuid", "_id", "title", "name", "summary", "heading", "subject",
            "category", "memory_type", "type", "kind", "type_name", "provenance", "source",
            "created_at", "timestamp", "date", "created",
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

    let explicit_title = get_str_field(val, &["title", "name", "summary", "heading", "subject"]);
    let cat_str = get_str_field(
        val,
        &["category", "memory_type", "type", "kind", "type_name"],
    );

    // Consult Native Needle 3 C-FFI when category or title is omitted on the imported JSON record
    if cat_str.is_none() || explicit_title.is_none() {
        if let Some((_, n_title, n_cat, n_tags)) =
            crate::reranker::NeedleReranker::extract_import_record_via_needle(&content)
        {
            needle_title = n_title;
            needle_cat = n_cat;
            for t in n_tags {
                if !heuristic_tags.contains(&t) {
                    heuristic_tags.push(t);
                }
            }
        }
    }

    // Rule inference (MemoryType::infer) takes priority for unambiguous signals (password, panic, we decided, always...),
    // and falls back to Needle 3's extracted category when rule inference defaults to Fact.
    let inferred_rule_cat = MemoryType::infer(&content);
    let category = if let Some(explicit_c) = cat_str.as_deref() {
        Some(parse_category_lenient(Some(explicit_c), &content))
    } else if inferred_rule_cat != MemoryType::Fact {
        Some(inferred_rule_cat)
    } else if let Some(nc) = needle_cat.as_deref() {
        Some(parse_category_lenient(Some(nc), &content))
    } else {
        Some(MemoryType::Fact)
    };

    let title = explicit_title
        .or(needle_title)
        .unwrap_or_else(|| {
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

/// Ingests JSON exports (Memanto, Mem0, Letta, LangChain, raw logs, JSONL, or wrapped objects)
pub struct JsonMemoryImporter;

impl JsonMemoryImporter {
    pub fn parse_flexible(&self, raw: &str) -> Result<Vec<ImportCandidate>> {
        let raw_values = extract_raw_json_values(raw);
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
                let tag = t.trim_start_matches("- ").trim().trim_matches('\'').trim_matches('"');
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
        })
    }
}

impl MemoryImporter for OkfMemoryImporter {
    fn name(&self) -> &str {
        "okf-importer"
    }

    fn parse(&self, raw: &str) -> Result<Vec<ImportCandidate>> {
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
