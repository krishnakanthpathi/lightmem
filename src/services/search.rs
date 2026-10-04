use crate::embeddings::{cosine_similarity, EmbeddingProvider};
use crate::models::{MemoryRecord, MemoryStatus, MemoryType, ScoredMemory};
use crate::storage::Storage;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

pub struct HybridSearchEngine;

impl HybridSearchEngine {
    /// Extract non-stopword tokens from query for acronym/initials matching.
    pub fn extract_query_tokens(query: &str) -> Vec<String> {
        const STOPWORDS: &[&str] = &[
            "who", "what", "where", "when", "why", "how", "is", "are", "was", "were", "the", "a",
            "an", "in", "on", "at", "to", "for", "of", "with", "by", "from", "as", "and", "or",
            "my", "me", "do", "does", "did", "user", "name",
        ];
        query
            .split(|c: char| !c.is_alphanumeric())
            .map(|w| w.trim().to_lowercase())
            .filter(|w| w.len() >= 2 && !STOPWORDS.contains(&w.as_str()))
            .collect()
    }

    const NON_INITIAL_WORDS: &[&str] = &[
        "os", "db", "vm", "ci", "cd", "ip", "io", "ui", "ux", "id", "qa", "pr", "ai", "ml", "go",
        "js", "ts", "py", "rs", "sh", "up", "no", "ok", "api", "url", "uri", "sql", "tls", "ssl",
        "ssh", "tcp", "udp", "dns", "cpu", "gpu", "ram", "ssd", "env", "cli", "sdk", "jwt", "ttl",
        "wal", "fts", "rrf", "app", "log", "key", "tag", "net", "web", "dev", "ops", "sec", "port",
        "host", "node", "code", "data", "file", "path", "name", "time", "date", "role", "team",
        "mode", "type", "list", "page", "test", "prod", "main", "core", "base", "auth", "sync",
        "pool", "lock", "user", "work", "task", "goal", "fact", "rule", "plan", "item", "info",
        "text", "body", "view", "repo", "rust", "json", "yaml", "toml", "http", "grpc", "rest",
        "raid", "cidr", "vlan", "wasm", "onnx",
    ];

