use crate::models::{
    GraphSnapshot, LightMemConfig, MemoryConflict, MemoryRecord, PaginatedMemories, RelatedMemory,
    ScoredMemory, StorageStats,
};
use crate::services::AnswerResult;
use anyhow::Result;
use colored::*;
use std::path::Path;

/// 22x38 pixel matrix of the LightMem Crimson Cloud emblem (`W` = White outline/swirl, `R` = Crimson fill, ` ` = Transparent).
/// Rendered via Unicode half-blocks (`▀`, `▄`, `█`) into 11 terminal rows.
const CLOUD_LOGO_MATRIX: [&str; 22] = [
    "                     WWWWWW           ",
    "                   WWWRRRRWWW         ",
    "                  WRRRRRRRRRWW        ",
    "              WWWWRRRRRRRRRRRWWW      ",
    "             WWRRWRRRRRWWRRRRWWWWWW   ",
    "            WRRRWWRRRRRRWRRRRRRRRRWW  ",
    "           WWRRRWWRRRRRRWRRRRRRRRRRWW ",
    "           WRRRRRWRRRRRRWRRRRRRRRRRRW ",
    "           WRRRRRRWWWRWWWRRRRRRRRRRRWW",
    "        WWWWRRRRRRRRWWWRRRRRRRRRRRRRRW",
    "W      WWRRRRRRRRRRRRRRRRRRRRRRRRRRRRW",
    "WWW  WWRRRRRRRRRRRRRRRRRRRRWWWWRRRRRRW",
    "WWWWWWRRRRRRRRRRRRRRRRRRRRWWRRRRRRRRWW",
    " WRRRRRRRRRRRRRRRRRRRRRRRRWRRRRRRRRRW ",
    " WWRRRRRRRRWWWWWRRRRRRRRRRWRRRRRRRRWW ",
    "  WRRRRRRRWRRRRWWRRRRRRRRRWWRRRRRRWW  ",
    "   WRRRRRWRRRRRRWRRRRRRRRRRWWRRRRWW   ",
    "    WRRRRWRRRRRRRRRRRRRRRRRRWWWWWW    ",
    "     WWWWWWRRRRRRRRRRRRRRRRRRW        ",
    "          WRRRRRRRRWRRRRRRRRW         ",
    "          WWRRRRRWWWWWRRRWWW          ",
    "            WWWWWW   WWWWW            ",
];

pub struct CliView;

impl CliView {
    // Theme palette (24-bit TrueColor)
    fn crimson(s: &str) -> ColoredString {
        s.truecolor(220, 38, 38)
    }

    fn crimson_bold(s: &str) -> ColoredString {
        s.truecolor(220, 38, 38).bold()
    }

    fn rose(s: &str) -> ColoredString {
        s.truecolor(248, 113, 113)
    }

    fn violet_bold(s: &str) -> ColoredString {
        s.truecolor(167, 139, 250).bold()
    }

    fn gold(s: &str) -> ColoredString {
        s.truecolor(251, 191, 36)
    }

    fn white_bold(s: &str) -> ColoredString {
        s.truecolor(248, 250, 252).bold()
    }

    fn slate(s: &str) -> ColoredString {
        s.truecolor(113, 113, 122)
    }

    fn emerald_bold(s: &str) -> ColoredString {
        s.truecolor(52, 211, 153).bold()
    }

    /// Render a single pair of vertical pixel rows from `CLOUD_LOGO_MATRIX` using Unicode half-blocks.
    fn render_logo_row(pair_idx: usize) -> String {
        let top_row = CLOUD_LOGO_MATRIX[pair_idx * 2].as_bytes();
        let bot_row = CLOUD_LOGO_MATRIX[pair_idx * 2 + 1].as_bytes();
        let width = top_row.len().min(bot_row.len());

        let mut out = String::from("  ");
        for x in 0..width {
            let t = top_row[x] as char;
            let b = bot_row[x] as char;

            let color_rgb = |c: char| -> Option<(u8, u8, u8)> {
                match c {
                    'W' => Some((248, 250, 252)),
                    'R' => Some((204, 36, 36)),
                    _ => None,
                }
            };

            match (color_rgb(t), color_rgb(b)) {
                (None, None) => out.push(' '),
                (Some((r, g, bl)), None) => {
                    out.push_str(&format!("\x1b[38;2;{};{};{}m▀\x1b[0m", r, g, bl));
                }
                (None, Some((r, g, bl))) => {
                    out.push_str(&format!("\x1b[38;2;{};{};{}m▄\x1b[0m", r, g, bl));
                }
                (Some((r1, g1, b1)), Some((r2, g2, b2))) => {
                    if (r1, g1, b1) == (r2, g2, b2) {
                        out.push_str(&format!("\x1b[38;2;{};{};{}m█\x1b[0m", r1, g1, b1));
                    } else {
                        out.push_str(&format!(
                            "\x1b[38;2;{};{};{};48;2;{};{};{}m▀\x1b[0m",
                            r1, g1, b1, r2, g2, b2
                        ));
                    }
                }
            }
        }
        out
    }

