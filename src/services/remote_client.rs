use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use std::time::Duration;
use crate::models::{
    MemoryRecord, MemoryStatus, MemoryType, PaginatedMemories, ScoredMemory, StorageStats,
};
use crate::models::link::{GraphSnapshot, RelatedMemory};
use crate::services::api_server::{
    AnswerRequest, ClearPayload, ForgetPayload, LinkRequest, RecallRequest,
    RememberRequest, UnlinkRequest,
};
use crate::services::reranker::AnswerResult;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkResult {
    pub source_id: String,
    pub target_id: String,
    pub relation: String,
    pub weight: f32,
}

#[derive(Debug, Clone)]
pub struct RemoteClient {
    pub base_url: String,
    pub api_key: Option<String>,
    timeout: Duration,
}

impl RemoteClient {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let mut base_url = base_url.into().trim().to_string();
        while base_url.ends_with('/') {
            base_url.pop();
        }
        Self {
            base_url,
            api_key: api_key.filter(|k| !k.trim().is_empty()),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn apply_auth(&self, req: ureq::Request) -> ureq::Request {
        let req = req.timeout(self.timeout);
        if let Some(ref key) = self.api_key {
            req.set(
                "Authorization",
                &crate::models::config::format_auth_header(key),
            )
        } else {
            req
        }
    }

    fn map_ureq_error(&self, e: ureq::Error) -> anyhow::Error {
        match e {
            ureq::Error::Status(401, resp) => {
                let body = resp.into_string().unwrap_or_default();
                anyhow::anyhow!(
                    "Authentication failed (401 Unauthorized) at remote LightMem backend {}. {}\nPlease verify your API key.",
                    self.base_url,
                    body
                )
            }
            ureq::Error::Status(code, resp) => {
                let body = resp.into_string().unwrap_or_else(|_| format!("HTTP {}", code));
                anyhow::anyhow!("Remote server error ({}) at {}: {}", code, self.base_url, body)
            }
            ureq::Error::Transport(t) => {
                anyhow::anyhow!(
                    "Failed to connect to remote LightMem backend at {}: {}",
                    self.base_url,
                    t
                )
            }
        }
    }

    pub fn health(&self) -> Result<serde_json::Value> {
        let url = format!("{}/v1/health", self.base_url);
        let resp = self
            .apply_auth(ureq::get(&url))
            .call()
            .map_err(|e| self.map_ureq_error(e))?;
        let json: serde_json::Value = resp
            .into_json()
            .context("Failed to parse health response as JSON")?;
        Ok(json)
    }

    pub fn stats(&self) -> Result<StorageStats> {
        let url = format!("{}/v1/stats", self.base_url);
        let resp = self
            .apply_auth(ureq::get(&url))
            .call()
            .map_err(|e| self.map_ureq_error(e))?;
        let stats: StorageStats = resp
            .into_json()
            .context("Failed to parse stats response as JSON")?;
        Ok(stats)
    }

    pub fn remember(&self, req: &RememberRequest) -> Result<MemoryRecord> {
        let url = format!("{}/v1/remember", self.base_url);
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(req)
            .map_err(|e| self.map_ureq_error(e))?;
        let record: MemoryRecord = resp
            .into_json()
            .context("Failed to parse MemoryRecord response as JSON")?;
        Ok(record)
    }

    pub fn recall(&self, req: &RecallRequest) -> Result<Vec<ScoredMemory>> {
        let url = format!("{}/v1/recall", self.base_url);
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(req)
            .map_err(|e| self.map_ureq_error(e))?;
        let results: Vec<ScoredMemory> = resp
            .into_json()
            .context("Failed to parse recall results as JSON")?;
        Ok(results)
    }

    pub fn answer(&self, req: &AnswerRequest) -> Result<AnswerResult> {
        let url = format!("{}/v1/answer", self.base_url);
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(req)
            .map_err(|e| self.map_ureq_error(e))?;
        let answer: AnswerResult = resp
            .into_json()
            .context("Failed to parse AnswerResult response as JSON")?;
        Ok(answer)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn list(
        &self,
        limit: usize,
        offset: usize,
        page: Option<usize>,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        media: Option<String>,
        date: Option<NaiveDate>,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<PaginatedMemories> {
        let mut url = format!("{}/v1/list?limit={}&offset={}", self.base_url, limit, offset);
        if let Some(p) = page {
            url.push_str(&format!("&page={}", p));
        }
        if let Some(cat) = category {
            url.push_str(&format!("&category={}", cat));
        }
        if let Some(stat) = status {
            url.push_str(&format!("&status={}", stat.as_str()));
        }
        if let Some(ref m) = media {
            url.push_str(&format!("&media={}", m));
        }
        if let Some(d) = date {
            url.push_str(&format!("&date={}", d));
        }
        if let Some(a) = as_of {
            url.push_str(&format!("&as_of={}", a.to_rfc3339()));
        }

        let resp = self
            .apply_auth(ureq::get(&url))
            .call()
            .map_err(|e| self.map_ureq_error(e))?;
        let paginated: PaginatedMemories = resp
            .into_json()
            .context("Failed to parse paginated memories as JSON")?;
        Ok(paginated)
    }

    pub fn get(&self, term: &str) -> Result<Option<MemoryRecord>> {
        let url = format!("{}/v1/get?term={}", self.base_url, term);
        let res = self.apply_auth(ureq::get(&url)).call();
        match res {
            Ok(resp) => {
                let record: MemoryRecord = resp
                    .into_json()
                    .context("Failed to parse MemoryRecord as JSON")?;
                Ok(Some(record))
            }
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(e) => Err(self.map_ureq_error(e)),
        }
    }

    pub fn forget(&self, id: Option<&str>, all: bool, hard: bool) -> Result<bool> {
        let url = format!("{}/v1/forget", self.base_url);
        let payload = ForgetPayload {
            id: id.map(String::from),
            all,
            hard,
        };
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(&payload)
            .map_err(|e| self.map_ureq_error(e))?;
        let val: serde_json::Value = resp
            .into_json()
            .context("Failed to parse forget response as JSON")?;
        let success = val
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(success)
    }

    pub fn clear(&self, soft: bool) -> Result<usize> {
        let url = format!("{}/v1/clear", self.base_url);
        let payload = ClearPayload { soft };
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(&payload)
            .map_err(|e| self.map_ureq_error(e))?;
        let val: serde_json::Value = resp
            .into_json()
            .context("Failed to parse clear response as JSON")?;
        let cleared = val.get("cleared").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        Ok(cleared)
    }

    pub fn link(
        &self,
        source: &str,
        target: &str,
        relation: Option<&str>,
        weight: Option<f32>,
    ) -> Result<LinkResult> {
        let url = format!("{}/v1/link", self.base_url);
        let payload = LinkRequest {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.unwrap_or("relates_to").to_string(),
            weight: weight.unwrap_or(1.0),
        };
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(&payload)
            .map_err(|e| self.map_ureq_error(e))?;
        let link: LinkResult = resp
            .into_json()
            .context("Failed to parse LinkResult response as JSON")?;
        Ok(link)
    }

    pub fn unlink(&self, source: &str, target: &str, relation: Option<&str>) -> Result<bool> {
        let url = format!("{}/v1/unlink", self.base_url);
        let payload = UnlinkRequest {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.map(String::from),
        };
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(&payload)
            .map_err(|e| self.map_ureq_error(e))?;
        let val: serde_json::Value = resp
            .into_json()
            .context("Failed to parse unlink response as JSON")?;
        let unlinked = val
            .get("unlinked")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Ok(unlinked)
    }

    pub fn related(&self, id: &str, hops: usize) -> Result<Vec<RelatedMemory>> {
        let url = format!("{}/v1/related?id={}&hops={}", self.base_url, id, hops);
        let resp = self
            .apply_auth(ureq::get(&url))
            .call()
            .map_err(|e| self.map_ureq_error(e))?;
        let related: Vec<RelatedMemory> = resp
            .into_json()
            .context("Failed to parse related memories as JSON")?;
        Ok(related)
    }

    pub fn graph(&self, focus: Option<&str>, hops: Option<usize>) -> Result<GraphSnapshot> {
        let mut url = format!("{}/v1/graph", self.base_url);
        let mut params = Vec::new();
        if let Some(f) = focus {
            params.push(format!("focus={}", f));
        }
        if let Some(h) = hops {
            params.push(format!("hops={}", h));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        let resp = self
            .apply_auth(ureq::get(&url))
            .call()
            .map_err(|e| self.map_ureq_error(e))?;
        let graph: GraphSnapshot = resp
            .into_json()
            .context("Failed to parse GraphSnapshot as JSON")?;
        Ok(graph)
    }
}
