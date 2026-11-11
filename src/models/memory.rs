use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,
    Decision,
    Instruction,
    Preference,
    Learning,
    Goal,
    Commitment,
    Artifact,
    Event,
    Relationship,
    Observation,
    Error,
    Context,
    #[value(alias = "passwords")]
    Password,
}

impl MemoryType {
    pub const ALL: &'static [MemoryType] = &[
        MemoryType::Fact,
        MemoryType::Decision,
        MemoryType::Instruction,
        MemoryType::Preference,
        MemoryType::Learning,
        MemoryType::Goal,
        MemoryType::Commitment,
        MemoryType::Artifact,
        MemoryType::Event,
        MemoryType::Relationship,
        MemoryType::Observation,
        MemoryType::Error,
        MemoryType::Context,
        MemoryType::Password,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::Fact => "fact",
            MemoryType::Decision => "decision",
            MemoryType::Instruction => "instruction",
            MemoryType::Preference => "preference",
            MemoryType::Learning => "learning",
            MemoryType::Goal => "goal",
            MemoryType::Commitment => "commitment",
            MemoryType::Artifact => "artifact",
            MemoryType::Event => "event",
            MemoryType::Relationship => "relationship",
            MemoryType::Observation => "observation",
            MemoryType::Error => "error",
            MemoryType::Context => "context",
            MemoryType::Password => "password",
        }
    }

    /// Automatically infer the most appropriate MemoryType from raw text content across all 14 categories
    pub fn infer(content: &str) -> Self {
        let lower = content.trim().to_lowercase();

        // 0. Explicit start-of-line category prefixes take precedence over mid-sentence substrings
        if lower.starts_with("password:")
            || lower.starts_with("secret:")
            || lower.starts_with("credential:")
        {
            return MemoryType::Password;
        }
        if lower.starts_with("error:")
            || lower.starts_with("bug:")
            || lower.starts_with("panic:")
            || lower.starts_with("exception:")
        {
            return MemoryType::Error;
        }
        if lower.starts_with("preference:") {
            return MemoryType::Preference;
        }
        if lower.starts_with("rule:")
            || lower.starts_with("instruction:")
            || lower.starts_with("runbook:")
            || lower.starts_with("how to ")
        {
            return MemoryType::Instruction;
        }
        if lower.starts_with("decision:") {
            return MemoryType::Decision;
        }
        if lower.starts_with("goal:")
            || lower.starts_with("objective:")
            || lower.starts_with("okr:")
            || lower.starts_with("milestone:")
        {
            return MemoryType::Goal;
        }
        if lower.starts_with("todo:")
            || lower.starts_with("commitment:")
            || lower.starts_with("action item:")
        {
            return MemoryType::Commitment;
        }
        if lower.starts_with("learning:") || lower.starts_with("til:") {
            return MemoryType::Learning;
        }
        if lower.starts_with("event:") || lower.starts_with("incident:") {
            return MemoryType::Event;
        }
        if lower.starts_with("relationship:") {
            return MemoryType::Relationship;
        }
        if lower.starts_with("observation:") {
            return MemoryType::Observation;
        }
        if lower.starts_with("artifact:") {
            return MemoryType::Artifact;
        }
        if lower.starts_with("context:") || lower.starts_with("background:") {
            return MemoryType::Context;
        }
        if lower.starts_with("fact:") {
            return MemoryType::Fact;
        }

        // 1. Password / Secret / Credential
        let has_cred_url = lower
            .split_whitespace()
            .any(|tok| tok.contains("://") && tok.contains('@') && tok.matches(':').count() >= 2);

        if has_cred_url
            || lower.contains("password:")
            || lower.contains("password is")
            || lower.contains("api_key")
            || lower.contains("api key")
            || lower.contains("secret_key")
            || lower.contains("secret key")
            || lower.contains("signing secret")
            || lower.contains("webhook secret")
            || lower.contains("auth token")
            || lower.contains("private key")
        {
            return MemoryType::Password;
        }

        // 2. Error / Bug / Failure
        if lower.contains("panic:")
            || lower.contains("panic in ")
            || lower.contains("fix panic")
            || lower.contains("panicked")
            || lower.contains("segfault")
            || lower.contains("stack trace")
            || lower.contains("failed with")
            || lower.contains("connection refused")
            || lower.contains("connection timeout")
            || lower.contains("pool exhausted")
            || lower.contains("deadlock")
            || lower.contains("timeout error")
            || lower.contains("out of memory")
            || lower.contains("exception:")
        {
            return MemoryType::Error;
        }

        // 3. Decision / Architecture Choice (checked before bare "dark mode" / Preference)
        if lower.contains("we decided")
            || lower.contains("decided to ")
            || lower.contains("agreed to ")
            || lower.contains("standardized on ")
            || lower.contains("switched from ")
            || lower.contains("migrating to ")
            || lower.contains("chosen over ")
            || lower.contains("is deprecated")
            || lower.contains("adopted ")
        {
            return MemoryType::Decision;
        }

        // 4. Security / engineering imperatives checked before generic "prefer "
        if lower.contains("prevent ")
            || lower.starts_with("never ")
            || lower.contains(" never ")
            || lower.contains("must always ")
            || lower.contains("must never ")
            || lower.starts_with("do not ")
            || lower.contains(" do not ")
            || lower.starts_with("don't ")
            || lower.contains(" don't ")
        {
            return MemoryType::Instruction;
        }

        // 5. Preference / Style (checked before "always " so "Always prefer..." maps to Preference)
        if lower.contains("prefers ")
            || lower.contains("prefer ")
            || lower.contains("user prefers")
            || lower.contains("likes to use ")
            || lower.contains("favorite ")
            || lower.contains("dark mode")
            || lower.contains("keybindings")
        {
            return MemoryType::Preference;
        }

        // 6. Instruction / Rule / Runbook
        if lower.starts_with("always ")
            || lower.contains("make sure to ")
            || lower.contains("step 1")
        {
            return MemoryType::Instruction;
        }

        // 6. Goal / Objective / Target
        if lower.starts_with("goal:")
            || lower.starts_with("objective:")
            || lower.contains("objective:")
            || lower.contains("roadmap ")
            || lower.contains("our goal is")
            || lower.contains("target is to ")
            || lower.contains("aiming to ")
            || lower.contains("okr:")
            || lower.contains("milestone:")
        {
            return MemoryType::Goal;
        }

        // 7. Commitment / Todo / Deadline
        if lower.starts_with("todo:")
            || lower.starts_with("commitment:")
            || lower.starts_with("action item:")
            || lower.contains("promised to ")
            || lower.contains("committed to ")
            || lower.contains("will deliver by ")
            || lower.contains("deadline is ")
            || lower.contains("due by ")
        {
            return MemoryType::Commitment;
        }

        // 8. Learning / Insight / Root Cause
        if lower.starts_with("learning:")
            || lower.starts_with("til:")
            || lower.contains("learned that ")
            || lower.contains("lesson learned")
            || lower.contains("discovered that ")
            || lower.contains("turns out that ")
            || lower.contains("root cause was ")
            || lower.contains("realized that ")
            || lower.contains("to avoid process freeze")
        {
            return MemoryType::Learning;
        }

        // 9. Event / Incident / Deployment
        if lower.starts_with("event:")
            || lower.starts_with("incident:")
            || lower.contains("incident occurred")
            || lower.contains("outage on ")
            || lower.contains("incident on ")
            || lower.contains("deployed v")
            || lower.contains("released v")
            || lower.contains("meeting with ")
            || lower.contains("postmortem")
        {
            return MemoryType::Event;
        }

        // 10. Relationship / Ownership / Team
        if lower.starts_with("relationship:")
            || lower.contains("reports to ")
            || lower.contains("is the maintainer of ")
            || lower.contains("is the owner of ")
            || lower.contains("tech lead for ")
            || lower.contains("manages the ")
            || lower.contains("works closely with ")
        {
            return MemoryType::Relationship;
        }

        // 11. Observation / Profiling / Telemetry Notice
        if lower.starts_with("observation:")
            || lower.contains("noticed that ")
            || lower.contains("observed that ")
            || lower.contains("profiling shows ")
            || lower.contains("metrics show ")
            || lower.contains("cpu spikes when ")
            || lower.contains("latency increases when ")
        {
            return MemoryType::Observation;
        }

        // 12. Artifact / File / Reference Spec
        if lower.starts_with("artifact:")
            || lower.contains("dockerfile")
            || lower.contains("openapi")
            || lower.contains("schema definition")
            || lower.contains("specification saved in ")
            || lower.contains("located at `")
            || lower.contains("stored in `src/")
        {
            return MemoryType::Artifact;
        }

        // 13. Context / Background / Scope
        if lower.starts_with("context:")
            || lower.starts_with("background:")
            || lower.contains("currently working on ")
            || lower.contains("for this sprint")
            || lower.contains("project scope ")
        {
            return MemoryType::Context;
        }

        // 14. Default to Fact
        MemoryType::Fact
    }
}

