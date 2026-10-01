use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use lightmem_core::{
    LightMem as CoreLightMem, LightMemConfig, MemoryRecord as CoreRecord,
    MemoryStatus as CoreStatus, MemoryType as CoreType,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::path::Path;

#[pyclass(name = "MemoryRecord")]
#[derive(Clone)]
pub struct PyMemoryRecord {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub category: String,
    #[pyo3(get)]
    pub title: String,
    #[pyo3(get)]
    pub content: String,
    #[pyo3(get)]
    pub tags: Vec<String>,
    #[pyo3(get)]
    pub confidence: f32,
    #[pyo3(get)]
    pub status: String,
    #[pyo3(get)]
    pub provenance: String,
    #[pyo3(get)]
    pub created_at: String,
    #[pyo3(get)]
    pub updated_at: String,
    #[pyo3(get)]
    pub expired_at: Option<String>,
}

impl From<&CoreRecord> for PyMemoryRecord {
    fn from(r: &CoreRecord) -> Self {
        Self {
            id: r.id.clone(),
            category: r.category.as_str().to_string(),
            title: r.title.clone(),
            content: r.content.clone(),
            tags: r.tags.clone(),
            confidence: r.confidence,
            status: r.status.as_str().to_string(),
            provenance: r.provenance.clone(),
            created_at: r.created_at.to_rfc3339(),
            updated_at: r.updated_at.to_rfc3339(),
            expired_at: r.expired_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

#[pymethods]
impl PyMemoryRecord {
    fn __repr__(&self) -> String {
        format!(
            "<MemoryRecord id='{}' category='{}' title='{}'>",
            self.id, self.category, self.title
        )
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("id", &self.id)?;
        dict.set_item("category", &self.category)?;
        dict.set_item("title", &self.title)?;
        dict.set_item("content", &self.content)?;
        dict.set_item("tags", &self.tags)?;
        dict.set_item("confidence", self.confidence)?;
        dict.set_item("status", &self.status)?;
        dict.set_item("provenance", &self.provenance)?;
        dict.set_item("created_at", &self.created_at)?;
        dict.set_item("updated_at", &self.updated_at)?;
        dict.set_item("expired_at", &self.expired_at)?;
        Ok(dict)
    }
}

#[pyclass(name = "ScoredMemory")]
#[derive(Clone)]
pub struct PyScoredMemory {
    #[pyo3(get)]
    pub memory: PyMemoryRecord,
    #[pyo3(get)]
    pub score: f32,
}

#[pymethods]
impl PyScoredMemory {
    fn __repr__(&self) -> String {
        format!(
            "<ScoredMemory score={:.3} title='{}'>",
            self.score, self.memory.title
        )
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("memory", self.memory.to_dict(py)?)?;
        dict.set_item("score", self.score)?;
        Ok(dict)
    }
}

#[pyclass(name = "AnswerResult")]
#[derive(Clone)]
pub struct PyAnswerResult {
    #[pyo3(get)]
    pub answer: String,
    #[pyo3(get)]
    pub selected_memory: Option<PyMemoryRecord>,
    #[pyo3(get)]
    pub confidence: f32,
    #[pyo3(get)]
    pub reranker_used: String,
}

#[pymethods]
impl PyAnswerResult {
    fn __repr__(&self) -> String {
        format!(
            "<AnswerResult reranker='{}' confidence={:.2} answer='{}'>",
            self.reranker_used,
            self.confidence,
            if self.answer.len() > 40 {
                format!("{}...", &self.answer[..37])
            } else {
                self.answer.clone()
            }
        )
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("answer", &self.answer)?;
        dict.set_item("confidence", self.confidence)?;
        dict.set_item("reranker_used", &self.reranker_used)?;
        if let Some(mem) = &self.selected_memory {
            dict.set_item("selected_memory", mem.to_dict(py)?)?;
        } else {
            dict.set_item("selected_memory", py.None())?;
        }
        Ok(dict)
    }
}

#[pyclass(name = "StorageStats")]
#[derive(Clone)]
pub struct PyStorageStats {
    #[pyo3(get)]
    pub total_memories: usize,
    #[pyo3(get)]
    pub active_memories: usize,
    #[pyo3(get)]
    pub expired_memories: usize,
    #[pyo3(get)]
    pub total_vectors: usize,
    #[pyo3(get)]
    pub by_category: HashMap<String, usize>,
}

#[pymethods]
impl PyStorageStats {
    fn __repr__(&self) -> String {
        format!(
            "<StorageStats total={} active={} expired={}>",
            self.total_memories, self.active_memories, self.expired_memories
        )
    }

    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("total_memories", self.total_memories)?;
        dict.set_item("active_memories", self.active_memories)?;
        dict.set_item("expired_memories", self.expired_memories)?;
        dict.set_item("total_vectors", self.total_vectors)?;
        dict.set_item("by_category", &self.by_category)?;
        Ok(dict)
    }
}

fn parse_date(s: &str) -> PyResult<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| PyValueError::new_err(format!("Invalid date format '{}': {}", s, e)))?;
    let naive_dt = date.and_hms_opt(23, 59, 59).unwrap();
    Ok(Utc.from_utc_datetime(&naive_dt))
}

#[pyclass(name = "LightMem")]
pub struct PyLightMem {
    engine: CoreLightMem,
}

#[pymethods]
impl PyLightMem {
    #[new]
    #[pyo3(signature = (db_path=None, global_db=false))]
    fn new(db_path: Option<String>, global_db: bool) -> PyResult<Self> {
        let engine = if let Some(path_str) = db_path {
            let config = LightMemConfig::load();
            CoreLightMem::open_at(Path::new(&path_str), config)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
        } else {
            CoreLightMem::open_default(global_db).map_err(|e| PyValueError::new_err(e.to_string()))?
        };

        Ok(Self { engine })
    }

    #[getter]
    fn db_path(&self) -> String {
        self.engine.db_path().display().to_string()
    }

    #[pyo3(signature = (content, category=None, title=None, tags=None, confidence=None))]
    fn remember(
        &self,
        content: String,
        category: Option<String>,
        title: Option<String>,
        tags: Option<Vec<String>>,
        confidence: Option<f32>,
    ) -> PyResult<PyMemoryRecord> {
        let cat = if let Some(c) = category {
            Some(
                c.parse::<CoreType>()
                    .map_err(|e| PyValueError::new_err(e.to_string()))?,
            )
        } else {
            None
        };

        let tag_vec = tags.unwrap_or_default();
        let record = self
            .engine
            .remember(&content, cat, title, tag_vec, confidence)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;

        Ok(PyMemoryRecord::from(&record))
    }

    #[pyo3(signature = (query, category=None, as_of=None, limit=None, min_similarity=None))]
    fn recall(
        &self,
        query: String,
        category: Option<String>,
        as_of: Option<String>,
        limit: Option<usize>,
        min_similarity: Option<f32>,
    ) -> PyResult<Vec<PyScoredMemory>> {
        let cat = if let Some(c) = category {
            Some(
                c.parse::<CoreType>()
                    .map_err(|e| PyValueError::new_err(e.to_string()))?,
            )
        } else {
            None
        };

        let as_of_dt = if let Some(d) = as_of {
            Some(parse_date(&d)?)
        } else {
            None
        };

        let lim = limit.unwrap_or(10);
        let results = self
            .engine
            .recall(&query, cat, as_of_dt, lim, min_similarity)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|r| PyScoredMemory {
                memory: PyMemoryRecord::from(&r.memory),
                score: r.score,
            })
            .collect())
    }

    #[pyo3(signature = (question, needle=None, category=None, as_of=None, limit=None))]
    fn answer(
        &self,
        question: String,
        needle: Option<bool>,
        category: Option<String>,
        as_of: Option<String>,
        limit: Option<usize>,
    ) -> PyResult<PyAnswerResult> {
        let cat = if let Some(c) = category {
            Some(
                c.parse::<CoreType>()
                    .map_err(|e| PyValueError::new_err(e.to_string()))?,
            )
        } else {
            None
        };

        let as_of_dt = if let Some(d) = as_of {
            Some(parse_date(&d)?)
        } else {
            None
        };

        let lim = limit.unwrap_or(5);
        let use_needle = needle.unwrap_or(false);

        let ans = self
            .engine
            .answer(&question, cat, as_of_dt, lim, use_needle)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;

        Ok(PyAnswerResult {
            answer: ans.answer,
            selected_memory: ans.selected_memory.as_ref().map(PyMemoryRecord::from),
            confidence: ans.confidence,
            reranker_used: ans.reranker_used,
        })
    }

    #[pyo3(signature = (category=None, status=None, as_of=None, limit=None))]
    fn list(
        &self,
        category: Option<String>,
        status: Option<String>,
        as_of: Option<String>,
        limit: Option<usize>,
    ) -> PyResult<Vec<PyMemoryRecord>> {
        let cat = if let Some(c) = category {
            Some(
                c.parse::<CoreType>()
                    .map_err(|e| PyValueError::new_err(e.to_string()))?,
            )
        } else {
            None
        };

        let st = if let Some(s) = status {
            Some(
                s.parse::<CoreStatus>()
                    .map_err(|e| PyValueError::new_err(e.to_string()))?,
            )
        } else {
            None
        };

        let as_of_dt = if let Some(d) = as_of {
            Some(parse_date(&d)?)
        } else {
            None
        };

        let lim = limit.unwrap_or(20);
        let records = self
            .engine
            .list(cat, st, as_of_dt, lim)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;

        Ok(records.iter().map(PyMemoryRecord::from).collect())
    }

    #[pyo3(signature = (id, hard=false))]
    fn forget(&self, id: String, hard: bool) -> PyResult<bool> {
        self.engine
            .forget(&id, hard)
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }

    fn stats(&self) -> PyResult<PyStorageStats> {
        let s = self
            .engine
            .stats()
            .map_err(|e| PyValueError::new_err(e.to_string()))?;

        let by_cat_map: HashMap<String, usize> = s.by_category.into_iter().collect();

        Ok(PyStorageStats {
            total_memories: s.total_memories,
            active_memories: s.active_memories,
            expired_memories: s.expired_memories,
            total_vectors: s.total_vectors,
            by_category: by_cat_map,
        })
    }

    #[pyo3(signature = (output_path=None))]
    fn export_okf(&self, output_path: Option<String>) -> PyResult<String> {
        let path = output_path.as_deref().map(Path::new);
        let out = self
            .engine
            .export_okf(path)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(out.display().to_string())
    }

    fn import_file(&self, file_path: String) -> PyResult<usize> {
        self.engine
            .import_file(Path::new(&file_path))
            .map_err(|e| PyValueError::new_err(e.to_string()))
    }
}

#[pymodule]
fn lightmem(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyLightMem>()?;
    m.add_class::<PyMemoryRecord>()?;
    m.add_class::<PyScoredMemory>()?;
    m.add_class::<PyAnswerResult>()?;
    m.add_class::<PyStorageStats>()?;
    m.add("__version__", "0.1.0")?;
    Ok(())
}
