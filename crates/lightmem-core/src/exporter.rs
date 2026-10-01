use crate::models::{MemoryRecord, MemoryStatus, MemoryType};
use crate::storage::Storage;
use anyhow::{Context, Result};
use chrono::Utc;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Exporter;

impl Exporter {
    /// Export memories to an Open Knowledge Format (OKF) markdown document
    pub fn export_okf(storage: &Storage, output_path: Option<&Path>) -> Result<PathBuf> {
        let memories = storage.list_memories(None, Some(MemoryStatus::Active), None, 0)?;

        let mut grouped: BTreeMap<String, Vec<MemoryRecord>> = BTreeMap::new();
        for m in memories {
            grouped.entry(m.category.as_str().to_string()).or_default().push(m);
        }

        let mut md = String::new();
        md.push_str("---\n");
        md.push_str("title: \"LightMem Open Knowledge Format (OKF) Export\"\n");
        md.push_str(&format!("date: \"{}\"\n", Utc::now().to_rfc3339()));
        md.push_str("format: \"okf-bundle-v1\"\n");
        md.push_str("---\n\n");

        md.push_str("# 🧠 Open Knowledge Base\n\n");
        md.push_str(&format!("> Exported on: **{}**\n\n", Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));

        for cat in MemoryType::ALL {
            let cat_str = cat.as_str();
            if let Some(items) = grouped.get(cat_str) {
                if items.is_empty() {
                    continue;
                }
                md.push_str(&format!("## {}\n\n", cat_str.to_uppercase()));

                for item in items {
                    md.push_str(&format!("### {}\n", item.title));
                    md.push_str(&format!("- **ID:** `{}`\n", item.id));
                    md.push_str(&format!("- **Confidence:** `{:.2}`\n", item.confidence));
                    if !item.tags.is_empty() {
                        md.push_str(&format!("- **Tags:** `{}`\n", item.tags.join("`, `")));
                    }
                    md.push_str(&format!("- **Created:** {}\n\n", item.created_at.to_rfc3339()));
                    md.push_str(&format!("{}\n\n", item.content.trim()));
                    md.push_str("---\n\n");
                }
            }
        }

        let target = if let Some(p) = output_path {
            p.to_path_buf()
        } else {
            let export_dir = dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".lightmem")
                .join("exports");
            std::fs::create_dir_all(&export_dir)?;
            let filename = format!("lightmem_export_{}.md", Utc::now().format("%Y%m%d_%H%M%S"));
            export_dir.join(filename)
        };

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&target, md)
            .with_context(|| format!("Failed to write export to {:?}", target))?;

        Ok(target)
    }
}