    /// Compute an acronym / initials / exact-token boost for a memory against query tokens.
    /// Example: query token `"kk"` matches consecutive capitalized words `"Krishna Kanth"` (`K` + `K` = `kk`).
    pub fn compute_acronym_and_lexical_boost(query_tokens: &[String], mem: &MemoryRecord) -> f32 {
        if query_tokens.is_empty() {
            return 0.0;
        }

        let combined_orig = format!("{} {}", mem.title, mem.content);
        let orig_words: Vec<&str> = combined_orig
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();

        let lower_words: Vec<String> = orig_words.iter().map(|w| w.to_lowercase()).collect();
        let tags_lower: Vec<String> = mem.tags.iter().map(|t| t.to_lowercase()).collect();

        let mut total_boost = 0.0f32;

        for q_tok in query_tokens {
            let q_chars: Vec<char> = q_tok.chars().collect();
            let n = q_chars.len();

            let is_exact_word_or_tag = lower_words.iter().any(|w| w == q_tok)
                || tags_lower
                    .iter()
                    .any(|t| t == q_tok || t.starts_with(&format!("{}-", q_tok)));
            let is_non_initial = Self::NON_INITIAL_WORDS.contains(&q_tok.as_str());

            let mut matched_proper_initials = false;
            let mut matched_any_initials = false;

            // Check if any N consecutive words in title/content have initials matching `q_tok`
            if !is_exact_word_or_tag
                && !is_non_initial
                && (2..=5).contains(&n)
                && orig_words.len() >= n
            {
                for window_idx in 0..=(orig_words.len() - n) {
                    let mut all_match = true;
                    let mut all_capitalized = true;
                    for k in 0..n {
                        let w_orig = orig_words[window_idx + k];
                        let first_orig = w_orig.chars().next().unwrap_or(' ');
                        if first_orig.to_ascii_lowercase() != q_chars[k] {
                            all_match = false;
                            break;
                        }
                        if !first_orig.is_ascii_uppercase() {
                            all_capitalized = false;
                        }
                    }
                    if all_match {
                        if all_capitalized {
                            matched_proper_initials = true;
                            break;
                        } else {
                            matched_any_initials = true;
                        }
                    }
                }
            }

            if matched_proper_initials {
                // Strong boost when query acronym matches Proper Noun initials (e.g. "kk" -> "Krishna Kanth")
                total_boost += 0.26;
            } else if matched_any_initials {
                total_boost += 0.12;
            } else if lower_words.iter().any(|w| w == q_tok)
                || tags_lower
                    .iter()
                    .any(|t| t == q_tok || t.starts_with(&format!("{}-", q_tok)))
            {
                // Exact token or tag prefix match (e.g. "kk" in "kk-linux")
                total_boost += 0.06;
            }
        }

        total_boost.min(0.35)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn search(
        storage: &Storage,
        embedder: &dyn EmbeddingProvider,
        query: &str,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        min_similarity: Option<f32>,
    ) -> Result<Vec<ScoredMemory>> {
        Self::search_with_identity(
            storage,
            embedder,
            query,
            category,
            status,
            as_of,
            limit,
            min_similarity,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn search_with_identity(
        storage: &Storage,
        embedder: &dyn EmbeddingProvider,
        query: &str,
        category: Option<MemoryType>,
        status: Option<MemoryStatus>,
        as_of: Option<DateTime<Utc>>,
        limit: usize,
        min_similarity: Option<f32>,
        identity: Option<&str>,
    ) -> Result<Vec<ScoredMemory>> {
        anyhow::ensure!(!query.trim().is_empty(), "Search query cannot be blank");
        if let Some(min) = min_similarity {
            anyhow::ensure!(
                min.is_finite() && (0.0..=1.0).contains(&min),
                "Minimum similarity must be between 0 and 1"
            );
        }
        if limit == 0 {
            return Ok(Vec::new());
        }
        let query_tokens = Self::extract_query_tokens(query);
        let is_bare_acronym_query = query_tokens.len() == 1
            && (2..=5).contains(&query_tokens[0].len())
            && !Self::NON_INITIAL_WORDS.contains(&query_tokens[0].as_str());
        let fetch_limit = if is_bare_acronym_query {
            limit.saturating_mul(6).max(1000)
        } else {
            limit.saturating_mul(6).max(80)
        };

        // 1. BM25 Search via FTS5
        let bm25_results = storage.search_bm25(query, category, status, as_of, fetch_limit)?;
        let mut bm25_ranks: HashMap<String, usize> = HashMap::new();
        for (rank_idx, (id, _)) in bm25_results.iter().enumerate() {
            bm25_ranks.insert(id.clone(), rank_idx + 1);
        }

        // 2. Vector Cosine Search
        let mut vector_ranks: HashMap<String, usize> = HashMap::new();
        let mut vector_scores: HashMap<String, f32> = HashMap::new();

        {
            let query_vector = embedder.embed(query).context("Query embedding failed")?;
            crate::embeddings::validate_vector(&query_vector)?;
            let candidates =
                storage.get_candidate_vectors_checked(category, status, as_of, identity)?;
            anyhow::ensure!(
                candidates
                    .iter()
                    .all(|(_, vector)| vector.len() == query_vector.len()
                        && vector.iter().all(|v| v.is_finite())),
                "Stored embedding dimensions/values are invalid; run lmem reindex"
            );
            let mut scored_vectors: Vec<(String, f32)> = candidates
                .into_iter()
                .map(|(id, vec)| {
                    let sim = cosine_similarity(&query_vector, &vec);
                    (id, sim)
                })
                .collect();

            // Select a bounded top-k pool without sorting the full vault.
            if !is_bare_acronym_query {
                scored_vectors.retain(|(_, score)| *score > 0.0);
            }
            let compare = |a: &(String, f32), b: &(String, f32)| {
                b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0))
            };
            if scored_vectors.len() > fetch_limit {
                scored_vectors.select_nth_unstable_by(fetch_limit, compare);
                scored_vectors.truncate(fetch_limit);
            }
            scored_vectors.sort_by(compare);

            for (rank_idx, (id, score)) in scored_vectors.into_iter().enumerate() {
                if let Some(min_sim) = min_similarity {
                    if score < min_sim {
                        continue;
                    }
                }
                vector_ranks.insert(id.clone(), rank_idx + 1);
                vector_scores.insert(id, score);
            }
        }

        // 3. Reciprocal Rank Fusion (RRF)
        const K: f32 = 60.0;
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();

        for (id, rank) in &bm25_ranks {
            let rrf = 1.0 / (K + *rank as f32);
            *rrf_scores.entry(id.clone()).or_insert(0.0) += rrf;
        }

        for (id, rank) in &vector_ranks {
            let rrf = 1.0 / (K + *rank as f32);
            *rrf_scores.entry(id.clone()).or_insert(0.0) += rrf;
        }

        // Sort candidate IDs by RRF score descending and inspect a wide pool for deduplication & acronym boosting
        let mut ranked_candidates: Vec<(String, f32)> = rrf_scores
            .iter()
            .map(|(id, score)| (id.clone(), *score))
            .collect();
        ranked_candidates.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let inspect_ids: Vec<String> = ranked_candidates
            .into_iter()
            .take(fetch_limit)
            .map(|(id, _)| id)
            .collect();

        // 4. Hydrate, deduplicate identical content, and apply Acronym/Initials boost
        let mut seen_contents: HashSet<(MemoryType, String)> = HashSet::new();
        let mut results = Vec::new();

        let mut memories = storage.get_memories_exact(&inspect_ids)?;
        for id in inspect_ids {
            if let Some(mem) = memories.remove(&id) {
                let key = (mem.category, crate::storage::duplicate_key(&mem.content));
                if !seen_contents.insert(key) {
                    continue;
                }
                if let Some(min) = min_similarity {
                    if vector_scores.get(&id).copied().unwrap_or(-1.0) < min {
                        continue;
                    }
                }
                let bm25_rank = bm25_ranks.get(&id).copied();
                let vector_rank = vector_ranks.get(&id).copied();
                let boost = Self::compute_acronym_and_lexical_boost(&query_tokens, &mem);
                let mut rrf_val = rrf_scores.get(&id).copied().unwrap_or(0.0);
                if !bm25_ranks.is_empty()
                    && bm25_rank.is_none()
                    && vector_rank.unwrap_or(999) <= 5
                    && boost >= 0.06 - 1e-5
                {
                    rrf_val += 1.0 / (K + 10.0 + vector_rank.unwrap_or(5) as f32);
                }
                let mut base_score = rrf_val * (K + 1.0) / 2.0;
                if is_bare_acronym_query && bm25_rank.is_none() && boost >= 0.12 - 1e-5 {
                    base_score += 0.5;
                }
                let effective_boost =
                    if boost >= 0.12 - 1e-5 || bm25_rank.is_some() || vector_rank == Some(1) {
                        boost
                    } else {
                        boost * 0.2
                    };
                let score = (0.85 * base_score + 0.15 * effective_boost / 0.35).min(1.0);

                results.push(ScoredMemory {
                    memory: mem,
                    score,
                    bm25_rank,
                    vector_rank,
                });
            }
        }

        // Keep fused ranking in the final score; it is a ranking score, not a probability.
        results.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.memory.id.cmp(&b.memory.id))
        });
        results.truncate(limit);

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptedEmbedder {
        query_vec: Vec<f32>,
    }

    impl EmbeddingProvider for ScriptedEmbedder {
        fn name(&self) -> &str {
            "scripted"
        }

        fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(self.query_vec.clone())
        }
    }

    #[test]
    fn ordinary_query_words_do_not_trigger_proper_initials_boost() {
        let tokens = HybridSearchEngine::extract_query_tokens("What is the OS of Helios?");
        assert_eq!(tokens, vec!["os".to_string(), "helios".to_string()]);

        // "Orion Service" has initials O + S = "os", but "os" is a common technical word in NON_INITIAL_WORDS
        let distractor = MemoryRecord::new(
            MemoryType::Fact,
            "Orion Service Config".to_string(),
            "Orion Service runs on port 4567 on Linux".to_string(),
            vec![],
            0.9,
            None,
        );
        let distractor_boost =
            HybridSearchEngine::compute_acronym_and_lexical_boost(&tokens, &distractor);
        assert_eq!(
            distractor_boost, 0.0,
            "Common token 'os' must not get +0.26 proper-initials boost on 'Orion Service'"
        );

        // Literal "OS" and "Helios" should still receive the normal 0.06 + 0.06 = 0.12 lexical boost
        let target = MemoryRecord::new(
            MemoryType::Fact,
            "Helios Server".to_string(),
            "Helios runs Ubuntu 24.04 LTS as its OS".to_string(),
            vec![],
            0.9,
            None,
        );
        let target_boost = HybridSearchEngine::compute_acronym_and_lexical_boost(&tokens, &target);
        assert!(
            (target_boost - 0.12).abs() < 1e-5,
            "Expected 0.12 exact-word boost for literal 'os' + 'helios', got {}",
            target_boost
        );
    }

    #[test]
    fn single_generic_token_lexical_boost_does_not_overtake_vector_rank_1_on_paraphrase() {
        let storage = Storage::open_in_memory().unwrap();
        let embedder = ScriptedEmbedder {
            query_vec: vec![1.0, 0.0],
        };

        // Paraphrase target at vector_rank = 1 (shares no exact tokens with query)
        let target = MemoryRecord::new(
            MemoryType::Fact,
            "Kubernetes Node Pool".to_string(),
            "Worker nodes autoscale from 3 to 12 instances".to_string(),
            vec![],
            0.9,
            None,
        );
        // Distractor at vector_rank = 2
        let dist2 = MemoryRecord::new(
            MemoryType::Fact,
            "Telemetry Pipeline".to_string(),
            "Metrics are exported every 15 seconds".to_string(),
            vec![],
            0.9,
            None,
        );
        // Distractor at vector_rank = 3 that happens to share one generic word "cluster" (boost = 0.06)
        let dist3 = MemoryRecord::new(
            MemoryType::Fact,
            "Redis Cache".to_string(),
            "Redis cluster uses 6 shards".to_string(),
            vec![],
            0.9,
            None,
        );

        storage.insert_memory(&target, Some(&[1.0, 0.0])).unwrap();
        storage
            .insert_memory(&dist2, Some(&[0.9, 0.43589]))
            .unwrap();
        storage.insert_memory(&dist3, Some(&[0.8, 0.6])).unwrap();

        let results = HybridSearchEngine::search(
            &storage,
            &embedder,
            "How is the cluster scaled?",
            None,
            None,
            None,
            3,
            None,
        )
        .unwrap();

        assert_eq!(
            results[0].memory.id, target.id,
            "Vector rank 1 paraphrase match must stay at Rank 1 ahead of vector rank 3 single-token distractor"
        );
    }

    #[test]
    fn bare_acronym_query_rescued_when_vault_exceeds_default_fetch_limit() {
        let storage = Storage::open_in_memory().unwrap();
        let embedder = ScriptedEmbedder {
            query_vec: vec![1.0, 0.0],
        };

        // Insert 120 unrelated distractors with higher cosine similarity (vector_rank 1..=120)
        for i in 0..120 {
            let m = MemoryRecord::new(
                MemoryType::Fact,
                format!("System Service {}", i),
                format!("Background worker node {} processes queue jobs", i),
                vec![],
                0.9,
                None,
            );
            let x = 0.99 - (i as f32) * 0.005;
            let y = (1.0 - x * x).max(0.0).sqrt();
            storage.insert_memory(&m, Some(&[x, y])).unwrap();
        }

        // Target memory for bare acronym "kk" is at vector_rank = 121 (outside old fetch_limit = 80)
        let target = MemoryRecord::new(
            MemoryType::Fact,
            "Architect Profile".to_string(),
            "Krishna Kanth is the Lead Systems Architect".to_string(),
            vec![],
            0.9,
            None,
        );
        storage
            .insert_memory(&target, Some(&[0.2, 0.979796]))
            .unwrap();

        let results =
            HybridSearchEngine::search(&storage, &embedder, "kk", None, None, None, 5, None)
                .unwrap();

        assert!(
            !results.is_empty(),
            "Expected bare acronym search to return results"
        );
        assert_eq!(
            results[0].memory.id, target.id,
            "Bare acronym 'kk' should rescue 'Krishna Kanth' from beyond default fetch_limit to Rank 1"
        );
    }
}