    /// Returns the multi-line Crimson Cloud intro banner string (used in `--help` and intro HUD).
    pub fn banner_string(db_path: Option<&Path>, stats: Option<&StorageStats>) -> String {
        let cfg = LightMemConfig::load();
        let db_display = db_path
            .map(Self::format_path)
            .unwrap_or_else(|| "~/.lightmem/memories.db".to_string());

        let mem_summary = if let Some(s) = stats {
            format!(
                "{} active  {}  {} vectors",
                s.active_memories,
                Self::slate("·"),
                s.total_vectors
            )
        } else {
            "hybrid SQLite FTS5 + vector store".to_string()
        };

        let embedder_label = match cfg.backend.as_str() {
            "onnx" => {
                let raw_em = cfg.onnx_model.as_deref().unwrap_or("bge-small");
                let short_em = match raw_em {
                    "Xenova/bge-small-en-v1.5" | "bge-small-en-v1.5" => "bge-small",
                    "Xenova/all-MiniLM-L6-v2" | "all-minilm-l6-v2" => "minilm",
                    "nomic-ai/nomic-embed-text-v1.5" | "nomic-embed-text" => "nomic",
                    other => other,
                };
                format!("onnx ({})", short_em)
            }
            "ollama" => format!("ollama ({})", cfg.embedding_model),
            other => other.to_string(),
        };
        let norm_reranker = LightMemConfig::normalize_reranker(&cfg.reranker, None);
        let reranker_label = match norm_reranker.as_str() {
            "minilm-squad2" | "tinyroberta-squad2" => format!("onnx ({})", norm_reranker),
            "ollama" => "ollama (auto)".to_string(),
            other if other.starts_with("ollama:") => {
                format!("ollama ({})", &other[7..])
            }
            other => other.to_string(),
        };

        let right_lines: [String; 11] = [
            String::new(),
            format!(
                "{}  {}  {}",
                Self::crimson_bold("❖"),
                Self::white_bold("L I G H T M E M"),
                Self::slate(&format!("v{}", env!("CARGO_PKG_VERSION")))
            ),
            format!(
                "{}",
                Self::crimson("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
            ),
            format!(
                "{} {}",
                Self::crimson_bold("▸"),
                "Ultra-Fast Local Agent Memory Engine".truecolor(228, 228, 231)
            ),
            format!(
                "  {} {:<10} {}",
                Self::slate("◫"),
                Self::slate("Vault"),
                Self::white_bold(&db_display)
            ),
            format!(
                "  {} {:<10} {}",
                Self::slate("◈"),
                Self::slate("Status"),
                mem_summary.truecolor(212, 212, 216)
            ),
            format!(
                "  {} {:<10} {} {} {}",
                Self::slate("✦"),
                Self::slate("Engines"),
                Self::violet_bold(&embedder_label),
                Self::slate("+"),
                Self::crimson_bold(&reranker_label)
            ),
            format!(
                "{}",
                Self::slate("────────────────────────────────────────────────")
            ),
            format!(
                "  {} {:<12} {} {:<12} {} {}",
                Self::crimson_bold("◈"),
                "remember",
                Self::crimson_bold("⌖"),
                "recall",
                Self::crimson_bold("✦"),
                "answer"
            ),
            format!(
                "  {} {:<12} {} {:<12} {} {}",
                Self::crimson_bold("≡"),
                "list",
                Self::crimson_bold("◫"),
                "graph",
                Self::crimson_bold("⇄"),
                "import / export"
            ),
            String::new(),
        ];

        let mut lines = Vec::with_capacity(13);
        lines.push(String::new());
        for (i, right_line) in right_lines.iter().enumerate() {
            let logo_part = Self::render_logo_row(i);
            lines.push(format!("{}   {}", logo_part, right_line));
        }
        lines.join("\n")
    }

    /// Render the full interactive intro screen when `lmem` is invoked with no subcommand.
    pub fn render_intro(db_path: &Path, stats: &StorageStats) {
        println!("{}", Self::banner_string(Some(db_path), Some(stats)));
        println!();
        println!(
            "  {} {}",
            Self::crimson_bold("╭─"),
            Self::white_bold("QUICK COMMANDS")
        );
        let cmds = [
            (
                "│",
                "lmem remember \"<fact>\" -t decision",
                "Store a categorized memory (auto-links [[...]])",
            ),
            (
                "│",
                "lmem recall \"<query>\" --limit 5",
                "Hybrid BM25 + vector search",
            ),
            (
                "│",
                "lmem answer \"<question>\"",
                "Extractive QA via ONNX or Ollama",
            ),
            (
                "│",
                "lmem graph",
                "Terminal network tree & connection map",
            ),
            (
                "│",
                "lmem related \"<id>\" --hops 2",
                "Multi-hop knowledge graph traversal",
            ),
            (
                "│",
                "lmem list --page 1 --limit 20",
                "Paginated chronological index",
            ),
            (
                "│",
                "lmem connect <agent>",
                "Deploy skill to Antigravity, Codex, Hermes, etc.",
            ),
            (
                "╰─",
                "lmem stats  |  lmem --help",
                "Inspect vault telemetry & flags",
            ),
        ];
        for (branch, cmd, desc) in cmds {
            println!(
                "  {:<2} {:<36} {}",
                Self::crimson_bold(branch),
                cmd.truecolor(248, 250, 252),
                Self::slate(desc)
            );
        }
        println!();
    }

    pub fn format_path(p: &Path) -> String {
        if let Some(home_path) = dirs::home_dir() {
            if let Ok(rel) = p.strip_prefix(&home_path) {
                return format!("~/{}", rel.display().to_string().replace('\\', "/"));
            }
        }
        p.display().to_string()
    }

    fn confidence_bar(score: f32) -> String {
        let clamped = score.clamp(0.0, 1.0);
        let filled = (clamped * 10.0).round() as usize;
        let empty = 10usize.saturating_sub(filled);
        format!(
            "{}{}",
            "▰".repeat(filled).truecolor(220, 38, 38),
            "▱".repeat(empty).truecolor(82, 82, 91)
        )
    }

    pub fn render_remembered(memory: &MemoryRecord, db_path: &Path, json: bool) -> Result<()> {
        Self::render_remembered_with_conflicts(memory, &[], false, db_path, json)
    }

    pub fn render_remembered_with_conflicts(
        memory: &MemoryRecord,
        conflicts: &[MemoryConflict],
        superseded: bool,
        db_path: &Path,
        json: bool,
    ) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(memory)?);
            return Ok(());
        }

        let short_id: String = memory.id.chars().take(8).collect();
        let db_str = Self::format_path(db_path);
        let ttl_suffix = memory
            .expired_at
            .map(|exp| {
                format!(
                    " {} {}",
                    Self::slate("· TTL expires"),
                    Self::gold(&exp.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                )
            })
            .unwrap_or_default();

        println!(
            "{} {} [{}] {} {} {} {}{}",
            Self::crimson_bold("◈"),
            Self::emerald_bold("Stored"),
            Self::violet_bold(memory.category.as_str()),
            Self::white_bold(&memory.title),
            Self::gold(&format!("({})", short_id)),
            Self::slate("in"),
            Self::slate(&db_str),
            ttl_suffix
        );

        for c in conflicts {
            let old_short: String = c.older_memory.id.chars().take(8).collect();
            let old_ts = c
                .older_memory
                .created_at
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string();
            let sim_pct = (c.similarity * 100.0).round() as u32;
            if superseded {
                println!(
                    "  {} {} {} {} {} {}",
                    Self::crimson_bold("⇄"),
                    Self::emerald_bold("Merged & superseded"),
                    Self::gold(&format!("({})", old_short)),
                    Self::slate(&format!("[OLDER · {}]", old_ts)),
                    Self::slate(&c.older_memory.title),
                    Self::rose(&format!("({}% match)", sim_pct))
                );
            } else {
                println!(
                    "  {} {} {} {} {} {} {}",
                    Self::crimson_bold("⇄"),
                    Self::gold("Overlap/conflict detected with"),
                    Self::gold(&format!("({})", old_short)),
                    Self::slate(&format!("[OLDER · {}]", old_ts)),
                    Self::slate(&c.older_memory.title),
                    Self::rose(&format!("({}% match)", sim_pct)),
                    Self::slate("— pass --supersede or run `lmem conflicts`")
                );
            }
        }
        Ok(())
    }

    pub fn render_conflict_pair(idx: usize, total: usize, c: &MemoryConflict, resolved: bool) {
        let old_short: String = c.older_memory.id.chars().take(8).collect();
        let new_short: String = c.newer_memory.id.chars().take(8).collect();
        let old_ts = c
            .older_memory
            .created_at
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string();
        let new_ts = c
            .newer_memory
            .created_at
            .format("%Y-%m-%d %H:%M:%S UTC")
            .to_string();
        let sim_pct = (c.similarity * 100.0).round() as u32;
        let ov_pct = (c.overlap_ratio * 100.0).round() as u32;

        println!(
            "  {} [{}] {} {} {}",
            Self::crimson_bold(&format!("{:02}/{:02}.", idx + 1, total)),
            Self::violet_bold(c.newer_memory.category.as_str()),
            Self::white_bold(&c.newer_memory.title),
            Self::confidence_bar(c.similarity),
            Self::rose(&format!("{}% sim · {}% overlap", sim_pct, ov_pct))
        );
        println!(
            "     {} {} {} {} {}",
            Self::slate("├─"),
            Self::rose(&format!("[OLDER · {}]", old_ts)),
            Self::gold(&format!("({})", old_short)),
            Self::slate(&c.older_memory.content),
            if resolved {
                Self::rose("[merged & retired]")
            } else {
                Self::gold("[active]")
            }
        );
        println!(
            "     {} {} {} {} {}",
            Self::slate("╰─"),
            Self::emerald_bold(&format!("[NEWER · {}]", new_ts)),
            Self::gold(&format!("({})", new_short)),
            Self::white_bold(&c.newer_memory.content),
            Self::emerald_bold("[active]")
        );
    }

    pub fn render_conflicts(
        conflicts: &[MemoryConflict],
        resolved: bool,
        db_path: &Path,
        json: bool,
    ) -> Result<()> {
        if json {
            let payload = serde_json::json!({
                "resolved": resolved,
                "count": conflicts.len(),
                "conflicts": conflicts,
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            return Ok(());
        }

        let db_str = Self::format_path(db_path);
        if conflicts.is_empty() {
            println!(
                "{} {} {} {}",
                Self::crimson_bold("⇄"),
                Self::emerald_bold("Zero active conflicts or duplicates found"),
                Self::slate("in"),
                Self::slate(&db_str)
            );
            return Ok(());
        }

        let header = if resolved {
            "Merged & resolved conflicts"
        } else {
            "Active conflicts & overlapping memories detected"
        };
        println!(
            "{} {} ({}) {} {}",
            Self::crimson_bold("⇄"),
            Self::white_bold(header),
            Self::crimson_bold(&conflicts.len().to_string()),
            Self::slate("in"),
            Self::slate(&db_str)
        );
        for (idx, c) in conflicts.iter().enumerate() {
            Self::render_conflict_pair(idx, conflicts.len(), c, resolved);
        }
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
            println!(
                "{} {}",
                Self::crimson_bold("⌖"),
                Self::slate("No relevant memories found.")
            );
            return Ok(());
        }

        let db_str = Self::format_path(db_path);
        let count_label = if results.len() == 1 {
            "memory"
        } else {
            "memories"
        };
        println!(
            "{} Found {} {} for {} {} {}",
            Self::crimson_bold("⌖"),
            Self::crimson_bold(&results.len().to_string()),
            count_label,
            Self::white_bold(&format!("\"{}\"", query)),
            Self::slate("in"),
            Self::slate(&db_str)
        );
        println!();

        for (idx, r) in results.iter().enumerate() {
            let m = &r.memory;
            let short_id: String = m.id.chars().take(8).collect();
            println!(
                "  {} [{}] {} {} {}",
                Self::crimson_bold(&format!("{:02}.", idx + 1)),
                Self::violet_bold(m.category.as_str()),
                Self::white_bold(&m.title),
                Self::confidence_bar(r.score),
                Self::rose(&format!("{:.2}", r.score))
            );
            println!(
                "     {} {}",
                Self::slate("│"),
                m.content
                    .replace('\n', &format!("\n     {} ", Self::slate("│")))
            );
            let mut meta = vec![format!("{} {}", Self::slate("ID:"), Self::gold(&short_id))];
            if !m.tags.is_empty() {
                meta.push(format!(
                    "{} {}",
                    Self::slate("Tags:"),
                    m.tags.join(", ").truecolor(167, 139, 250)
                ));
            }
            println!(
                "     {} {}",
                Self::slate("╰─▸"),
                meta.join(&format!(" {} ", Self::slate("·")))
            );
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
                    "{} {}",
                    Self::crimson_bold("≡"),
                    Self::slate(&format!(
                        "No memories on this page/offset (total matching: {}, offset: {}).",
                        paginated.total, paginated.offset
                    ))
                );
            } else {
                println!(
                    "{} {}",
                    Self::crimson_bold("≡"),
                    Self::slate("No memories found.")
                );
            }
            return Ok(());
        }

        let db_str = Self::format_path(db_path);
        let start_idx = paginated.offset + 1;
        let end_idx = paginated.offset + paginated.items.len();
        println!(
            "{} Displaying {}–{} of {} memories {} {}",
            Self::crimson_bold("≡"),
            Self::crimson_bold(&start_idx.to_string()),
            Self::crimson_bold(&end_idx.to_string()),
            Self::white_bold(&paginated.total.to_string()),
            Self::slate(&format!(
                "(page {}/{}, offset {})",
                paginated.page, paginated.total_pages, paginated.offset
            )),
            Self::slate(&format!("from {}", db_str))
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
                "  {} [{}] {} {} {} {}",
                Self::crimson_bold("▪"),
                Self::violet_bold(m.category.as_str()),
                Self::white_bold(&m.title),
                Self::gold(&format!("({})", short_id)),
                Self::slate("─"),
                Self::slate(&snippet)
            );
        }

        if paginated.has_more {
            let next_offset = paginated.offset + paginated.items.len();
            let next_page = paginated.page + 1;
            println!();
            println!(
                "  {} {}",
                Self::crimson_bold("╰─▸"),
                Self::slate(&format!(
                    "More memories available (next: --page {} -l {}  or  --offset {} -l {})",
                    next_page, paginated.limit, next_offset, paginated.limit
                ))
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
            println!(
                "{} {} memory {}",
                Self::crimson_bold("◈"),
                Self::white_bold(action),
                Self::gold(id)
            );
        } else {
            println!(
                "{} Memory '{}' not found",
                Self::crimson_bold("✕"),
                Self::gold(id)
            );
        }
        Ok(())
    }

    pub fn render_inspect(
        memory: &MemoryRecord,
        links: &[crate::models::link::MemoryLink],
        db_path: &Path,
        json: bool,
    ) -> Result<()> {
        if json {
            let obj = serde_json::json!({
                "memory": memory,
                "links": links,
            });
            println!("{}", serde_json::to_string_pretty(&obj)?);
            return Ok(());
        }

        let short_id: String = memory.id.chars().take(8).collect();
        let db_str = Self::format_path(db_path);
        println!(
            "\n{} {} {} [{}] {}",
            Self::crimson_bold("◈"),
            Self::white_bold(&memory.title),
            Self::gold(&format!("({})", short_id)),
            Self::violet_bold(memory.category.as_str()),
            if memory.status == crate::models::MemoryStatus::Active {
                Self::emerald_bold("● active")
            } else {
                Self::slate("○ expired")
            }
        );
        println!("  {} {}", Self::slate("Vault:"), Self::slate(&db_str));
        println!("\n  {}", memory.content);

        let tags_str = if memory.tags.is_empty() {
            "none".to_string()
        } else {
            memory.tags.iter().map(|t| format!("#{}", t)).collect::<Vec<_>>().join(" ")
        };

        println!("\n  {} {}", Self::slate("├─ Tags:       "), Self::gold(&tags_str));
        println!("  {} {}", Self::slate("├─ Confidence: "), Self::white_bold(&format!("{:.2}", memory.confidence)));
        println!("  {} {}", Self::slate("├─ Provenance: "), Self::slate(&memory.provenance));
        println!("  {} {}", Self::slate("├─ Created:    "), Self::slate(&memory.created_at.to_rfc3339()));
        if let Some(exp) = memory.expired_at {
            println!("  {} {}", Self::slate("╰─ Expired:    "), Self::gold(&exp.to_rfc3339()));
        } else {
            println!("  {} {}", Self::slate("╰─ Updated:    "), Self::slate(&memory.updated_at.to_rfc3339()));
        }

        if !links.is_empty() {
            println!("\n  {} Knowledge Graph Links ({}):", Self::crimson_bold("◫"), links.len());
            for link in links {
                let target_short = if link.target_id.len() >= 8 { &link.target_id[..8] } else { &link.target_id };
                println!(
                    "    ├─ ──{}──▶ ({}) {}",
                    Self::violet_bold(&link.relation),
                    Self::gold(target_short),
                    Self::slate(&format!("[weight: {:.1}]", link.weight))
                );
            }
        }
        println!();
        Ok(())
    }

    pub fn render_dedup(merged_count: usize, json: bool) -> Result<()> {
        if json {
            let obj = serde_json::json!({
                "merged": merged_count,
            });
            println!("{}", serde_json::to_string_pretty(&obj)?);
            return Ok(());
        }

        if merged_count == 0 {
            println!(
                "  {} No duplicate memories found. Vault is canonical.",
                Self::emerald_bold("✓")
            );
        } else {
            println!(
                "  {} Merged {} duplicate memories into canonical cards.",
                Self::crimson_bold("◈"),
                Self::white_bold(&merged_count.to_string())
            );
        }
        Ok(())
    }

    pub fn render_clear(removed: usize, hard: bool, db_path: &Path, json: bool) -> Result<()> {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "removed": removed,
                    "hard_deleted": hard,
                    "database": db_path.display().to_string(),
                })
            );
            return Ok(());
        }

        let db_str = Self::format_path(db_path);
        let action = if hard {
            "Permanently wiped"
        } else {
            "Soft-retired"
        };
        println!(
            "{} {} {} memories {} {}",
            Self::crimson_bold("◈"),
            Self::emerald_bold(action),
            Self::white_bold(&removed.to_string()),
            Self::slate("from"),
            Self::slate(&db_str)
        );
        Ok(())
    }

    pub fn render_export(exported_path: &Path) {
        println!(
            "{} Exported OKF bundle {} {}",
            Self::crimson_bold("⇄"),
            Self::slate("▸"),
            Self::white_bold(&exported_path.display().to_string())
        );
    }

    pub fn render_import(count: usize, file: &Path) {
        println!(
            "{} Imported {} memories from {}",
            Self::crimson_bold("⇄"),
            Self::crimson_bold(&count.to_string()),
            Self::white_bold(&file.display().to_string())
        );
    }

    pub fn render_answer(result: &AnswerResult, json: bool) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(result)?);
            return Ok(());
        }

        let conf_pct = (result.confidence * 100.0).round() as u32;
        println!(
            "{} {} {} {} {}%",
            Self::crimson_bold("✦"),
            Self::white_bold("Answer"),
            Self::slate(&format!("[reranker: {}]", result.reranker_used)),
            Self::confidence_bar(result.confidence),
            Self::rose(&conf_pct.to_string())
        );
        println!();
        println!(
            "  {} {}",
            Self::crimson_bold("▸"),
            Self::white_bold(&result.answer)
        );
        println!();

        if let Some(m) = &result.selected_memory {
            let short_id: String = m.id.chars().take(8).collect();
            println!(
                "  {} {} [{}] {} {}",
                Self::slate("╰─▸"),
                Self::slate("Source:"),
                Self::violet_bold(m.category.as_str()),
                Self::white_bold(&m.title),
                Self::gold(&format!("({})", short_id))
            );
        }
        Ok(())
    }

    pub fn render_config(cfg: &LightMemConfig, updated: bool) {
        if updated {
            println!(
                "{} {}",
                Self::crimson_bold("◈"),
                Self::emerald_bold("Configuration updated")
            );
            println!();
        }

        println!(
            "{} {}",
            Self::crimson_bold("❖"),
            Self::white_bold("LightMem Configuration")
        );
        println!(
            "  {} {:<16} {}",
            Self::slate("├─"),
            Self::slate("Embedder"),
            Self::emerald_bold(&cfg.active_embedding_summary())
        );
        println!(
            "  {} {:<16} {}",
            Self::slate("├─"),
            Self::slate("Reranker"),
            Self::crimson_bold(&cfg.active_reranker_summary())
        );
        println!(
            "  {} {:<16} {}",
            Self::slate("├─"),
            Self::slate("Ollama URL"),
            Self::slate(&cfg.ollama_url)
        );
        println!(
            "  {} {:<16} {}",
            Self::slate("╰─"),
            Self::slate("Config File"),
            Self::slate(&Self::format_path(&LightMemConfig::config_file()))
        );
    }

    pub fn render_stats(stats: &StorageStats, db_path: &Path, json: bool) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(stats)?);
            return Ok(());
        }

        println!("{}", Self::banner_string(Some(db_path), Some(stats)));
        println!();

        let db_str = Self::format_path(db_path);
        println!(
            "  {} {}",
            Self::crimson_bold("◫"),
            Self::white_bold("Vault Storage Telemetry")
        );
        println!(
            "  {} {:<14} {}",
            Self::slate("├─"),
            Self::slate("Database"),
            Self::white_bold(&db_str)
        );
        println!(
            "  {} {:<14} {} total  ({} active, {} expired)",
            Self::slate("├─"),
            Self::slate("Memories"),
            Self::white_bold(&stats.total_memories.to_string()),
            Self::emerald_bold(&stats.active_memories.to_string()),
            Self::gold(&stats.expired_memories.to_string())
        );
        println!(
            "  {} {:<14} {} embedded",
            Self::slate("╰─"),
            Self::slate("Vectors"),
            Self::violet_bold(&stats.total_vectors.to_string())
        );

        if !stats.by_category.is_empty() {
            println!();
            println!(
                "  {} {}",
                Self::crimson_bold("≡"),
                Self::white_bold("Category Distribution")
            );
            let max_count = stats
                .by_category
                .iter()
                .map(|(_, c)| *c)
                .max()
                .unwrap_or(1)
                .max(1);
            for (idx, (cat, count)) in stats.by_category.iter().enumerate() {
                let branch = if idx + 1 == stats.by_category.len() {
                    "╰─"
                } else {
                    "├─"
                };
                let bar_len = ((*count as f32 / max_count as f32) * 16.0).round() as usize;
                let bar = "▰".repeat(bar_len.max(1)).truecolor(220, 38, 38);
                println!(
                    "  {} {:<14} {:>4}  {}",
                    Self::slate(branch),
                    Self::violet_bold(cat),
                    Self::white_bold(&count.to_string()),
                    bar
                );
            }
        }
        Ok(())
    }

    pub fn render_link(
        src: &str,
        dst: &str,
        relation: &str,
        weight: f32,
        json: bool,
    ) -> Result<()> {
        if json {
            let obj = serde_json::json!({
                "source": src,
                "target": dst,
                "relation": relation,
                "weight": weight,
                "status": "linked"
            });
            println!("{}", serde_json::to_string_pretty(&obj)?);
            return Ok(());
        }

        let short_src = if src.len() >= 8 { &src[..8] } else { src };
        let short_dst = if dst.len() >= 8 { &dst[..8] } else { dst };

        println!(
            "{} {} ({}) ──{}──▶ ({}) {}",
            Self::crimson_bold("◈"),
            Self::emerald_bold("Linked"),
            Self::gold(short_src),
            Self::violet_bold(relation),
            Self::gold(short_dst),
            Self::slate(&format!("[weight: {:.1}]", weight))
        );
        Ok(())
    }

    pub fn render_unlink(
        src: &str,
        dst: &str,
        relation: Option<&str>,
        removed: bool,
        json: bool,
    ) -> Result<()> {
        if json {
            let obj = serde_json::json!({
                "source": src,
                "target": dst,
                "relation": relation,
                "removed": removed
            });
            println!("{}", serde_json::to_string_pretty(&obj)?);
            return Ok(());
        }

        let short_src = if src.len() >= 8 { &src[..8] } else { src };
        let short_dst = if dst.len() >= 8 { &dst[..8] } else { dst };

        if removed {
            println!(
                "{} {} connection between ({}) and ({})",
                Self::crimson_bold("◈"),
                Self::rose("Removed"),
                Self::gold(short_src),
                Self::gold(short_dst)
            );
        } else {
            println!(
                "{} No matching link found between ({}) and ({})",
                Self::crimson_bold("◈"),
                Self::gold(short_src),
                Self::gold(short_dst)
            );
        }
        Ok(())
    }

    pub fn render_related(
        start_term: &str,
        results: &[RelatedMemory],
        json: bool,
    ) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(results)?);
            return Ok(());
        }

        println!(
            "\n  {} {} for '{}'",
            Self::crimson_bold("❖"),
            Self::white_bold("MULTI-HOP RELATED MEMORIES"),
            Self::gold(start_term)
        );
        println!(
            "  {}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
        );

        if results.is_empty() {
            println!(
                "  {}",
                Self::slate("No connected memories found within specified hops.")
            );
            println!();
            return Ok(());
        }

        println!(
            "  {} {} connected memories found\n",
            Self::crimson_bold("◈"),
            results.len()
        );

        for r in results {
            let short_id: String = r.memory.id.chars().take(8).collect();
            let path_str = r.relation_path.join(" ──▶ ");
            let snippet = if r.memory.content.chars().count() > 100 {
                format!("{}...", r.memory.content.chars().take(100).collect::<String>())
            } else {
                r.memory.content.clone()
            };

            println!(
                "  {} [{}] {} [{}] {} {}",
                Self::crimson_bold("◈"),
                Self::gold(&format!("Hop {}", r.distance)),
                Self::violet_bold(r.memory.category.as_str()),
                Self::white_bold(&r.memory.title),
                Self::gold(&format!("({})", short_id)),
                Self::slate(&format!("[score: {:.2}]", r.score))
            );
            println!(
                "    {} Path: {}",
                Self::slate("├─"),
                Self::violet_bold(&path_str)
            );
            println!(
                "    {} {}\n",
                Self::slate("╰─"),
                Self::slate(&snippet)
            );
        }

        Ok(())
    }

    pub fn render_graph(
        snapshot: &GraphSnapshot,
        json: bool,
    ) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(snapshot)?);
            return Ok(());
        }

        print!("{}", crate::services::render_terminal(snapshot));
        Ok(())
    }

    pub fn render_autolink(
        new_links: usize,
        total_nodes: usize,
        total_edges: usize,
        json: bool,
    ) -> Result<()> {
        if json {
            let obj = serde_json::json!({
                "new_links": new_links,
                "total_nodes": total_nodes,
                "total_edges": total_edges,
            });
            println!("{}", serde_json::to_string_pretty(&obj)?);
            return Ok(());
        }

        println!(
            "\n  {} {}",
            Self::crimson_bold("❖"),
            Self::white_bold("AUTOLINK VAULT KNOWLEDGE GRAPH")
        );
        println!(
            "  {}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
        );
        if new_links > 0 {
            println!(
                "  {} Created {} new knowledge connections across {} memories",
                Self::crimson_bold("✦"),
                Self::emerald_bold(&new_links.to_string()),
                Self::white_bold(&total_nodes.to_string())
            );
        } else {
            println!(
                "  {} Vault is already fully connected (0 new links discovered)",
                Self::crimson_bold("✦")
            );
        }
        println!(
            "  {} Graph state: {} nodes · {} total links",
            Self::crimson_bold("◈"),
            Self::white_bold(&total_nodes.to_string()),
            Self::white_bold(&total_edges.to_string())
        );
        println!(
            "  {} Run {} to inspect the connection tree.\n",
            Self::slate("Tip:"),
            Self::gold("lmem graph")
        );
        Ok(())
    }

    pub fn render_connect(results: &[crate::services::ConnectResult], json: bool) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(results)?);
            return Ok(());
        }

        println!(
            "\n  {} {}",
            Self::crimson_bold("❖"),
            Self::white_bold("CONNECT LIGHTMEM AGENT SKILLS")
        );
        println!(
            "  {}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
        );

        if results.is_empty() {
            println!(
                "  {} No agent platforms were selected or detected.",
                Self::slate("▪")
            );
            println!(
                "  {} Run {} to see all supported platforms.",
                Self::slate("Tip:"),
                Self::gold("lmem connect --list")
            );
            println!();
            return Ok(());
        }

        println!(
            "  {} Deployed LightMem skill to {} platform(s):\n",
            Self::crimson_bold("✦"),
            Self::white_bold(&results.len().to_string())
        );

        for r in results {
            let path_str = Self::format_path(&r.target_file);
            let badge = if r.created {
                Self::emerald_bold("[new]")
            } else {
                Self::gold("[updated]")
            };
            println!(
                "  {} [{}] {} {} {}",
                Self::crimson_bold("◈"),
                Self::violet_bold(&r.platform),
                badge,
                Self::white_bold(&path_str),
                Self::slate(&format!("({})", r.message))
            );
        }

        println!();
        println!(
            "  {} Your AI agents can now automatically read and run {} commands!",
            Self::slate("Tip:"),
            Self::gold("lmem")
        );
        println!();
        Ok(())
    }

    pub fn render_platforms_list(
        platforms: &[crate::services::PlatformInfo],
        workspace: bool,
        json: bool,
    ) -> Result<()> {
        if json {
            println!("{}", serde_json::to_string_pretty(platforms)?);
            return Ok(());
        }

        let mode_str = if workspace { "Workspace (local)" } else { "Global (~/)" };
        println!(
            "\n  {} {} {}",
            Self::crimson_bold("❖"),
            Self::white_bold("SUPPORTED AGENT PLATFORMS"),
            Self::slate(&format!("({})", mode_str))
        );
        println!(
            "  {}",
            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
        );

        for p in platforms {
            let status = if p.detected {
                Self::emerald_bold("● detected")
            } else {
                Self::slate("○ not found")
            };
            let path_str = Self::format_path(&p.target_file);
            println!(
                "  {} {:<16} {:<14} {}",
                Self::crimson_bold("▪"),
                Self::white_bold(&p.id),
                status,
                Self::slate(&path_str)
            );
        }

        println!();
        println!(
            "  {} Run {} to install to a platform, or {} for all.",
            Self::slate("Usage:"),
            Self::gold("lmem connect <platform>"),
            Self::gold("lmem connect all")
        );
        println!();
        Ok(())
    }
}

