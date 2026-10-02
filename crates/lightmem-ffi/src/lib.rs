pub mod c_api;
pub mod types;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use lightmem_core::{LightMem as CoreLightMem, LightMemConfig};
use std::path::Path;
use std::sync::Arc;
pub use types::*;

uniffi::setup_scaffolding!("lightmem_ffi");

#[derive(uniffi::Object)]
pub struct LightMem {
    engine: CoreLightMem,
}

#[uniffi::export]
impl LightMem {
    #[uniffi::constructor]
    pub fn new(db_path: Option<String>, global_db: bool) -> Result<Arc<Self>, LightMemFfiError> {
        let engine = if let Some(p) = db_path {
            let config = LightMemConfig::load();
            CoreLightMem::open_at(Path::new(&p), config)?
        } else {
            CoreLightMem::open_default(global_db)?
        };

        Ok(Arc::new(Self { engine }))
    }

    pub fn db_path(&self) -> String {
        self.engine.db_path().display().to_string()
    }

    pub fn remember(
        &self,
        content: String,
        category: Option<String>,
        title: Option<String>,
        tags: Vec<String>,
        confidence: Option<f32>,
    ) -> Result<FfiMemoryRecord, LightMemFfiError> {
        let cat = if let Some(c) = category {
            Some(parse_memory_type(&c)?)
        } else {
            None
        };

        let record = self
            .engine
            .remember(&content, cat, title, tags, confidence)?;

        Ok(FfiMemoryRecord::from(&record))
    }

    pub fn recall(
        &self,
        query: String,
        category: Option<String>,
        as_of: Option<String>,
        limit: Option<u32>,
        min_similarity: Option<f32>,
    ) -> Result<Vec<FfiScoredMemory>, LightMemFfiError> {
        let cat = if let Some(c) = category {
            Some(parse_memory_type(&c)?)
        } else {
            None
        };

        let as_of_dt = if let Some(d) = as_of {
            Some(parse_date(&d)?)
        } else {
            None
        };

        let lim = limit.unwrap_or(10) as usize;
        let results = self
            .engine
            .recall(&query, cat, as_of_dt, lim, min_similarity)?;

        Ok(results.iter().map(FfiScoredMemory::from).collect())
    }

    pub fn answer(
        &self,
        question: String,
        needle: Option<bool>,
        category: Option<String>,
        as_of: Option<String>,
        limit: Option<u32>,
    ) -> Result<FfiAnswerResult, LightMemFfiError> {
        let cat = if let Some(c) = category {
            Some(parse_memory_type(&c)?)
        } else {
            None
        };

        let as_of_dt = if let Some(d) = as_of {
            Some(parse_date(&d)?)
        } else {
            None
        };

        let lim = limit.unwrap_or(5) as usize;
        let use_needle = needle.unwrap_or(false);

        let ans = self
            .engine
            .answer(&question, cat, as_of_dt, lim, use_needle)?;

        Ok(FfiAnswerResult {
            answer: ans.answer,
            selected_memory: ans.selected_memory.as_ref().map(FfiMemoryRecord::from),
            confidence: ans.confidence,
            reranker_used: ans.reranker_used,
        })
    }

    pub fn list(
        &self,
        category: Option<String>,
        status: Option<String>,
        as_of: Option<String>,
        limit: Option<u32>,
    ) -> Result<Vec<FfiMemoryRecord>, LightMemFfiError> {
        let cat = if let Some(c) = category {
            Some(parse_memory_type(&c)?)
        } else {
            None
        };

        let stat = if let Some(s) = status {
            Some(parse_memory_status(&s)?)
        } else {
            None
        };

        let as_of_dt = if let Some(d) = as_of {
            Some(parse_date(&d)?)
        } else {
            None
        };

        let lim = limit.unwrap_or(50) as usize;
        let records = self.engine.list(cat, stat, as_of_dt, lim)?;

        Ok(records.iter().map(FfiMemoryRecord::from).collect())
    }

    pub fn forget(&self, id: String, hard: bool) -> Result<bool, LightMemFfiError> {
        Ok(self.engine.forget(&id, hard)?)
    }

    pub fn stats(&self) -> Result<FfiStorageStats, LightMemFfiError> {
        let s = self.engine.stats()?;
        Ok(FfiStorageStats::from(&s))
    }

    pub fn export_okf(&self, output_path: Option<String>) -> Result<String, LightMemFfiError> {
        let p = output_path.as_deref().map(Path::new);
        let out = self.engine.export_okf(p)?;
        Ok(out.display().to_string())
    }

    pub fn import_file(&self, path: String) -> Result<u64, LightMemFfiError> {
        let count = self.engine.import_file(Path::new(&path))?;
        Ok(count as u64)
    }
}

fn parse_date(s: &str) -> Result<DateTime<Utc>, LightMemFfiError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        if let Some(naive_dt) = d.and_hms_opt(23, 59, 59) {
            return Ok(Utc.from_utc_datetime(&naive_dt));
        }
    }
    Err(LightMemFfiError::InvalidInput {
        msg: format!(
            "Invalid date format '{}'. Expected YYYY-MM-DD or RFC3339 timestamp",
            s
        ),
    })
}
