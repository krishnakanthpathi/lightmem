use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use lightmem::{
    encode_base64, CliView, EntityFilter, MemoryType, MultimodalInput, ResolvedConnection,
};
use std::path::{Path, PathBuf};

use super::config::migrate_embeddings;
use super::context::{open_controller, open_files_in_default_app, AlreadyHandledError};
use crate::parse_tag_list;

#[allow(clippy::too_many_arguments)]
pub fn handle_recall(
    query: String,
    image: Option<PathBuf>,
    category: Option<MemoryType>,
    tag: Vec<String>,
    entity: Option<EntityFilter>,
    media: Option<String>,
    as_of: Option<DateTime<Utc>>,
    date: Option<NaiveDate>,
    limit: usize,
    min_similarity: Option<f32>,
    precision: bool,
    multi_hop: bool,
    save_to: Option<PathBuf>,
    path: bool,
    open: bool,
    session: Option<&str>,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let tags = parse_tag_list(&tag);
    if precision {
        anyhow::ensure!(
            image.is_none()
                && save_to.is_none()
                && !path
                && !open
                && media.is_none()
                && !multi_hop
                && min_similarity.is_none(),
            "Precision recall returns an answer; image, media, attachment, multi-hop and similarity options are not supported together. Use recall without --precision for filtered results."
        );
    }
    let lm = open_controller(effective_db, global, connection)?;
    let cat = category;
    let as_of_dt = as_of;

    if !migrate_embeddings(&lm, false, json || path, false)? {
        return Ok(());
    }
    if precision && !path {
        anyhow::ensure!(
            !query.trim().is_empty(),
            "Search query cannot be blank with --precision"
        );
        let result = match lm.answer_with_filters(
            &query,
            cat,
            as_of_dt,
            date,
            &tags,
            session,
            entity.as_ref(),
            limit,
            None,
        ) {
            Ok(ans) => ans,
            Err(err) => {
                let msg = err.to_string();
                CliView::render_llm_or_error(&err, json);
                return Err(AlreadyHandledError::new(msg).into());
            }
        };
        CliView::render_answer(&result, json)?;
    } else {
        let filter_attachments = media.is_some() || path || save_to.is_some() || open;
        let fetch_limit = if filter_attachments {
            limit.max(100)
        } else {
            limit
        };
        let img_b64 = if let Some(ref img_path) = image {
            let data = std::fs::read(img_path).with_context(|| {
                format!("Failed to read query image file: {}", img_path.display())
            })?;
            anyhow::ensure!(
                !data.is_empty(),
                "Query image file is empty: {}",
                img_path.display()
            );
            Some(encode_base64(&data))
        } else {
            None
        };
        let mm_input = if let Some(ref b64) = img_b64 {
            MultimodalInput::with_image(&query, b64)
        } else {
            MultimodalInput::text_only(&query)
        };
        let mut results = lm.recall_multimodal_filtered(
            &mm_input,
            cat,
            as_of_dt,
            date,
            &tags,
            session,
            entity.as_ref(),
            fetch_limit,
            min_similarity,
            multi_hop,
        )?;
        if filter_attachments {
            let k = media.as_deref().unwrap_or("any").trim().to_lowercase();
            results.retain(|r| {
                !r.memory.attachments.is_empty()
                    && (k == "any"
                        || k == "all"
                        || r.memory.attachments.iter().any(|a| {
                            a.media_kind.eq_ignore_ascii_case(&k)
                                || a.mime_type.to_lowercase().contains(&k)
                        }))
            });
            results.truncate(limit);
        }
        let ids: Vec<String> = results.iter().map(|r| r.memory.id.clone()).collect();
        let display_query = if query.trim().is_empty() {
            image
                .as_ref()
                .map(|p| format!("[image: {}]", p.display()))
                .unwrap_or_default()
        } else {
            query.clone()
        };
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
                let mut offset = 0usize;
                let json_items: Vec<serde_json::Value> = results
                    .iter()
                    .map(|r| {
                        let count = r.memory.attachments.len();
                        let end = (offset + count).min(all_saved.len());
                        let saved_slice = &all_saved[offset..end];
                        offset = end;
                        let mut val = serde_json::to_value(r).unwrap_or_default();
                        if let Some(obj) = val.as_object_mut() {
                            obj.insert(
                                "saved_attachments".to_string(),
                                serde_json::json!(saved_slice),
                            );
                        }
                        val
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&json_items)?);
            } else {
                CliView::render_recall(&display_query, &results, lm.db_path(), false)?;
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
            CliView::render_recall(&display_query, &results, lm.db_path(), json)?;
        }
    }
    Ok(())
}
