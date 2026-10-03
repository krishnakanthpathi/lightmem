use crate::models::{
    LightMemConfig, MemoryRecord, PaginatedMemories, ScoredMemory, StorageStats,
};
use crate::services::AnswerResult;
use anyhow::Result;
use colored::*;
use std::path::Path;

pub struct CliView;

impl CliView {
    pub fn format_path(p: &Path) -> String {
        if let Ok(home) = std::env::var("HOME") {
            let home_path = Path::new(&home);
            if let Ok(rel) = p.strip_prefix(home_path) {
                return format!("~/{}", rel.display());
            }
        }
        p.display().to_string()
    }

    pub fn render_remembered(memory: &MemoryRecord, db_path: &Path, json: bool) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(memory)?);
            return Ok(());
        }

        let short_id: String = memory.id.chars().take(8).collect();
        let db_str = Self::format_path(db_path);
        println!(
            "{} Stored [{}] {} ({}) in {}",
            "✔".green().bold(),
            memory.category.as_str().cyan().bold(),
            memory.title.bold(),
            short_id.yellow(),
            db_str.dimmed()
        );
        Ok(())
    }

    pub fn render_recall(
        query: &str,
        results: &[ScoredMemory],
        db_path: &Path,
        json: bool,
    ) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(results)?);
            return Ok(());
        }

        if results.is_empty() {
            println!("{}", "No relevant memories found.".dimmed());
            return Ok(());
        }

        let db_str = Self::format_path(db_path);
        let count_label = if results.len() == 1 {
            "memory"
        } else {
            "memories"
        };
        println!(
            "{} Found {} {} for \"{}\" in {}:",
            "🔍".bold(),
            results.len().to_string().cyan().bold(),
            count_label,
            query.bold(),
            db_str.dimmed()
        );
        println!();

        for (idx, r) in results.iter().enumerate() {
            let m = &r.memory;
            let short_id: String = m.id.chars().take(8).collect();
            println!(
                "{}. [{}] {} {}",
                idx + 1,
                m.category.as_str().cyan().bold(),
                m.title.bold(),
                format!("(score: {:.2})", r.score).dimmed()
            );
            println!("   {}", m.content.replace('\n', "\n   "));
            let mut meta = vec![format!("ID: {}", short_id.yellow())];
            if !m.tags.is_empty() {
                meta.push(format!("Tags: {}", m.tags.join(", ").blue()));
            }
            println!("   {}", meta.join(" | ").dimmed());
            if idx + 1 < results.len() {
                println!();
            }
        }
        Ok(())
    }

    pub fn render_paginated_list(
        paginated: &PaginatedMemories,
        db_path: &Path,
        json: bool,
    ) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(paginated)?);
            return Ok(());
        }

        if paginated.items.is_empty() {
            if paginated.total > 0 {
                println!(
                    "{}",
                    format!(
                        "No memories on this page/offset (total matching: {}, offset: {}).",
                        paginated.total, paginated.offset
                    )
                    .dimmed()
                );
            } else {
                println!("{}", "No memories found.".dimmed());
            }
            return Ok(());
        }

        let db_str = Self::format_path(db_path);
        let start_idx = paginated.offset + 1;
        let end_idx = paginated.offset + paginated.items.len();
        println!(
            "{} Displaying {}–{} of {} memories (page {}/{}, offset {}) from {}:",
            "📋".bold(),
            start_idx.to_string().cyan().bold(),
            end_idx.to_string().cyan().bold(),
            paginated.total.to_string().bold(),
            paginated.page.to_string().cyan(),
            paginated.total_pages.to_string().cyan(),
            paginated.offset,
            db_str.dimmed()
        );
        println!();

        for m in &paginated.items {
            let short_id: String = m.id.chars().take(8).collect();
            let first_line = m.content.lines().next().unwrap_or("").trim();
            let snippet = if first_line.len() > 64 {
                format!("{}...", &first_line[..61])
            } else {
                first_line.to_string()
            };
            println!(
                "• [{}] {} {} - {}",
                m.category.as_str().cyan().bold(),
                m.title.bold(),
                format!("({})", short_id).yellow(),
                snippet.dimmed()
            );
        }

        if paginated.has_more {
            let next_offset = paginated.offset + paginated.items.len();
            let next_page = paginated.page + 1;
            println!();
            println!(
                "  {}",
                format!(
                    "↳ More memories available (next: --page {} -l {}  or  --offset {} -l {})",
                    next_page, paginated.limit, next_offset, paginated.limit
                )
                .dimmed()
            );
        }
        Ok(())
    }

    pub fn render_forget(id: &str, ok: bool, hard: bool, json: bool) -> Result<()> {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "id": id,
                    "success": ok,
                    "hard_deleted": hard,
                })
            );
            return Ok(());
        }

        if ok {
            let action = if hard {
                "Permanently deleted"
            } else {
                "Retired (soft-expired)"
            };
            println!("{} {} memory {}", "✔".green().bold(), action, id.yellow());
        } else {
            println!("{} Memory '{}' not found", "✖".red().bold(), id);
        }
        Ok(())
    }

    pub fn render_export(exported_path: &Path) {
        println!(
            "{} Exported OKF bundle to: {}",
            "✔".green().bold(),
            exported_path.display().to_string().cyan().bold()
        );
    }

    pub fn render_import(count: usize, file: &Path) {
        println!(
            "{} Successfully imported {} memories from {:?}",
            "✔".green().bold(),
            count.to_string().cyan().bold(),
            file
        );
    }

    pub fn render_answer(result: &AnswerResult, json: bool) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(result)?);
            return Ok(());
        }

        let conf_pct = (result.confidence * 100.0).round() as u32;
        println!(
            "{} Answer [reranker: {}] (confidence: {}%):",
            "💡".bold(),
            result.reranker_used.cyan().bold(),
            conf_pct
        );
        println!();
        println!("  {}", result.answer.green().bold());
        println!();

        if let Some(m) = &result.selected_memory {
            let short_id: String = m.id.chars().take(8).collect();
            println!(
                "  {} Source: [{}] {} {}",
                "📌".dimmed(),
                m.category.as_str().cyan(),
                m.title.bold(),
                format!("(ID: {})", short_id).dimmed()
            );
        }
        Ok(())
    }

    pub fn render_config(cfg: &LightMemConfig, updated: bool) {
        if updated {
            println!("{} Configuration updated successfully!", "✔".green().bold());
        }

        println!("Current Configuration:");
        println!("  Backend:          {}", cfg.backend.cyan().bold());
        println!(
            "  ONNX Model/Path:  {}",
            cfg.onnx_model.as_deref().unwrap_or("bge-small").cyan()
        );
        println!("  Reranker:         {}", cfg.reranker.cyan().bold());
        println!("  Ollama URL:       {}", cfg.ollama_url.dimmed());
        println!("  Ollama Model:     {}", cfg.embedding_model.dimmed());
        println!(
            "  Config File:      {}",
            LightMemConfig::config_file().display()
        );
    }

    pub fn render_stats(stats: &StorageStats, db_path: &Path, json: bool) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(stats)?);
            return Ok(());
        }

        let db_str = Self::format_path(db_path);
        println!("{} LightMem Storage Statistics", "📊".bold());
        println!("  Database:  {}", db_str.cyan().bold());
        println!(
            "  Memories:  {} total  ({} active, {} expired)",
            stats.total_memories.to_string().bold(),
            stats.active_memories.to_string().green(),
            stats.expired_memories.to_string().yellow()
        );
        println!(
            "  Vectors:   {} embedded",
            stats.total_vectors.to_string().blue()
        );
        if !stats.by_category.is_empty() {
            println!();
            println!("  Breakdown by Category:");
            for (cat, count) in &stats.by_category {
                println!("    • {:<14} {}", cat.cyan(), count);
            }
        }
        Ok(())
    }
}
