use crate::embeddings::{cosine_similarity, EmbeddingProvider};
use crate::models::{MemoryRecord, MemoryStatus, MemoryType, ScoredMemory};
use crate::storage::Storage;
use anyhow::Result;
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

            let mut matched_proper_initials = false;
            let mut matched_any_initials = false;

            // Check if any N consecutive words in title/content have initials matching `q_tok`
            if (2..=5).contains(&n) && orig_words.len() >= n {
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
                || tags_lower.iter().any(|t| t == q_tok || t.starts_with(&format!("{}-", q_tok)))
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
        let fetch_limit = (limit * 6).max(80);
        let query_tokens = Self::extract_query_tokens(query);

        // 1. BM25 Search via FTS5
        let bm25_results = storage.search_bm25(query, category, status, as_of, fetch_limit)?;
        let mut bm25_ranks: HashMap<String, usize> = HashMap::new();
        for (rank_idx, (id, _)) in bm25_results.iter().enumerate() {
            bm25_ranks.insert(id.clone(), rank_idx + 1);
        }

        // 2. Vector Cosine Search
        let mut vector_ranks: HashMap<String, usize> = HashMap::new();
        let mut vector_scores: HashMap<String, f32> = HashMap::new();

        if let Ok(query_vector) = embedder.embed(query) {
            let candidates = storage.get_candidate_vectors(category, status, as_of)?;
            let mut scored_vectors: Vec<(String, f32)> = candidates
                .into_iter()
                .map(|(id, vec)| {
                    let sim = cosine_similarity(&query_vector, &vec);
                    (id, sim)
                })
                .collect();

            // Sort by cosine similarity descending
            scored_vectors
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            for (rank_idx, (id, score)) in scored_vectors.into_iter().enumerate() {
                if let Some(min_sim) = min_similarity {
                    if score < min_sim && !bm25_ranks.contains_key(&id) {
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
        let mut ranked_candidates: Vec<(String, f32)> = rrf_scores.into_iter().collect();
        ranked_candidates
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let inspect_ids: Vec<String> = ranked_candidates
            .into_iter()
            .take(fetch_limit)
            .map(|(id, _)| id)
            .collect();

        // 4. Hydrate, deduplicate identical content, and apply Acronym/Initials boost
        let mut seen_contents: HashSet<String> = HashSet::new();
        let mut results = Vec::new();

        for id in inspect_ids {
            if let Some(mem) = storage.get_memory(&id)? {
                let norm_content = mem
                    .content
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase();
                if !seen_contents.insert(norm_content) {
                    // Skip exact duplicate memory content so top candidates are 100% unique
                    continue;
                }

                let bm25_rank = bm25_ranks.get(&id).copied();
                let vector_rank = vector_ranks.get(&id).copied();
                let base_score = vector_scores.get(&id).copied().unwrap_or(0.0);
                let boost = Self::compute_acronym_and_lexical_boost(&query_tokens, &mem);
                let score = (base_score + boost).min(0.99);

                results.push(ScoredMemory {
                    memory: mem,
                    score,
                    bm25_rank,
                    vector_rank,
                });
            }
        }

        // Final sort by boosted similarity score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit);

        Ok(results)
    }
}