impl fmt::Display for MemoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for MemoryType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "fact" => Ok(MemoryType::Fact),
            "decision" => Ok(MemoryType::Decision),
            "instruction" => Ok(MemoryType::Instruction),
            "preference" => Ok(MemoryType::Preference),
            "learning" => Ok(MemoryType::Learning),
            "goal" => Ok(MemoryType::Goal),
            "commitment" => Ok(MemoryType::Commitment),
            "artifact" => Ok(MemoryType::Artifact),
            "event" => Ok(MemoryType::Event),
            "relationship" => Ok(MemoryType::Relationship),
            "observation" => Ok(MemoryType::Observation),
            "error" => Ok(MemoryType::Error),
            "context" => Ok(MemoryType::Context),
            "password" | "passwords" => Ok(MemoryType::Password),
            other => Err(format!(
                "Unknown memory type '{}'. Valid types: fact, decision, instruction, preference, learning, goal, commitment, artifact, event, relationship, observation, error, context, password",
                other
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Expired,
}

impl MemoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryStatus::Active => "active",
            MemoryStatus::Expired => "expired",
        }
    }
}

impl FromStr for MemoryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "active" => Ok(MemoryStatus::Active),
            "expired" => Ok(MemoryStatus::Expired),
            other => Err(format!(
                "Unknown status '{}'. Must be 'active' or 'expired'",
                other
            )),
        }
    }
}

