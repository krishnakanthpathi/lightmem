use crate::models::link::{GraphSnapshot, RelatedMemory};
use crate::models::{
    EntitySummary, ListSort, MemoryConflict, MemoryRecord, MemoryStatus, MemoryType,
    PaginatedMemories, ScoredMemory, StorageStats,
};
use crate::services::api_server::{
    AnswerRequest, AutolinkRequest, ClearPayload, ConflictsRequest, ForgetPayload, LinkRequest,
    ObserveRequest, RecallRequest, RememberRequest, UnlinkRequest, UpdateRequest,
};
use crate::services::observer::ObservedCandidate;
use crate::services::reranker::AnswerResult;
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use std::time::Duration;

use serde::{Deserialize, Serialize};

fn query_encode(value: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            let _ = write!(encoded, "%{:02X}", byte);
        }
    }
    encoded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkResult {
    pub source_id: String,
    pub target_id: String,
    pub relation: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberResponse {
    #[serde(flatten)]
    pub memory: MemoryRecord,
    #[serde(default)]
    pub superseded: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<crate::MemoryConflict>,
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

    fn extract_error_detail(body: &str) -> String {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(msg) = v.get("error").and_then(|e| e.as_str()) {
                return msg.to_string();
            }
        }
        body.to_string()
    }

    fn map_ureq_error(&self, e: ureq::Error) -> anyhow::Error {
        match e {
            ureq::Error::Status(401, resp) => {
                let body = resp.into_string().unwrap_or_default();
                let detail = Self::extract_error_detail(&body);
                anyhow::anyhow!(
                    "Authentication failed (401 Unauthorized) at remote LightMem backend {}. {}\nPlease verify your API key.",
                    self.base_url,
                    detail
                )
            }
            ureq::Error::Status(code, resp) => {
                let body = resp
                    .into_string()
                    .unwrap_or_else(|_| format!("HTTP {}", code));
                let detail = Self::extract_error_detail(&body);
                anyhow::anyhow!(
                    "Remote server error ({}) at {}: {}",
                    code,
                    self.base_url,
                    detail
                )
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
        Ok(self.remember_detailed(req)?.memory)
    }

    pub fn remember_detailed(&self, req: &RememberRequest) -> Result<RememberResponse> {
        let url = format!("{}/v1/remember", self.base_url);
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(req)
            .map_err(|e| self.map_ureq_error(e))?;
        let record: RememberResponse = resp
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
        self.list_filtered(
            limit,
            offset,
            page,
            category,
            status,
            true,
            media,
            date,
            as_of,
            &[],
            None,
            None,
            ListSort::Newest,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn list_filtered(
        &self,
        limit: usize,
        offset: usize,
        page: Option<usize>,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        explicit_all_status: bool,
        media: Option<String>,
        date: Option<NaiveDate>,
        as_of: Option<DateTime<Utc>>,
        tags: &[String],
        session: Option<&str>,
        entity: Option<&str>,
        sort: ListSort,
    ) -> Result<PaginatedMemories> {
        let mut url = format!(
            "{}/v1/list?limit={}&offset={}&sort={}",
            self.base_url,
            limit,
            offset,
            sort.as_str()
        );
        if let Some(p) = page {
            url.push_str(&format!("&page={}", p));
        }
        if let Some(cat) = category {
            url.push_str(&format!("&category={}", cat));
        }
        if let Some(stat) = status {
            url.push_str(&format!("&status={}", stat.as_str()));
        } else if explicit_all_status {
            url.push_str("&status=all");
        }
        if let Some(ref m) = media {
            url.push_str(&format!("&media={}", query_encode(m)));
        }
        if let Some(d) = date {
            url.push_str(&format!("&date={}", d));
        }
        if let Some(a) = as_of {
            url.push_str(&format!("&as_of={}", query_encode(&a.to_rfc3339())));
        }
        if !tags.is_empty() {
            url.push_str(&format!("&tags={}", query_encode(&tags.join(","))));
        }
        if let Some(sid) = session.map(str::trim).filter(|s| !s.is_empty()) {
            url.push_str(&format!("&session={}", query_encode(sid)));
        }
        if let Some(ent) = entity.map(str::trim).filter(|s| !s.is_empty()) {
            url.push_str(&format!("&entity={}", query_encode(ent)));
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
        let url = format!("{}/v1/get?term={}", self.base_url, query_encode(term));
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

    pub fn forget_all(&self, hard: bool) -> Result<usize> {
        let url = format!("{}/v1/forget", self.base_url);
        let payload = ForgetPayload {
            id: None,
            all: true,
            hard,
        };
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(&payload)
            .map_err(|e| self.map_ureq_error(e))?;
        let val: serde_json::Value = resp
            .into_json()
            .context("Failed to parse forget response as JSON")?;
        Ok(val.get("deleted").and_then(|v| v.as_u64()).unwrap_or(0) as usize)
    }

    pub fn update(&self, req: &UpdateRequest) -> Result<MemoryRecord> {
        let url = format!("{}/v1/update", self.base_url);
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(req)
            .map_err(|e| self.map_ureq_error(e))?;
        resp.into_json()
            .context("Failed to parse updated MemoryRecord as JSON")
    }

    pub fn tags(&self, query: Option<&str>) -> Result<Vec<(String, usize)>> {
        let mut url = format!("{}/v1/tags", self.base_url);
        if let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) {
            url.push_str(&format!("?q={}", query_encode(q)));
        }
        let resp = self
            .apply_auth(ureq::get(&url))
            .call()
            .map_err(|e| self.map_ureq_error(e))?;
        let items: Vec<serde_json::Value> = resp
            .into_json()
            .context("Failed to parse tags response as JSON")?;
        Ok(items
            .into_iter()
            .filter_map(|v| {
                let tag = v.get("tag")?.as_str()?.to_string();
                let count = v.get("count").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
                Some((tag, count))
            })
            .collect())
    }

    pub fn entities(&self, name: Option<&str>) -> Result<Vec<EntitySummary>> {
        let mut url = format!("{}/v1/entities", self.base_url);
        if let Some(n) = name.map(str::trim).filter(|s| !s.is_empty()) {
            url.push_str(&format!("?name={}", query_encode(n)));
        }
        let resp = self
            .apply_auth(ureq::get(&url))
            .call()
            .map_err(|e| self.map_ureq_error(e))?;
        resp.into_json()
            .context("Failed to parse entities response as JSON")
    }

    pub fn find_by_entity(&self, name: &str, val: Option<&str>) -> Result<Vec<MemoryRecord>> {
        let mut url = format!(
            "{}/v1/entities?name={}&memories=true",
            self.base_url,
            query_encode(name)
        );
        if let Some(v) = val.map(str::trim).filter(|s| !s.is_empty()) {
            url.push_str(&format!("&val={}", query_encode(v)));
        }
        let resp = self
            .apply_auth(ureq::get(&url))
            .call()
            .map_err(|e| self.map_ureq_error(e))?;
        resp.into_json()
            .context("Failed to parse entity memories as JSON")
    }

    pub fn deduplicate(&self) -> Result<usize> {
        let url = format!("{}/v1/deduplicate", self.base_url);
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(serde_json::json!({}))
            .map_err(|e| self.map_ureq_error(e))?;
        let val: serde_json::Value = resp
            .into_json()
            .context("Failed to parse deduplicate response as JSON")?;
        Ok(val.get("merged").and_then(|v| v.as_u64()).unwrap_or(0) as usize)
    }

    pub fn autolink(&self, min_similarity: Option<f32>) -> Result<(usize, usize, usize)> {
        let url = format!("{}/v1/autolink", self.base_url);
        let payload = AutolinkRequest { min_similarity };
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(&payload)
            .map_err(|e| self.map_ureq_error(e))?;
        let val: serde_json::Value = resp
            .into_json()
            .context("Failed to parse autolink response as JSON")?;
        let new_links = val.get("new_links").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let nodes = val.get("nodes").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let edges = val.get("edges").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        Ok((new_links, nodes, edges))
    }

    pub fn conflicts(
        &self,
        yes: bool,
        min_similarity: f32,
        reranker: Option<&str>,
    ) -> Result<Vec<MemoryConflict>> {
        let url = format!("{}/v1/conflicts", self.base_url);
        let payload = ConflictsRequest {
            yes,
            min_similarity: Some(min_similarity),
            reranker: reranker.map(ToString::to_string),
        };
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(&payload)
            .map_err(|e| self.map_ureq_error(e))?;
        resp.into_json()
            .context("Failed to parse conflicts response as JSON")
    }

    pub fn observe(
        &self,
        req: &ObserveRequest,
    ) -> Result<(Vec<ObservedCandidate>, Vec<MemoryRecord>)> {
        let url = format!("{}/v1/observe", self.base_url);
        let resp = self
            .apply_auth(ureq::post(&url))
            .send_json(req)
            .map_err(|e| self.map_ureq_error(e))?;
        let val: serde_json::Value = resp
            .into_json()
            .context("Failed to parse observe response as JSON")?;
        let candidates: Vec<ObservedCandidate> = serde_json::from_value(
            val.get("candidates")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        )?;
        let saved: Vec<MemoryRecord> = serde_json::from_value(
            val.get("saved")
                .cloned()
                .unwrap_or_else(|| serde_json::json!([])),
        )?;
        Ok((candidates, saved))
    }

    pub fn links(&self, id: &str) -> Result<Vec<crate::models::MemoryLink>> {
        self.apply_auth(ureq::get(&format!(
            "{}/v1/links?id={}",
            self.base_url,
            query_encode(id)
        )))
        .call()
        .map_err(|e| self.map_ureq_error(e))?
        .into_json()
        .context("Invalid links response")
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
        let url = format!(
            "{}/v1/related?id={}&hops={}",
            self.base_url,
            query_encode(id),
            hops
        );
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
            params.push(format!("focus={}", query_encode(f)));
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
