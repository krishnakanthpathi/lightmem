use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use lightmem::{
    AttachmentInput, CliView, EntityFilter, HistoryService, IdentityRegistry, LightMemConfig,
    ListSort, MemoryStatus, MemoryType, ResolvedConnection,
};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use super::config::migrate_embeddings;
use super::context::{
    open_controller, open_files_in_default_app, remove_db_files, AlreadyHandledError,
};
use crate::parse_tag_list;

pub fn handle_status(json: bool, default_db: PathBuf) -> Result<()> {
    let registry = IdentityRegistry::open_default()?;
    let overview = registry.status_overview(default_db)?;
    CliView::render_vault_status_overview(&overview, json)
}

pub fn handle_context(
    connection: Option<&ResolvedConnection>,
    effective_db: Option<&Path>,
    global: bool,
    json: bool,
) -> Result<()> {
    let value = match connection {
        Some(connection) => serde_json::to_value(connection)?,
        None => {
            serde_json::json!({
                "connection_id": null,
                "db_path": effective_db
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| LightMemConfig::resolve_db_path(global))
            })
        }
    };
    CliView::render_context(&value, json)
}

#[allow(clippy::too_many_arguments)]
pub fn handle_list(
    category: Option<MemoryType>,
    tag: Vec<String>,
    entity: Option<EntityFilter>,
    media: Option<String>,
    status: Option<MemoryStatus>,
    sort: ListSort,
    as_of: Option<DateTime<Utc>>,
    date: Option<NaiveDate>,
    limit: usize,
    offset: usize,
    page: Option<usize>,
    save_to: Option<PathBuf>,
    path: bool,
    open: bool,
    session: Option<&str>,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let cat = category;
    let st = status;
    let as_of_dt = as_of;
    let tags = parse_tag_list(&tag);

    let effective_offset = if let Some(p) = page {
        anyhow::ensure!(p > 0 && limit > 0, "Page and limit must be positive");
        (p - 1).checked_mul(limit).context("Pagination overflow")?
    } else {
        offset
    };
    let effective_media = media.as_deref().or_else(|| {
        if path || save_to.is_some() || open {
            Some("any")
        } else {
            None
        }
    });
    let paginated = lm.list_filtered_full(
        cat,
        st,
        as_of_dt,
        date,
        effective_media,
        &tags,
        session,
        entity.as_ref(),
        sort,
        limit,
        effective_offset,
    )?;

    let ids: Vec<String> = paginated.items.iter().map(|m| m.id.clone()).collect();
    if path {
        let all_paths = lm.materialize_memories_attachments(&ids, save_to.as_deref())?;
        if open {
            open_files_in_default_app(&all_paths);
        }
        if json {
            println!("{}", serde_json::to_string_pretty(&all_paths)?);
        } else {
            for p in all_paths {
                println!("{}", p.display());
            }
        }
    } else if let Some(ref target) = save_to {
        let all_saved = lm.save_memories_attachments_to(&ids, target)?;
        if open {
            open_files_in_default_app(&all_saved);
        }
        if json {
            let mut val = serde_json::to_value(&paginated)?;
            if let Some(obj) = val.as_object_mut() {
                obj.insert(
                    "saved_attachments".to_string(),
                    serde_json::json!(&all_saved),
                );
                if let Some(items_arr) = obj.get_mut("items").and_then(|v| v.as_array_mut()) {
                    let mut offset_idx = 0usize;
                    for (item_val, m) in items_arr.iter_mut().zip(paginated.items.iter()) {
                        let count = m.attachments.len();
                        let end = (offset_idx + count).min(all_saved.len());
                        let saved_slice = &all_saved[offset_idx..end];
                        offset_idx = end;
                        if let Some(item_obj) = item_val.as_object_mut() {
                            item_obj.insert(
                                "saved_attachments".to_string(),
                                serde_json::json!(saved_slice),
                            );
                        }
                    }
                }
            }
            println!("{}", serde_json::to_string_pretty(&val)?);
        } else {
            CliView::render_paginated_list(&paginated, lm.db_path(), false)?;
            if !all_saved.is_empty() {
                println!();
                for p in &all_saved {
                    println!(
                        "  {} {} {}",
                        CliView::emerald_bold("✓ Saved verified attachment to"),
                        CliView::white_bold(&p.display().to_string()),
                        CliView::slate("(SHA-256 verified)")
                    );
                }
            }
        }
    } else {
        if open {
            let all_paths = lm.materialize_memories_attachments(&ids, None)?;
            open_files_in_default_app(&all_paths);
        }
        CliView::render_paginated_list(&paginated, lm.db_path(), json)?;
    }
    Ok(())
}