#[cfg(test)]
mod infer_tests {
    use super::*;
    use crate::repositories::Storage;
    use chrono::Duration;

    #[test]
    fn explicit_category_prefixes_take_precedence_over_substrings() {
        assert_eq!(
            MemoryType::infer("TODO: fix panic in worker"),
            MemoryType::Commitment
        );
        assert_eq!(
            MemoryType::infer("Error: api_key expired"),
            MemoryType::Error
        );
        assert_eq!(
            MemoryType::infer("Artifact: we decided to store schema here"),
            MemoryType::Artifact
        );
        assert_eq!(
            MemoryType::infer("Context: currently working on migrating to Postgres"),
            MemoryType::Context
        );
        assert_eq!(
            MemoryType::infer("Fact: user prefers dark mode in some docs"),
            MemoryType::Fact
        );
        assert_eq!(
            MemoryType::infer("Decision: fix panic by using bounded channels"),
            MemoryType::Decision
        );
        assert_eq!(
            MemoryType::infer("Goal: fix panic rate to zero"),
            MemoryType::Goal
        );
        assert_eq!(
            MemoryType::infer("Learning: api_key rotation requires dual reads"),
            MemoryType::Learning
        );
        assert_eq!(
            MemoryType::infer("Event: panic in production cluster"),
            MemoryType::Event
        );
        assert_eq!(
            MemoryType::infer("Relationship: Alice decided to mentor Bob"),
            MemoryType::Relationship
        );
        assert_eq!(
            MemoryType::infer("Observation: connection timeout during deploy"),
            MemoryType::Observation
        );
        assert_eq!(
            MemoryType::infer("Secret: some random text"),
            MemoryType::Password
        );
        assert_eq!(
            MemoryType::infer("Credential: internal service token"),
            MemoryType::Password
        );
    }

    #[test]
    fn credential_url_heuristic_checks_per_token() {
        assert_eq!(
            MemoryType::infer(
                "Docs for @scope/pkg are at https://registry.npmjs.org/@scope/pkg (port: 443)"
            ),
            MemoryType::Fact
        );
        assert_eq!(
            MemoryType::infer("Database URI is postgres://admin:secret123@localhost:5432/mydb"),
            MemoryType::Password
        );
    }

    #[test]
    fn instruction_preference_and_decision_collisions_resolved() {
        assert_eq!(
            MemoryType::infer("We decided to support dark mode in v2"),
            MemoryType::Decision
        );
        assert_eq!(
            MemoryType::infer("Always prefer parameterized SQL queries to prevent injection"),
            MemoryType::Instruction
        );
        assert_eq!(
            MemoryType::infer("Always prefer dark mode"),
            MemoryType::Preference
        );
        assert_eq!(
            MemoryType::infer("Always prefer concise single-line answers with zero fluff"),
            MemoryType::Preference
        );
    }

