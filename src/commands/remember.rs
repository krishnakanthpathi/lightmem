use anyhow::Result;
use lightmem::{CliView, MemoryType, ResolvedConnection};
use std::io::{self, IsTerminal, Read};
use std::path::{Path, PathBuf};

use super::config::migrate_embeddings;
use super::context::open_controller_with_session;

#[allow(clippy::too_many_arguments)]
pub fn handle_remember(
    content: Option<String>,
    files: Vec<PathBuf>,
    extract: bool,
    reranker: Option<String>,
    category: Option<MemoryType>,
    title: Option<String>,
    tags: Option<String>,
    confidence: f32,
    ttl: Option<chrono::Duration>,
    supersede: bool,
    provenance: Option<String>,
    no_entities: bool,
    session: Option<&str>,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let resolved_content = if content.as_deref() == Some("-")
        || (content.is_none() && files.is_empty() && !io::stdin().is_terminal())
    {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        let trimmed = buf.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    } else {
        content
    };
    let has_text = resolved_content
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some();
    anyhow::ensure!(
        has_text || !files.is_empty(),
        "Provide memory content, pipe via stdin ('-'), or attach at least one file via --file / -f"
    );
    let lm = open_controller_with_session(effective_db, global, connection, session)?;
    let tag_vec = tags
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    if !migrate_embeddings(&lm, false, json, false)? {
        return Ok(());
    }
    let (memory, conflicts, entity_warning) = lm.remember_with_attachments_and_entities(
        resolved_content.as_deref(),
        &files,
        extract,
        !no_entities,
        reranker.as_deref(),
        category,
        title,
        tag_vec,
        Some(confidence),
        ttl,
        supersede,
        provenance,
    )?;
    CliView::render_remembered_full(
        &memory,
        &conflicts,
        supersede,
        lm.db_path(),
        json,
        entity_warning.as_deref(),
    )?;
    Ok(())
}
