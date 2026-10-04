//! Synthetic smoke evaluation. Run with native assets installed: cargo run --example needle_eval
use lightmem::{MemoryRecord, MemoryType, NeedleReranker, Reranker, ScoredMemory};
fn main() -> anyhow::Result<()> {
    let fixtures = [
        (
            "What port does Redis use?",
            "Redis runs on port 6379 on Linux.",
            Some("6379"),
        ),
        (
            "What port does Postgres service use?",
            "Postgres runs on port 5432 on Linux.",
            Some("5432"),
        ),
        (
            "What OS does Redis use?",
            "Redis runs on port 6379 on Linux.",
            Some("Linux"),
        ),
        (
            "What service runs on port 6379?",
            "Redis runs on port 6379 on Linux.",
            Some("Redis"),
        ),
        (
            "What is the user's name?",
            "The user's name is Avery Chen.",
            Some("Avery Chen"),
        ),
        (
            "What is the Atlas endpoint?",
            "Atlas endpoint is https://atlas.example.test/api",
            Some("https://atlas.example.test/api"),
        ),
        (
            "What port does Kafka use?",
            "Redis runs on port 6379 on Linux.",
            None,
        ),
        (
            "What port does Redis use?",
            "Redis is our cache. Its port has not been recorded.",
            None,
        ),
    ];
    let mut results = Vec::new();
    for (query, text, expected) in fixtures {
        let memory = MemoryRecord::new(
            MemoryType::Fact,
            "Synthetic fixture".into(),
            text.into(),
            vec![],
            0.9,
            None,
        );
        let result = NeedleReranker.answer(
            query,
            &[ScoredMemory {
                memory,
                score: 0.8,
                bm25_rank: Some(1),
                vector_rank: Some(1),
            }],
        )?;
        let correct = expected
            .map(|e| result.answer.eq_ignore_ascii_case(e))
            .unwrap_or(result.selected_memory.is_none());
        results.push(serde_json::json!({"question": query, "expected": expected, "answer": result.answer, "provider": result.reranker_used, "confidence": result.confidence, "correct": correct}));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