    #[test]
    fn as_of_excludes_expired_memories_with_null_expired_at() {
        let storage = Storage::open_in_memory().expect("open_in_memory");
        let now = Utc::now();

        let mut expired_no_ts = MemoryRecord::new(
            MemoryType::Fact,
            "Expired No TS".to_string(),
            "Redis legacy cluster runs on port 6379".to_string(),
            vec![],
            0.9,
            None,
        );
        expired_no_ts.created_at = now - Duration::hours(2);
        expired_no_ts.updated_at = now - Duration::hours(1);
        expired_no_ts.status = MemoryStatus::Expired;
        expired_no_ts.expired_at = None;
        storage
            .insert_memory(&expired_no_ts, Some(&[1.0, 0.0, 0.0, 0.0]))
            .expect("insert expired_no_ts");

        let mut active_mem = MemoryRecord::new(
            MemoryType::Fact,
            "Active Mem".to_string(),
            "Redis primary cluster runs on port 6380".to_string(),
            vec![],
            0.9,
            None,
        );
        active_mem.created_at = now - Duration::hours(2);
        active_mem.updated_at = now - Duration::hours(2);
        active_mem.status = MemoryStatus::Active;
        active_mem.expired_at = None;
        storage
            .insert_memory(&active_mem, Some(&[0.0, 1.0, 0.0, 0.0]))
            .expect("insert active_mem");

        let bm25_hits = storage
            .search_bm25("Redis", None, None, Some(now), 10)
            .expect("search_bm25");
        assert_eq!(bm25_hits.len(), 1);
        assert_eq!(bm25_hits[0].0, active_mem.id);

        let vec_hits = storage
            .get_candidate_vectors_checked(None, None, Some(now), None)
            .expect("get_candidate_vectors_checked");
        assert_eq!(vec_hits.len(), 1);
        assert_eq!(vec_hits[0].0, active_mem.id);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachmentMetadata {
    #[serde(default = "default_attachment_id")]
    pub id: String,
    #[serde(default)]
    pub memory_id: String,
    pub filename: String,
    pub mime_type: String,
    pub media_kind: String,
    pub size_bytes: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f32>,
    #[serde(default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_base64: Option<String>,
}

fn default_attachment_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl AttachmentMetadata {
    pub fn summary_text(&self) -> String {
        let short_sha = &self.sha256[..self.sha256.len().min(12)];
        let mut parts = vec![
            self.media_kind.clone(),
            self.mime_type.clone(),
            format!("{} bytes", self.size_bytes),
        ];
        if let Some(ref dims) = self.dimensions {
            parts.push(dims.clone());
        }
        if let Some(dur) = self.duration_secs {
            parts.push(format!("{:.1}s", dur));
        }
        parts.push(format!("sha256:{}", short_sha));
        format!("{} ({})", self.filename, parts.join(", "))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachmentInput {
    pub metadata: AttachmentMetadata,
    pub data: Vec<u8>,
}

impl AttachmentInput {
    pub fn from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        use anyhow::Context;
        let data = std::fs::read(path)
            .with_context(|| format!("Failed to read attachment file: {}", path.display()))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment.bin")
            .to_string();
        Ok(Self::from_bytes(&filename, data))
    }

    pub fn from_bytes(filename: &str, data: Vec<u8>) -> Self {
        let clean_name = std::path::Path::new(filename)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("attachment.bin")
            .trim();
        let final_name = if clean_name.is_empty() {
            "attachment.bin".to_string()
        } else {
            clean_name.to_string()
        };
        let (mime_type, media_kind) = detect_mime_and_kind(&final_name, &data);
        let (mp4_dims, mp4_dur) = if media_kind == "video" {
            parse_mp4_dimensions_and_duration(&data)
        } else {
            (None, None)
        };
        let dimensions = parse_image_dimensions(&data).or(mp4_dims);
        let duration_secs = parse_audio_duration_secs(&data).or(mp4_dur);
        let sha256 = compute_sha256_hex(&data);
        let size_bytes = data.len() as u64;

        Self {
            metadata: AttachmentMetadata {
                id: Uuid::new_v4().to_string(),
                memory_id: String::new(),
                filename: final_name,
                mime_type,
                media_kind,
                size_bytes,
                sha256,
                dimensions,
                duration_secs,
                created_at: Utc::now(),
                data_base64: None,
            },
            data,
        }
    }
}

/// Pure-Rust FIPS 180-4 SHA-256 hex digest implementation
pub fn compute_sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = Vec::with_capacity((data.len() + 72) & !63);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, &word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    format!(
        "{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
        h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]
    )
}

/// Pure-Rust RFC 4648 Base64 encoder
pub fn encode_base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(ALPHABET[((triple >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Pure-Rust RFC 4648 Base64 decoder
pub fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    if !clean.len().is_multiple_of(4) {
        return Err("Invalid base64 length".to_string());
    }
    let val = |b: u8| -> Result<u32, String> {
        match b {
            b'A'..=b'Z' => Ok((b - b'A') as u32),
            b'a'..=b'z' => Ok((b - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((b - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            b'=' => Ok(0),
            _ => Err(format!("Invalid base64 byte: {}", b)),
        }
    };
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    for chunk in clean.as_chunks::<4>().0 {
        let triple = (val(chunk[0])? << 18)
            | (val(chunk[1])? << 12)
            | (val(chunk[2])? << 6)
            | val(chunk[3])?;
        out.push(((triple >> 16) & 0xFF) as u8);
        if chunk[2] != b'=' {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if chunk[3] != b'=' {
            out.push((triple & 0xFF) as u8);
        }
    }
    Ok(out)
}

/// Detect `(mime_type, media_kind)` from magic bytes and filename extension
pub fn detect_mime_and_kind(filename: &str, data: &[u8]) -> (String, String) {
    // 1. Magic byte inspection
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return ("image/png".to_string(), "image".to_string());
    }
    if data.starts_with(b"\xFF\xD8\xFF") {
        return ("image/jpeg".to_string(), "image".to_string());
    }
    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return ("image/gif".to_string(), "image".to_string());
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return ("image/webp".to_string(), "image".to_string());
    }
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        return ("audio/wav".to_string(), "audio".to_string());
    }
    if data.starts_with(b"ID3") || (data.len() >= 2 && data[0] == 0xFF && (data[1] & 0xE0) == 0xE0)
    {
        return ("audio/mpeg".to_string(), "audio".to_string());
    }
    if data.starts_with(b"OggS") {
        return ("audio/ogg".to_string(), "audio".to_string());
    }
    if data.starts_with(b"fLaC") {
        return ("audio/flac".to_string(), "audio".to_string());
    }
    if data.starts_with(b"%PDF-") {
        return ("application/pdf".to_string(), "document".to_string());
    }
    if data.starts_with(b"PK\x03\x04") {
        return ("application/zip".to_string(), "archive".to_string());
    }
    if data.starts_with(b"\x1F\x8B") {
        return ("application/gzip".to_string(), "archive".to_string());
    }
    if data.len() >= 12 && &data[4..8] == b"ftyp" {
        let brand = &data[8..12];
        if brand == b"M4A " {
            return ("audio/mp4".to_string(), "audio".to_string());
        }
        return ("video/mp4".to_string(), "video".to_string());
    }

    // 2. Extension fallback
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "png" => ("image/png".to_string(), "image".to_string()),
        "jpg" | "jpeg" => ("image/jpeg".to_string(), "image".to_string()),
        "gif" => ("image/gif".to_string(), "image".to_string()),
        "webp" => ("image/webp".to_string(), "image".to_string()),
        "svg" => ("image/svg+xml".to_string(), "image".to_string()),
        "bmp" => ("image/bmp".to_string(), "image".to_string()),
        "ico" => ("image/x-icon".to_string(), "image".to_string()),
        "mp3" => ("audio/mpeg".to_string(), "audio".to_string()),
        "wav" => ("audio/wav".to_string(), "audio".to_string()),
        "ogg" | "opus" => ("audio/ogg".to_string(), "audio".to_string()),
        "flac" => ("audio/flac".to_string(), "audio".to_string()),
        "m4a" => ("audio/mp4".to_string(), "audio".to_string()),
        "aac" => ("audio/aac".to_string(), "audio".to_string()),
        "mp4" | "m4v" => ("video/mp4".to_string(), "video".to_string()),
        "webm" => ("video/webm".to_string(), "video".to_string()),
        "mov" => ("video/quicktime".to_string(), "video".to_string()),
        "pdf" => ("application/pdf".to_string(), "document".to_string()),
        "md" | "markdown" => ("text/markdown".to_string(), "document".to_string()),
        "txt" | "log" => ("text/plain".to_string(), "document".to_string()),
        "csv" => ("text/csv".to_string(), "document".to_string()),
        "json" => ("application/json".to_string(), "code".to_string()),
        "jsonl" => ("application/x-ndjson".to_string(), "code".to_string()),
        "toml" => ("application/toml".to_string(), "code".to_string()),
        "yaml" | "yml" => ("application/yaml".to_string(), "code".to_string()),
        "rs" => ("text/x-rust".to_string(), "code".to_string()),
        "py" => ("text/x-python".to_string(), "code".to_string()),
        "js" | "mjs" => ("text/javascript".to_string(), "code".to_string()),
        "ts" | "tsx" => ("text/typescript".to_string(), "code".to_string()),
        "go" => ("text/x-go".to_string(), "code".to_string()),
        "c" | "h" | "cpp" | "hpp" => ("text/x-c".to_string(), "code".to_string()),
        "java" => ("text/x-java".to_string(), "code".to_string()),
        "sh" | "zsh" | "bash" => ("text/x-shellscript".to_string(), "code".to_string()),
        "sql" => ("application/sql".to_string(), "code".to_string()),
        "html" | "htm" => ("text/html".to_string(), "code".to_string()),
        "css" => ("text/css".to_string(), "code".to_string()),
        "zip" => ("application/zip".to_string(), "archive".to_string()),
        "tar" => ("application/x-tar".to_string(), "archive".to_string()),
        "gz" | "tgz" => ("application/gzip".to_string(), "archive".to_string()),
        _ => {
            if std::str::from_utf8(data).is_ok() && !data.is_empty() {
                ("text/plain".to_string(), "document".to_string())
            } else {
                ("application/octet-stream".to_string(), "binary".to_string())
            }
        }
    }
}

/// Extract `WxH` dimensions from PNG, GIF, WebP, or JPEG headers
pub fn parse_image_dimensions(data: &[u8]) -> Option<String> {
    // PNG: 8-byte signature + 4-byte length + "IHDR" + 4-byte width + 4-byte height
    if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") && &data[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        if w > 0 && h > 0 {
            return Some(format!("{}x{}", w, h));
        }
    }
    // GIF: "GIF87a" / "GIF89a" + 2-byte LE width + 2-byte LE height
    if data.len() >= 10 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        let w = u16::from_le_bytes([data[6], data[7]]);
        let h = u16::from_le_bytes([data[8], data[9]]);
        if w > 0 && h > 0 {
            return Some(format!("{}x{}", w, h));
        }
    }
    // WebP: RIFF....WEBP followed by VP8X, VP8 , or VP8L chunk
    if data.len() >= 25 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        let chunk = &data[12..16];
        if chunk == b"VP8X" && data.len() >= 30 {
            let w = u32::from_le_bytes([data[24], data[25], data[26], 0]) + 1;
            let h = u32::from_le_bytes([data[27], data[28], data[29], 0]) + 1;
            if w > 0 && h > 0 {
                return Some(format!("{}x{}", w, h));
            }
        } else if chunk == b"VP8 " && data.len() >= 30 && data[23..26] == [0x9D, 0x01, 0x2A] {
            let w = (u16::from_le_bytes([data[26], data[27]]) & 0x3FFF) as u32;
            let h = (u16::from_le_bytes([data[28], data[29]]) & 0x3FFF) as u32;
            if w > 0 && h > 0 {
                return Some(format!("{}x{}", w, h));
            }
        } else if chunk == b"VP8L" && data[20] == 0x2F {
            let bits = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
            let w = (bits & 0x3FFF) + 1;
            let h = ((bits >> 14) & 0x3FFF) + 1;
            if w > 0 && h > 0 {
                return Some(format!("{}x{}", w, h));
            }
        }
    }
    // JPEG: scan SOF0..SOF3 markers (0xFF 0xC0..0xC3)
    if data.len() >= 4 && data.starts_with(b"\xFF\xD8") {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            while i < data.len() && data[i] == 0xFF {
                i += 1;
            }
            if i >= data.len() {
                break;
            }
            let marker = data[i];
            i += 1;
            if marker == 0xD9 || marker == 0xDA {
                break;
            }
            if i + 2 > data.len() {
                break;
            }
            let seg_len = u16::from_be_bytes([data[i], data[i + 1]]) as usize;
            if seg_len < 2 || i + seg_len > data.len() {
                break;
            }
            if matches!(marker, 0xC0..=0xC3) && seg_len >= 7 {
                let h = u16::from_be_bytes([data[i + 3], data[i + 4]]);
                let w = u16::from_be_bytes([data[i + 5], data[i + 6]]);
                if w > 0 && h > 0 {
                    return Some(format!("{}x{}", w, h));
                }
            }
            i += seg_len;
        }
    }
    None
}