pub fn handle_inspect(
    terms: Vec<String>,
    save_to: Option<PathBuf>,
    path: bool,
    open: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    if path {
        let mut ids = Vec::new();
        for term in &terms {
            let Some(record) = lm.get(term)? else {
                continue;
            };
            ids.push(record.id);
        }
        if ids.is_empty() {
            if json {
                println!("[]");
            } else {
                eprintln!("Memory not found: {}", terms.join(", "));
            }
            return Err(AlreadyHandledError::new(format!(
                "Memory not found: {}",
                terms.join(", ")
            ))
            .into());
        }
        let all_paths = lm.materialize_memories_attachments(&ids, save_to.as_deref())?;
        if open {
            open_files_in_default_app(&all_paths);
        }
        if all_paths.is_empty() {
            if json {
                println!("[]");
            } else {
                eprintln!("No attachments found for: {}", terms.join(", "));
            }
            return Err(AlreadyHandledError::new(format!(
                "No attachments found for: {}",
                terms.join(", ")
            ))
            .into());
        }
        if json {
            println!("{}", serde_json::to_string_pretty(&all_paths)?);
        } else {
            for p in all_paths {
                println!("{}", p.display());
            }
        }
    } else if terms.len() == 1 {
        let term = &terms[0];
        if let Some(record) = lm.get(term)? {
            let links = lm.get_links(&record.id).unwrap_or_default();
            let saved_paths = if let Some(ref target) = save_to {
                lm.save_attachments_to(&record.id, target)?
            } else {
                Vec::new()
            };
            if open {
                if save_to.is_some() {
                    open_files_in_default_app(&saved_paths);
                } else {
                    let all_paths = lm
                        .materialize_memories_attachments(std::slice::from_ref(&record.id), None)?;
                    open_files_in_default_app(&all_paths);
                }
            }
            if json && save_to.is_some() {
                let obj = serde_json::json!({
                    "memory": record,
                    "links": links,
                    "saved_attachments": saved_paths,
                });
                println!("{}", serde_json::to_string_pretty(&obj)?);
            } else {
                CliView::render_inspect(&record, &links, lm.db_path(), json)?;
                if !saved_paths.is_empty() && !json {
                    for p in &saved_paths {
                        println!(
                            "  {} {} {}",
                            CliView::emerald_bold("✓ Saved verified attachment to"),
                            CliView::white_bold(&p.display().to_string()),
                            CliView::slate("(SHA-256 verified)")
                        );
                    }
                    println!();
                }
            }
        } else {
            if json {
                println!("null");
            } else {
                eprintln!("Memory not found: {}", term);
            }
            return Err(AlreadyHandledError::new(format!("Memory not found: {}", term)).into());
        }
    } else {
        let mut records = Vec::new();
        for term in &terms {
            if let Some(record) = lm.get(term)? {
                records.push(record);
            }
        }
        if records.is_empty() {
            if json {
                println!("[]");
            } else {
                eprintln!("Memory not found: {}", terms.join(", "));
            }
            return Err(AlreadyHandledError::new(format!(
                "Memory not found: {}",
                terms.join(", ")
            ))
            .into());
        }
        let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let all_saved = if let Some(ref target) = save_to {
            lm.save_memories_attachments_to(&ids, target)?
        } else {
            Vec::new()
        };
        if open {
            if save_to.is_some() {
                open_files_in_default_app(&all_saved);
            } else {
                let all_paths = lm.materialize_memories_attachments(&ids, None)?;
                open_files_in_default_app(&all_paths);
            }
        }
        let mut offset_idx = 0usize;
        let mut json_items = Vec::new();
        for record in records {
            let links = lm.get_links(&record.id).unwrap_or_default();
            let saved_paths = if save_to.is_some() {
                let count = record.attachments.len();
                let end = (offset_idx + count).min(all_saved.len());
                let slice = all_saved[offset_idx..end].to_vec();
                offset_idx = end;
                slice
            } else {
                Vec::new()
            };
            if json {
                if save_to.is_some() {
                    json_items.push(serde_json::json!({
                        "memory": record,
                        "links": links,
                        "saved_attachments": saved_paths,
                    }));
                } else {
                    json_items.push(serde_json::json!({
                        "memory": record,
                        "links": links,
                    }));
                }
            } else {
                CliView::render_inspect(&record, &links, lm.db_path(), false)?;
                if !saved_paths.is_empty() {
                    for p in &saved_paths {
                        println!(
                            "  {} {} {}",
                            CliView::emerald_bold("✓ Saved verified attachment to"),
                            CliView::white_bold(&p.display().to_string()),
                            CliView::slate("(SHA-256 verified)")
                        );
                    }
                    println!();
                }
            }
        }
        if json {
            println!("{}", serde_json::to_string_pretty(&json_items)?);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn handle_update(
    id: String,
    content: Option<String>,
    title: Option<String>,
    category: Option<MemoryType>,
    tags: Option<String>,
    add_tags: Vec<String>,
    remove_tags: Vec<String>,
    confidence: Option<f32>,
    ttl: Option<chrono::Duration>,
    clear_ttl: bool,
    provenance: Option<String>,
    files: Vec<PathBuf>,
    refresh_entities: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    if !migrate_embeddings(&lm, false, json, false)? {
        return Ok(());
    }
    let parsed_tags = tags.map(|t| parse_tag_list(&[t]));
    let parsed_add_tags = parse_tag_list(&add_tags);
    let parsed_remove_tags = parse_tag_list(&remove_tags);
    let new_expired_at = if clear_ttl {
        Some(None)
    } else {
        ttl.map(|d| Some(Utc::now() + d))
    };
    let mut new_attachments = Vec::with_capacity(files.len());
    for p in &files {
        new_attachments.push(AttachmentInput::from_path(p)?);
    }
    let updated = lm.update_memory_full(
        &id,
        content.as_deref(),
        title.as_deref(),
        category,
        parsed_tags,
        parsed_add_tags,
        parsed_remove_tags,
        confidence,
        provenance.as_deref(),
        new_expired_at,
        new_attachments,
        refresh_entities,
    )?;
    let links = lm.get_links(&updated.id)?;
    if !json {
        println!(
            "\n  {} {}",
            CliView::emerald_bold("✓"),
            CliView::emerald_bold("Memory updated in-place")
        );
    }
    CliView::render_inspect(&updated, &links, lm.db_path(), json)
}

pub fn handle_dedup(
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let merged = lm.deduplicate()?;
    CliView::render_dedup(merged, json)
}

#[allow(clippy::too_many_arguments)]
pub fn handle_forget(
    ids: Vec<String>,
    all: bool,
    hard: bool,
    yes: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    if all {
        let db_path = lm.db_path().to_path_buf();
        if !yes {
            if !json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
                let action = if hard { "permanently delete" } else { "expire" };
                eprint!(
                    "Are you sure you want to {} all memories in vault '{}'? [y/N]: ",
                    action,
                    db_path.display()
                );
                std::io::stderr().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                    eprintln!("Operation cancelled.");
                    return Ok(());
                }
            } else {
                anyhow::bail!(
                    "Refusing to forget all memories without confirmation. Pass --yes (or -y) to proceed non-interactively."
                );
            }
        }
        let removed = lm.clear_all(hard)?;
        drop(lm);
        if hard {
            remove_db_files(&db_path);
        }
        CliView::render_clear(removed, hard, &db_path, json)?;
    } else if ids.len() == 1 {
        let target_id = &ids[0];
        let ok = lm.forget(target_id, hard)?;
        CliView::render_forget(target_id, ok, hard, json)?;
        if !ok {
            return Err(
                AlreadyHandledError::new(format!("Memory '{}' not found", target_id)).into(),
            );
        }
    } else if ids.len() > 1 {
        let mut results = Vec::with_capacity(ids.len());
        let mut all_ok = true;
        for target_id in &ids {
            let ok = lm.forget(target_id, hard)?;
            if !ok {
                all_ok = false;
            }
            if json {
                results.push(serde_json::json!({
                    "id": target_id,
                    "success": ok,
                    "hard_deleted": hard,
                }));
            } else {
                CliView::render_forget(target_id, ok, hard, false)?;
            }
        }
        if json {
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        if !all_ok {
            return Err(AlreadyHandledError::new("One or more memories were not found").into());
        }
    } else {
        anyhow::bail!("Provide a memory <ID> or pass '--all' to forget all memories.");
    }
    Ok(())
}

pub fn handle_restore(
    term: String,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let memory = lm.restore_memory(&term)?;
    if !json {
        println!("Restored memory in {}", lm.db_path().display());
    }
    let links = lm.get_links(&memory.id)?;
    CliView::render_inspect(&memory, &links, lm.db_path(), json)
}

pub fn handle_clear(
    soft: bool,
    yes: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let hard = !soft;
    let lm = open_controller(effective_db, global, connection)?;
    let db_path = lm.db_path().to_path_buf();
    if !yes {
        if !json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
            let action = if hard {
                "permanently delete database"
            } else {
                "soft-retire all memories in"
            };
            eprint!(
                "Are you sure you want to {} vault '{}'? [y/N]: ",
                action,
                db_path.display()
            );
            std::io::stderr().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                eprintln!("Operation cancelled.");
                return Ok(());
            }
        } else {
            anyhow::bail!(
                "Refusing to clear vault without confirmation. Pass --yes (or -y) to proceed non-interactively."
            );
        }
    }
    let removed = lm.clear_all(hard)?;
    drop(lm);
    if hard {
        remove_db_files(&db_path);
    }
    CliView::render_clear(removed, hard, &db_path, json)
}

