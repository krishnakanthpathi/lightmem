use anyhow::{Context, Result};
use colored::*;
use lightmem::{CliView, MemoryType, ResolvedConnection};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use super::config::migrate_embeddings;
use super::context::open_controller_with_session;

#[allow(clippy::too_many_arguments)]
pub fn handle_observe(
    input: Option<String>,
    file: Option<PathBuf>,
    yes: bool,
    dry_run: bool,
    category: Option<MemoryType>,
    tags: Option<String>,
    confidence: f32,
    reranker: Option<String>,
    session: Option<&str>,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller_with_session(effective_db, global, connection, session)?;
    if !dry_run && !migrate_embeddings(&lm, false, json, false)? {
        return Ok(());
    }

    let raw_text = if let Some(ref path) = file {
        std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read conversation file: {}", path.display()))?
    } else if let Some(ref arg) = input {
        if arg == "-" {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            buf
        } else if Path::new(arg).is_file() {
            std::fs::read_to_string(arg).with_context(|| format!("Failed to read file: {}", arg))?
        } else {
            arg.clone()
        }
    } else if !io::stdin().is_terminal() {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        anyhow::bail!(
            "Provide conversation text or a file path (or pipe via stdin): `lmem observe \"User: I prefer dark mode\"`"
        );
    };

    let parsed_tags: Option<Vec<String>> = tags.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });

    let candidates = lm.observe_and_extract(
        &raw_text,
        category,
        parsed_tags.as_deref(),
        Some(confidence),
        reranker.as_deref(),
    )?;

    if candidates.is_empty() {
        CliView::render_observed_candidates(&[], dry_run, json)?;
        return Ok(());
    }

    if dry_run {
        CliView::render_observed_candidates(&candidates, true, json)?;
        return Ok(());
    }

    if yes {
        let saved_records = lm.save_observed_candidates(&candidates)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&saved_records)?);
        } else {
            CliView::render_observed_candidates(&candidates, false, false)?;
            CliView::render_observed_summary(saved_records.len(), 0, false, false)?;
        }
        return Ok(());
    }

    if json || !io::stdin().is_terminal() {
        CliView::render_observed_candidates(&candidates, true, json)?;
        return Ok(());
    }

    println!(
        "\n  {} {} ({} candidates discovered)",
        CliView::crimson_bold("❖"),
        CliView::white_bold("OBSERVED CONVERSATIONAL MEMORIES"),
        candidates.len()
    );
    println!(
        "  {}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".bright_black()
    );
    println!("  Reviewing candidate memories one by one:\n");

    let mut saved_count = 0usize;
    let mut skipped_count = 0usize;
    let mut force_all = false;
    let mut quit_requested = false;

    for (i, c) in candidates.iter().enumerate() {
        CliView::render_observed_card(i, candidates.len(), c);

        let should_save = if force_all {
            true
        } else {
            loop {
                eprint!("  Save this memory? [y]es / [n]o (skip) / [a]ll (--yes) / [q]uit: ");
                io::stderr().flush()?;
                let mut answer = String::new();
                if io::stdin().read_line(&mut answer)? == 0 {
                    quit_requested = true;
                    break false;
                }
                match answer.trim().to_lowercase().as_str() {
                    "y" | "yes" => break true,
                    "" | "n" | "no" => break false,
                    "a" | "all" => {
                        force_all = true;
                        break true;
                    }
                    "q" | "quit" => {
                        quit_requested = true;
                        break false;
                    }
                    _ => eprintln!("  Enter y, n, a, or q."),
                }
            }
        };
        if quit_requested {
            println!(
                "  Stopped candidate review ({} saved, {} skipped).",
                saved_count, skipped_count
            );
            break;
        }

        if should_save {
            let record = lm.save_observed_candidate(c)?;
            saved_count += 1;
            println!(
                "  {} Saved memory {} ({}): \"{}\"\n",
                CliView::emerald_bold("✓"),
                CliView::gold(&record.id.chars().take(8).collect::<String>()),
                CliView::violet_bold(record.category.as_str()),
                record.title
            );
        } else {
            skipped_count += 1;
            println!("  {} Skipped.\n", CliView::slate("↷"));
        }
    }

    CliView::render_observed_summary(saved_count, skipped_count, false, false)
}