/// Extract audio duration in seconds from WAV RIFF header
pub fn parse_audio_duration_secs(data: &[u8]) -> Option<f32> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    let mut byte_rate = 0u32;
    let mut data_bytes = 0u32;
    while pos + 8 <= data.len() {
        let chunk_id = &data[pos..pos + 4];
        let chunk_size =
            u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
        let body_start = pos + 8;
        if chunk_id == b"fmt " && body_start + 12 <= data.len() {
            byte_rate = u32::from_le_bytes([
                data[body_start + 8],
                data[body_start + 9],
                data[body_start + 10],
                data[body_start + 11],
            ]);
        } else if chunk_id == b"data" {
            data_bytes = chunk_size;
            break;
        }
        let padded = (chunk_size as usize) + ((chunk_size as usize) & 1);
        pos = body_start.saturating_add(padded);
    }
    if byte_rate > 0 && data_bytes > 0 {
        Some((data_bytes as f32) / (byte_rate as f32))
    } else {
        None
    }
}

/// Extract `(dimensions, duration_secs)` from ISO BMFF / MP4 / MOV `moov` -> `mvhd` & `tkhd` boxes
pub fn parse_mp4_dimensions_and_duration(data: &[u8]) -> (Option<String>, Option<f32>) {
    fn walk_boxes(slice: &[u8], dims: &mut Option<String>, dur: &mut Option<f32>) {
        let mut pos = 0usize;
        while pos + 8 <= slice.len() {
            let size32 =
                u32::from_be_bytes([slice[pos], slice[pos + 1], slice[pos + 2], slice[pos + 3]]);
            let box_type = &slice[pos + 4..pos + 8];
            let (box_len, hdr_len) = if size32 == 1 {
                if pos + 16 > slice.len() {
                    break;
                }
                let raw_size = u64::from_be_bytes([
                    slice[pos + 8],
                    slice[pos + 9],
                    slice[pos + 10],
                    slice[pos + 11],
                    slice[pos + 12],
                    slice[pos + 13],
                    slice[pos + 14],
                    slice[pos + 15],
                ]);
                let Ok(size64) = usize::try_from(raw_size) else {
                    break;
                };
                (size64, 16usize)
            } else if size32 == 0 {
                (slice.len() - pos, 8usize)
            } else {
                (size32 as usize, 8usize)
            };
            let Some(end) = pos.checked_add(box_len) else {
                break;
            };
            if box_len < hdr_len || end > slice.len() {
                break;
            }
            let body = &slice[pos + hdr_len..end];
            if box_type == b"moov" || box_type == b"trak" {
                walk_boxes(body, dims, dur);
            } else if box_type == b"mvhd" && dur.is_none() && !body.is_empty() {
                let version = body[0];
                if version == 0 && body.len() >= 20 {
                    let timescale = u32::from_be_bytes([body[12], body[13], body[14], body[15]]);
                    let duration = u32::from_be_bytes([body[16], body[17], body[18], body[19]]);
                    if timescale > 0 && duration > 0 {
                        *dur = Some((duration as f32) / (timescale as f32));
                    }
                } else if version == 1 && body.len() >= 32 {
                    let timescale = u32::from_be_bytes([body[20], body[21], body[22], body[23]]);
                    let duration = u64::from_be_bytes([
                        body[24], body[25], body[26], body[27], body[28], body[29], body[30],
                        body[31],
                    ]);
                    if timescale > 0 && duration > 0 {
                        *dur = Some((duration as f32) / (timescale as f32));
                    }
                }
            } else if box_type == b"tkhd" && dims.is_none() && !body.is_empty() {
                let version = body[0];
                let (w, h) = if version == 0 && body.len() >= 84 {
                    let w = u32::from_be_bytes([body[76], body[77], body[78], body[79]]) >> 16;
                    let h = u32::from_be_bytes([body[80], body[81], body[82], body[83]]) >> 16;
                    (w, h)
                } else if version == 1 && body.len() >= 96 {
                    let w = u32::from_be_bytes([body[88], body[89], body[90], body[91]]) >> 16;
                    let h = u32::from_be_bytes([body[92], body[93], body[94], body[95]]) >> 16;
                    (w, h)
                } else {
                    (0, 0)
                };
                if w > 0 && h > 0 {
                    *dims = Some(format!("{}x{}", w, h));
                }
            }
            if dims.is_some() && dur.is_some() {
                break;
            }
            pos = end;
        }
    }

    let mut dims = None;
    let mut dur = None;
    walk_boxes(data, &mut dims, &mut dur);
    (dims, dur)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntityRecord {
    pub name: String,
    pub val: String,
}

impl EntityRecord {
    pub fn new(name: impl Into<String>, val: impl Into<String>) -> Self {
        Self {
            name: name.into().trim().to_lowercase(),
            val: val.into().trim().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<crate::models::MemoryContext>,
    pub id: String,
    pub category: MemoryType,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub confidence: f32,
    pub status: MemoryStatus,
    pub provenance: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expired_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<EntityRecord>,
}

impl MemoryRecord {
    pub fn new(
        category: MemoryType,
        title: String,
        content: String,
        tags: Vec<String>,
        confidence: f32,
        provenance: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            context: None,
            category,
            title,
            content,
            tags,
            confidence: confidence.clamp(0.0, 1.0),
            status: MemoryStatus::Active,
            provenance: provenance.unwrap_or_else(|| "explicit_statement".to_string()),
            created_at: now,
            updated_at: now,
            expired_at: None,
            attachments: Vec::new(),
            entities: Vec::new(),
        }
    }

    /// Format as standardized memory card for embedding and display
    pub fn to_card_text(&self) -> String {
        let mut out = format!(
            "[{}] {}\n\n{}",
            self.category.as_str().to_uppercase(),
            self.title,
            self.content
        );
        if !self.attachments.is_empty() {
            let summaries: Vec<String> = self
                .attachments
                .iter()
                .map(AttachmentMetadata::summary_text)
                .collect();
            out.push_str(&format!("\n\nAttachments: {}", summaries.join("; ")));
        }
        if !self.entities.is_empty() {
            let ent_summaries: Vec<String> = self
                .entities
                .iter()
                .map(|e| format!("{}: {}", e.name, e.val))
                .collect();
            out.push_str(&format!("\n\nEntities: {}", ent_summaries.join("; ")));
        }
        if !self.tags.is_empty() {
            out.push_str(&format!("\n\nTags: {}", self.tags.join(", ")));
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredMemory {
    pub memory: MemoryRecord,
    pub score: f32,
    pub bm25_rank: Option<usize>,
    pub vector_rank: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedMemories {
    pub items: Vec<MemoryRecord>,
    pub total: usize,
    pub limit: usize,
    pub offset: usize,
    pub page: usize,
    pub total_pages: usize,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConflict {
    pub similarity: f32,
    pub overlap_ratio: f32,
    pub older_memory: MemoryRecord,
    pub newer_memory: MemoryRecord,
}

/// Parse a human-friendly TTL duration string (e.g. "30s", "15m", "24h", "7d", "2w")
pub fn parse_ttl_duration(raw: &str) -> Result<chrono::Duration, String> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() {
        return Err("TTL duration cannot be empty (examples: 30s, 15m, 24h, 7d)".to_string());
    }
    let (num_str, unit) = if let Some(n) = s.strip_suffix("ms") {
        (n.trim(), "ms")
    } else if let Some(n) = s.strip_suffix('s') {
        (n.trim(), "s")
    } else if let Some(n) = s.strip_suffix('m') {
        (n.trim(), "m")
    } else if let Some(n) = s.strip_suffix('h') {
        (n.trim(), "h")
    } else if let Some(n) = s.strip_suffix('d') {
        (n.trim(), "d")
    } else if let Some(n) = s.strip_suffix('w') {
        (n.trim(), "w")
    } else {
        (s.as_str(), "s")
    };

    let val: i64 = num_str.parse().map_err(|_| {
        format!(
            "Invalid TTL '{}'. Expected positive integer with unit s, m, h, d, or w (e.g. 24h, 7d)",
            raw
        )
    })?;
    if val <= 0 {
        return Err("TTL duration must be greater than 0".to_string());
    }

    let duration = match unit {
        "ms" => chrono::Duration::try_milliseconds(val),
        "s" => chrono::Duration::try_seconds(val),
        "m" => val.checked_mul(60).and_then(chrono::Duration::try_seconds),
        "h" => val
            .checked_mul(3600)
            .and_then(chrono::Duration::try_seconds),
        "d" => val
            .checked_mul(86400)
            .and_then(chrono::Duration::try_seconds),
        "w" => val
            .checked_mul(7 * 86400)
            .and_then(chrono::Duration::try_seconds),
        _ => return Err(format!("Unsupported TTL unit '{}'", unit)),
    };
    duration.ok_or_else(|| format!("TTL duration out of range: {}{}", val, unit))
}

pub fn extract_meaningful_tokens(
    content: &str,
) -> (
    std::collections::HashSet<String>,
    std::collections::HashSet<String>,
) {
    use std::collections::HashSet;

    const STOP_WORDS: &[&str] = &[
        "the", "a", "an", "on", "in", "at", "to", "for", "of", "with", "by", "is", "are", "was",
        "were", "be", "been", "has", "have", "had", "and", "or", "as", "from", "that", "this",
    ];

    let all_tokens: HashSet<String> = content
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.')
        .map(|w| {
            w.trim_matches(|c: char| c == '.' || c == '-' || c == '_')
                .to_lowercase()
        })
        .filter(|w| w.len() > 1 && !STOP_WORDS.contains(&w.as_str()))
        .collect();

    let word_tokens: HashSet<String> = all_tokens
        .iter()
        .filter(|w| w.chars().any(|c| c.is_alphabetic()))
        .cloned()
        .collect();

    (word_tokens, all_tokens)
}

/// Evaluate whether two memories conflict or overlap based on vector cosine similarity and token overlap.
/// No hardcoded slot regexes — works for any near-duplicate or updated fact.
pub fn detect_memory_conflict_with_similarity(
    a: &MemoryRecord,
    b: &MemoryRecord,
    vec_similarity: Option<f32>,
    min_similarity: f32,
) -> Option<MemoryConflict> {
    if a.id == b.id {
        return None;
    }

    let (words_a, all_a) = extract_meaningful_tokens(&a.content);
    let (words_b, all_b) = extract_meaningful_tokens(&b.content);

    if words_a.is_empty() || words_b.is_empty() {
        return None;
    }

    let word_inter = words_a.intersection(&words_b).count();
    if word_inter == 0 {
        return None;
    }

    let word_min = words_a.len().min(words_b.len()).max(1);
    let word_max = words_a.len().max(words_b.len()).max(1);
    let word_overlap = word_inter as f32 / word_min as f32;
    let max_overlap = word_inter as f32 / word_max as f32;

    let all_inter = all_a.intersection(&all_b).count();
    let all_min = all_a.len().min(all_b.len()).max(1);
    let overlap_ratio = all_inter as f32 / all_min as f32;

    // Combine semantic vector cosine similarity with lexical word overlap
    let raw_vec_sim = vec_similarity.unwrap_or(0.0);
    let effective_sim = if a.content.trim().eq_ignore_ascii_case(b.content.trim()) {
        1.0
    } else {
        raw_vec_sim.max(word_overlap * 0.92)
    };

    if effective_sim < min_similarity || word_overlap < 0.55 || max_overlap < 0.45 {
        return None;
    }

    let (older, newer) = if (a.created_at, &a.id) <= (b.created_at, &b.id) {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    };

    Some(MemoryConflict {
        similarity: effective_sim.clamp(0.0, 1.0),
        overlap_ratio: overlap_ratio.clamp(0.0, 1.0),
        older_memory: older,
        newer_memory: newer,
    })
}

/// Convenience wrapper when comparing two memories directly (uses lexical + optional vector similarity)
pub fn detect_memory_conflict(a: &MemoryRecord, b: &MemoryRecord) -> Option<MemoryConflict> {
    detect_memory_conflict_with_similarity(a, b, None, 0.72)
}

#[cfg(test)]
mod conflict_tests {
    use super::*;

    fn make_fact(content: &str) -> MemoryRecord {
        MemoryRecord::new(
            MemoryType::Fact,
            content.to_string(),
            content.to_string(),
            vec![],
            1.0,
            None,
        )
    }

    #[test]
    fn test_detects_overlapping_and_updated_memories_without_hardcoded_slots() {
        let m1 = make_fact("PostgreSQL primary database runs on port 5432");
        let m2 = make_fact("PostgreSQL primary database runs on port 5433");
        let conflict = detect_memory_conflict(&m1, &m2).expect("Should detect overlapping update");
        assert!(conflict.similarity >= 0.75);
        assert!(conflict.overlap_ratio >= 0.60);

        let d1 = make_fact("The vault stores sensitive personal documents including Aadhaar cards, PAN cards, educational certificates");
        let d2 = make_fact("The vault stores various personal documents including Aadhaar cards, PAN cards, educational certificates");
        let dup_conflict =
            detect_memory_conflict(&d1, &d2).expect("Should detect near-duplicate memories");
        assert!(dup_conflict.similarity >= 0.80);

        let unrelated1 = make_fact("PostgreSQL primary database runs on port 5432");
        let unrelated2 = make_fact("Mistri Venkata Annapurna Devi is my mother name");
        assert!(detect_memory_conflict(&unrelated1, &unrelated2).is_none());
    }
}
