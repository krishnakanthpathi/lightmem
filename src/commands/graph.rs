use anyhow::Result;
use lightmem::{CliView, ResolvedConnection};
use std::io::{self, IsTerminal, Write};
use std::path::Path;

use super::config::migrate_embeddings;
use super::context::open_controller;

pub fn handle_conflicts(
    yes: bool,
    reranker: Option<String>,
    min_similarity: f32,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    if !migrate_embeddings(&lm, false, json, false)? {
        return Ok(());
    }

    if yes || json || !io::stdin().is_terminal() {
        let conflicts = lm.find_conflicts_with_options(yes, min_similarity, reranker.as_deref())?;
        CliView::render_conflicts(&conflicts, yes, lm.db_path(), json)?;
    } else {
        let conflicts =
            lm.find_conflicts_with_options(false, min_similarity, reranker.as_deref())?;
        if conflicts.is_empty() {
            CliView::render_conflicts(&conflicts, false, lm.db_path(), false)?;
            return Ok(());
        }

        let mut retired_ids = std::collections::HashSet::new();
        let mut force_all = false;
        let mut merged_count = 0usize;

        for (i, c) in conflicts.iter().enumerate() {
            if retired_ids.contains(&c.older_memory.id) || retired_ids.contains(&c.newer_memory.id)
            {
                continue;
            }

            CliView::render_conflict_pair(i + 1, conflicts.len(), c, false);

            let should_merge = if force_all {
                true
            } else {
                print!("  Merge these memories? [y]es / [n]o (next) / [a]ll (--yes) / [q]uit: ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                match input.trim().to_lowercase().as_str() {
                    "y" | "yes" => true,
                    "a" | "all" => {
                        force_all = true;
                        true
                    }
                    "q" | "quit" => {
                        println!("  Stopped conflict review ({} merged).", merged_count);
                        break;
                    }
                    _ => false,
                }
            };

            if should_merge {
                let merged = lm.merge_conflict_pair(c, reranker.as_deref())?;
                retired_ids.insert(c.older_memory.id.clone());
                merged_count += 1;
                println!(
                    "  ✓ Merged into {} (retired older {}): {}\n",
                    merged.id.chars().take(8).collect::<String>(),
                    c.older_memory.id.chars().take(8).collect::<String>(),
                    merged.content
                );
            } else {
                println!("  ↷ Skipped (moving to next).\n");
            }
        }

        println!("✦ Conflict check complete: {} merged.", merged_count);
    }
    Ok(())
}

pub fn handle_link(
    source: String,
    target: String,
    relation: String,
    weight: f32,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let (src_id, dst_id) = lm.link(&source, &target, Some(&relation), Some(weight))?;
    CliView::render_link(&src_id, &dst_id, &relation, weight, json)
}

pub fn handle_unlink(
    source: String,
    target: String,
    relation: Option<String>,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let removed = lm.unlink(&source, &target, relation.as_deref())?;
    CliView::render_unlink(&source, &target, relation.as_deref(), removed, json)
}

pub fn handle_links(
    id: String,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let links = lm.get_links(&id)?;
    let resolved_term = lm.get(&id)?.map(|m| m.id).unwrap_or(id);
    CliView::render_links_list(&resolved_term, &links, json)
}

pub fn handle_tags(
    query: String,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let mut all_tags = lm.get_all_tags()?;
    if !query.trim().is_empty() {
        let q_lower = query.trim().to_lowercase();
        all_tags.retain(|(t, _)| t.contains(&q_lower));
    }
    CliView::render_tags(&all_tags, json)
}

pub fn handle_related(
    id: String,
    hops: usize,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let related = lm.related(&id, hops)?;
    CliView::render_related(&id, &related, json)
}

pub fn handle_graph(
    focus: Option<String>,
    hops: usize,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let snapshot = lm.graph(focus.as_deref(), Some(hops))?;
    CliView::render_graph(&snapshot, json)
}

pub fn handle_autolink(
    min_similarity: f32,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let new_links = lm.autolink(Some(min_similarity))?;
    let snapshot = lm.graph(None, None)?;
    CliView::render_autolink(new_links, snapshot.nodes.len(), snapshot.edges.len(), json)
}