pub fn handle_stats(
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let stats = lm.stats()?;
    CliView::render_stats(&stats, lm.db_path(), json)
}

pub fn handle_entities(
    name: Option<String>,
    val: Option<String>,
    memories: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let (parsed_name, parsed_val) = match (name, val) {
        (Some(raw), explicit_val) => {
            if let Some(v) = explicit_val {
                (Some(raw), Some(v))
            } else if let Some((k, v)) = raw.split_once('=') {
                (Some(k.trim().to_string()), Some(v.trim().to_string()))
            } else {
                (Some(raw), None)
            }
        }
        (None, Some(v)) => {
            anyhow::bail!("--val '{}' requires an entity name argument", v);
        }
        (None, None) => (None, None),
    };
    if parsed_val.is_some() || memories {
        let target_name = parsed_name.as_deref().context(
            "Provide an entity name when using --memories or --val (e.g. 'lmem entities role --memories')",
        )?;
        let records = lm.find_by_entity(target_name, parsed_val.as_deref())?;
        let total = records.len();
        let paginated = lightmem::PaginatedMemories {
            items: records,
            total,
            limit: total.max(1),
            offset: 0,
            page: 1,
            total_pages: 1,
            has_more: false,
        };
        CliView::render_paginated_list(&paginated, lm.db_path(), json)?;
    } else {
        let list = lm.list_entities(parsed_name.as_deref())?;
        CliView::render_entities(&list, json)?;
    }
    Ok(())
}

pub fn handle_history(
    query: Option<String>,
    limit: usize,
    status: Option<String>,
    clear: bool,
    json: bool,
) -> Result<()> {
    if clear {
        let count = HistoryService::clear()?;
        CliView::render_history_cleared(count, json)?;
    } else {
        let (entries, total) = HistoryService::list(query.as_deref(), status.as_deref(), limit)?;
        CliView::render_history(&entries, total, &HistoryService::db_path(), json)?;
    }
    Ok(())
}
