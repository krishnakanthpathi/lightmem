use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use lightmem::{CliView, EntityFilter, MemoryType, ResolvedConnection};
use std::path::Path;

use super::config::migrate_embeddings;
use super::context::{open_controller, AlreadyHandledError};
use crate::parse_tag_list;

#[allow(clippy::too_many_arguments)]
pub fn handle_answer(
    question: String,
    precision: bool,
    reranker: Option<String>,
    category: Option<MemoryType>,
    tag: Vec<String>,
    entity: Option<EntityFilter>,
    as_of: Option<DateTime<Utc>>,
    date: Option<NaiveDate>,
    limit: usize,
    session: Option<&str>,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let cat = category;
    let as_of_dt = as_of;
    let tags = parse_tag_list(&tag);

    if !migrate_embeddings(&lm, false, json, false)? {
        return Ok(());
    }
    let effective_reranker = if let Some(ref explicit_r) = reranker {
        Some(explicit_r.as_str())
    } else if precision {
        Some("onnx")
    } else {
        None
    };
    let result = match lm.answer_with_filters(
        &question,
        cat,
        as_of_dt,
        date,
        &tags,
        session,
        entity.as_ref(),
        limit,
        effective_reranker,
    ) {
        Ok(ans) => ans,
        Err(err) => {
            let msg = err.to_string();
            CliView::render_llm_or_error(&err, json);
            return Err(AlreadyHandledError::new(msg).into());
        }
    };
    CliView::render_answer(&result, json)?;
    Ok(())
}
