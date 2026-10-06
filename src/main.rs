use anyhow::Result;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use clap::{Parser, Subcommand};
use lightmem::{parse_ttl_duration, CliView, LightMem, LightMemConfig, MemoryStatus, MemoryType};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "lmem")]
#[command(
    about = "❖ LightMem - Ultra-fast, lightweight agent memory engine",
    long_about = None,
    before_help = CliView::banner_string(None, None)
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Force use of global database (~/.lightmem/memories.db)
    #[arg(short = 'g', long, global = true)]
    global: bool,

    /// Explicit path to SQLite database file
    #[arg(long, global = true)]
    db: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Store a memory principle or fact
    Remember {
        /// Content of the memory
        content: String,

        /// Memory category (fact, decision, instruction, preference, learning, goal, commitment, artifact, event, relationship, observation, error, context, password)
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Short title (defaults to first line)
        #[arg(long)]
        title: Option<String>,

        /// Comma-separated tags
        #[arg(long)]
        tags: Option<String>,

        /// Confidence score (0.0 to 1.0)
        #[arg(short = 'c', long, default_value = "0.9")]
        confidence: f32,

        /// Auto-expire memory after a TTL duration (e.g. '30s', '15m', '24h', '7d', '2w')
        #[arg(long, value_parser = parse_ttl_duration)]
        ttl: Option<chrono::Duration>,

        /// Automatically supersede and merge older conflicting/overlapping memories
        #[arg(short = 's', long)]
        supersede: bool,

        /// Output created memory as JSON
        #[arg(long)]
        json: bool,
    },

    /// Recall memories via hybrid semantic + keyword search
    Recall {
        /// Search query string
        query: String,

        /// Filter by category
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Point-in-time recall: what was active as of date (YYYY-MM-DD or RFC3339)
        #[arg(long, value_parser = parse_as_of_date)]
        as_of: Option<DateTime<Utc>>,

        /// Filter to memories from a specific calendar day (YYYY-MM-DD)
        #[arg(long, value_parser = parse_exact_date)]
        date: Option<NaiveDate>,

        /// Max results to return
        #[arg(short = 'l', long, default_value = "10")]
        limit: usize,

        /// Minimum similarity threshold (0.0 - 1.0)
        #[arg(long)]
        min_similarity: Option<f32>,

        /// Toggle: Run precision QA reranker on recalled memories
        #[arg(long)]
        precision: bool,

        /// Expand recall hits with their 1-hop connected neighbors in the knowledge graph
        #[arg(long)]
        multi_hop: bool,

        /// Output results as JSON for agent consumption
        #[arg(long)]
        json: bool,
    },

    /// List memories chronologically with pagination (limit, offset, page, total)
    List {
        /// Filter by category
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Filter by status (active | expired)
        #[arg(long, default_value = "active")]
        status: MemoryStatus,

        /// Point-in-time view (YYYY-MM-DD)
        #[arg(long, value_parser = parse_as_of_date)]
        as_of: Option<DateTime<Utc>>,

        /// Filter to memories from a specific calendar day (YYYY-MM-DD)
        #[arg(long, value_parser = parse_exact_date)]
        date: Option<NaiveDate>,

        /// Max results per page
        #[arg(short = 'l', long, default_value = "20")]
        limit: usize,

        /// Number of records to skip (0-indexed offset)
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Page number (1-indexed; overrides --offset if specified)
        #[arg(short = 'p', long)]
        page: Option<usize>,

        /// Output paginated result as JSON (includes total, limit, offset, page, total_pages, has_more, items)
        #[arg(long)]
        json: bool,
    },

    /// Deeply inspect a memory record, its metadata, and knowledge graph links
    #[command(alias = "get", alias = "show")]
    Inspect {
        /// Memory ID (UUID) or title
        term: String,

        /// Output memory details as JSON
        #[arg(long)]
        json: bool,
    },

    /// Deduplicate identical or near-duplicate memories by merging tags, titles, and timestamps
    #[command(alias = "dedup")]
    Deduplicate {
        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Forget or expire a memory (or all memories with --all)
    Forget {
        /// Memory ID to forget (or omit when using --all)
        id: Option<String>,

        /// Forget all memories in the database
        #[arg(long)]
        all: bool,

        /// Permanently delete instead of soft-retiring
        #[arg(long)]
        hard: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Delete / wipe the entire memory database (aliases: purge, reset)
    #[command(visible_alias = "purge", visible_alias = "reset")]
    Clear {
        /// Soft-retire all memories instead of permanently deleting the database
        #[arg(long)]
        soft: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Export memories to Open Knowledge Format (OKF)
    Export {
        /// Export to Open Knowledge Format bundle
        #[arg(long)]
        okf: bool,

        /// Lossless JSON backup including expired memories
        #[arg(long, conflicts_with = "okf")]
        json: bool,

        /// Output file path
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },

    /// Import external memories from file (.json, .jsonl, or .okf/.md) or directory
    Import {
        /// Path to import file or OKF directory
        file: PathBuf,

        /// Opt in to heuristic title/category enrichment
        #[arg(long)]
        enrich: bool,
    },

    /// Ask a question and synthesize/extract the factual answer (via local ONNX Extractive QA or Ollama)
    Answer {
        /// The question to answer
        question: String,

        /// Toggle: Force use of precision extractive QA reranker
        #[arg(long)]
        precision: bool,

        /// Override reranker mode for this query ("onnx", "ollama", "ollama:<model>", or "top1")
        #[arg(short = 'r', long)]
        reranker: Option<String>,

        /// Filter candidate memories by category
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Point-in-time view (YYYY-MM-DD or RFC3339)
        #[arg(long, value_parser = parse_as_of_date)]
        as_of: Option<DateTime<Utc>>,

        /// Filter candidate memories to a specific calendar day (YYYY-MM-DD)
        #[arg(long, value_parser = parse_exact_date)]
        date: Option<NaiveDate>,

        /// Max candidate memories to retrieve for reranking
        #[arg(short = 'l', long, default_value = "10")]
        limit: usize,

        /// Output results as JSON for agent consumption
        #[arg(long)]
        json: bool,
    },

    /// View or update backend configuration
    Config {
        /// Set backend engine: 'onnx', 'ollama', or 'hash'
        #[arg(long)]
        backend: Option<String>,

        /// ONNX embedding model name ('bge-small', 'minilm', 'nomic') or path to directory with custom model.onnx
        #[arg(long)]
        onnx_model: Option<String>,

        /// Pre-download ONNX model(s) into ~/.lightmem/models ('bge-small', 'minilm', 'nomic', 'minilm-squad2', 'tinyroberta-squad2', 'qa', or 'all')
        #[arg(long)]
        download: Option<String>,

        /// Ollama server URL (e.g. http://localhost:11434)
        #[arg(long)]
        url: Option<String>,

        /// Ollama embedding model name (e.g. nomic-embed-text)
        #[arg(long)]
        model: Option<String>,

        /// Reranker model/engine ('minilm-squad2', 'tinyroberta-squad2', 'onnx', 'ollama', 'ollama:<model>', or 'top1')
        #[arg(long)]
        reranker: Option<String>,

        /// Delete / reset the active memory database
        #[arg(long)]
        reset_db: bool,

        /// Approve migration of the selected database when the embedding model changes
        #[arg(long)]
        yes: bool,
    },

    /// Rebuild embeddings for the configured model, with progress
    Reindex {
        /// Explicitly approve migration without an interactive prompt
        #[arg(long)]
        yes: bool,
    },

    /// Display storage statistics and active database path
    Stats {
        /// Output statistics as JSON
        #[arg(long)]
        json: bool,
    },

    /// Find overlapping or conflicting memories via vector similarity + token overlap, and interactively (or with --yes) merge them
    Conflicts {
        /// Force-merge all detected conflict/overlap pairs without prompting
        #[arg(short = 'y', long, visible_alias = "resolve")]
        yes: bool,

        /// Override reranker used for merging ("onnx", "ollama", "ollama:<model>", or "top1")
        #[arg(short = 'r', long)]
        reranker: Option<String>,

        /// Minimum similarity threshold to flag as overlapping/conflicting (0.0 - 1.0)
        #[arg(long, default_value = "0.78")]
        min_similarity: f32,

        /// Output conflict report as JSON
        #[arg(long)]
        json: bool,
    },

    /// Create a directional relationship link between two memories
    Link {
        /// Source memory ID, title, or prefix
        source: String,

        /// Target memory ID, title, or prefix
        target: String,

        /// Relationship type (e.g. relates_to, depends_on, references, implements, causes)
        #[arg(short = 'r', long, default_value = "relates_to")]
        relation: String,

        /// Connection weight / strength (0.1 to 10.0)
        #[arg(short = 'w', long, default_value = "1.0")]
        weight: f32,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Remove a relationship link between two memories
    Unlink {
        /// Source memory ID, title, or prefix
        source: String,

        /// Target memory ID, title, or prefix
        target: String,

        /// Optional specific relation to remove (omitting removes all links between the two)
        #[arg(short = 'r', long)]
        relation: Option<String>,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Traverse and list multi-hop connected memories from a starting node
    Related {
        /// Starting memory ID, title, or prefix
        id: String,

        /// Maximum hop distance to traverse (1 to 5)
        #[arg(short = 'n', long, default_value = "2")]
        hops: usize,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Terminal network tree or JSON knowledge graph
    Graph {
        /// Focus the graph on a specific memory neighborhood (ID or title)
        #[arg(short = 'f', long)]
        focus: Option<String>,

        /// Maximum hop radius when focused on a memory neighborhood
        #[arg(short = 'n', long, default_value = "2")]
        hops: usize,

        /// Output graph nodes and edges as JSON
        #[arg(long)]
        json: bool,
    },

    /// Automatically discover and connect memories via title mentions, shared tags, and vector similarity
    #[command(alias = "auto-link")]
    Autolink {
        /// Minimum cosine similarity threshold for semantic links (0.1 to 1.0)
        #[arg(long, default_value = "0.75")]
        min_similarity: f32,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Connect and deploy LightMem agent skill to AI environments (Antigravity, Codex, Hermes, Cursor, Claude, Agents)
    Connect {
        /// Target agent platform: 'antigravity', 'codex', 'hermes', 'cursor', 'claude', 'agents', or 'all' (auto-detects if omitted)
        platform: Option<String>,

        /// Install skill into the current workspace directory instead of user global config
        #[arg(short = 'w', long)]
        workspace: bool,

        /// Custom target directory path to write the lightmem skill into
        #[arg(short = 'p', long)]
        path: Option<PathBuf>,

        /// List supported agent platforms and detection status without installing
        #[arg(short = 'l', long)]
        list: bool,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Generate shell completion scripts (zsh, bash, fish) with interactive tab/arrow navigation
    Completions {
        /// Target shell (zsh, bash, fish, elvish, powershell)
        shell: clap_complete::Shell,
    },

    /// Uninstall LightMem binaries, shell completions, and optionally purge data
    Uninstall {
        /// Purge database (~/.lightmem/memories.db) and downloaded models
        #[arg(short = 'a', long = "purge-data")]
        purge_data: bool,

        /// Non-interactive mode (proceed without confirmation)
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },
}

fn parse_as_of_date(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
    let naive_dt = date.and_hms_opt(23, 59, 59).unwrap();
    Ok(Utc.from_utc_datetime(&naive_dt))
}

fn parse_exact_date(s: &str) -> Result<NaiveDate> {
    if let Ok(date) = NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
        return Ok(date);
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s.trim()) {
        return Ok(dt.with_timezone(&Utc).date_naive());
    }
    anyhow::bail!(
        "Invalid --date '{}'. Expected YYYY-MM-DD (e.g. 2026-09-20)",
        s
    )
}

fn open_controller(db_path: Option<&Path>, global: bool) -> Result<LightMem> {
    if let Some(path) = db_path {
        let config = LightMemConfig::try_load()?;
        LightMem::open_at(path, config)
    } else {
        LightMem::open_default(global)
    }
}

fn remove_db_files(db_path: &Path) {
    let _ = std::fs::remove_file(db_path);
    if let Some(s) = db_path.to_str() {
        let _ = std::fs::remove_file(format!("{}-wal", s));
        let _ = std::fs::remove_file(format!("{}-shm", s));
    }
}

fn migrate_embeddings(lm: &LightMem, yes: bool, machine_output: bool, force: bool) -> Result<bool> {
    if !force && !lm.embedding_migration_needed()? {
        return Ok(true);
    }
    let count = lm.count(None, None, None)?;
    eprintln!(
        "Embedding migration for {} ({} memories)",
        lm.db_path().display(),
        count
    );
    eprintln!(
        "  From: {}",
        lm.stored_embedding_identity()?
            .as_deref()
            .unwrap_or("legacy / unknown")
    );
    eprintln!("  To:   {}", lm.requested_embedding_identity());
    if !yes {
        anyhow::ensure!(!machine_output && io::stdin().is_terminal() && io::stderr().is_terminal(),
            "Migration needs approval. For a model change, repeat the same config command with --yes. Otherwise run lmem --db '{}' reindex --yes. No embeddings were changed.", lm.db_path().display());
        eprint!("Rebuild embeddings now? The old index stays intact until completion. [y/N] ");
        io::stderr().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if !matches!(response.trim().to_lowercase().as_str(), "y" | "yes") {
            eprintln!("Migration cancelled. Configuration and embeddings were not changed.");
            return Ok(false);
        }
    }
    let count = lm.reindex(|done, total| {
        eprintln!(
            "Embedding progress: {}/{} ({}%)",
            done,
            total,
            done.saturating_mul(100).checked_div(total).unwrap_or(100)
        );
    })?;
    eprintln!("Migration complete: {} embeddings committed.", count);
    Ok(true)
}

fn main() -> Result<()> {
    #[cfg(windows)]
    {
        let _ = colored::control::set_virtual_terminal(true);
    }
    let cli = Cli::parse();
    let env_db = std::env::var("LIGHTMEM_DB").ok().map(PathBuf::from);
    let effective_db = cli.db.as_deref().or(env_db.as_deref());
    let global = cli.global;

    let Some(command) = cli.command else {
        let lm = open_controller(effective_db, global)?;
        let stats = lm.stats()?;
        CliView::render_intro(lm.db_path(), &stats);
        return Ok(());
    };

    match command {
        Commands::Remember {
            content,
            category,
            title,
            tags,
            confidence,
            ttl,
            supersede,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category;
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
            let (memory, conflicts) = lm.remember_with_options(
                &content,
                cat,
                title,
                tag_vec,
                Some(confidence),
                ttl,
                supersede,
            )?;
            CliView::render_remembered_with_conflicts(
                &memory,
                &conflicts,
                supersede,
                lm.db_path(),
                json,
            )?;
        }

        Commands::Recall {
            query,
            category,
            as_of,
            date,
            limit,
            min_similarity,
            precision,
            multi_hop,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category;
            let as_of_dt = as_of;

            if !migrate_embeddings(&lm, false, json, false)? {
                return Ok(());
            }
            if precision {
                let result =
                    lm.answer_with_options(&query, cat, as_of_dt, date, limit, Some("onnx"))?;
                CliView::render_answer(&result, json)?;
            } else {
                let results = lm.recall_expanded(
                    &query,
                    cat,
                    as_of_dt,
                    date,
                    limit,
                    min_similarity,
                    multi_hop,
                )?;
                CliView::render_recall(&query, &results, lm.db_path(), json)?;
            }
        }

        Commands::List {
            category,
            status,
            as_of,
            date,
            limit,
            offset,
            page,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category;
            let st = Some(status);
            let as_of_dt = as_of;

            let paginated = if let Some(p) = page {
                lm.list_page_with_date(cat, st, as_of_dt, date, p, limit)?
            } else {
                lm.list_paginated_with_date(cat, st, as_of_dt, date, limit, offset)?
            };
            CliView::render_paginated_list(&paginated, lm.db_path(), json)?;
        }

        Commands::Inspect { term, json } => {
            let lm = open_controller(effective_db, global)?;
            if let Some(record) = lm.get(&term)? {
                let links = lm.get_links(&record.id).unwrap_or_default();
                CliView::render_inspect(&record, &links, lm.db_path(), json)?;
            } else {
                if json {
                    println!("null");
                } else {
                    eprintln!("Memory not found: {}", term);
                }
                std::process::exit(1);
            }
        }

        Commands::Deduplicate { json } => {
            let lm = open_controller(effective_db, global)?;
            let merged = lm.deduplicate()?;
            CliView::render_dedup(merged, json)?;
        }

        Commands::Forget {
            id,
            all,
            hard,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            if all || id.as_deref() == Some("all") {
                let db_path = lm.db_path().to_path_buf();
                let removed = lm.clear_all(hard)?;
                drop(lm);
                if hard {
                    remove_db_files(&db_path);
                }
                CliView::render_clear(removed, hard, &db_path, json)?;
            } else if let Some(target_id) = id {
                let ok = lm.forget(&target_id, hard)?;
                CliView::render_forget(&target_id, ok, hard, json)?;
            } else {
                anyhow::bail!("Provide a memory <ID> or pass '--all' to forget all memories.");
            }
        }

        Commands::Clear { soft, json } => {
            let hard = !soft;
            let lm = open_controller(effective_db, global)?;
            let db_path = lm.db_path().to_path_buf();
            let removed = lm.clear_all(hard)?;
            drop(lm);
            if hard {
                remove_db_files(&db_path);
            }
            CliView::render_clear(removed, hard, &db_path, json)?;
        }

        Commands::Export {
            okf: _,
            json,
            output,
        } => {
            let lm = open_controller(effective_db, global)?;
            let exported_path = if json {
                let target = output.unwrap_or_else(|| PathBuf::from("lightmem-backup.json"));
                lm.export_json(&target)?
            } else {
                lm.export_okf(output.as_deref())?
            };
            CliView::render_export(&exported_path);
        }

        Commands::Import { file, enrich } => {
            let lm = open_controller(effective_db, global)?;
            if !migrate_embeddings(&lm, false, false, false)? {
                return Ok(());
            }
            let count = lm.import_file_with_enrichment(&file, enrich)?;
            CliView::render_import(count, &file);
        }

        Commands::Answer {
            question,
            precision,
            reranker,
            category,
            as_of,
            date,
            limit,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category;
            let as_of_dt = as_of;

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
            let result =
                lm.answer_with_options(&question, cat, as_of_dt, date, limit, effective_reranker)?;
            CliView::render_answer(&result, json)?;
        }

        Commands::Config {
            backend,
            onnx_model,
            download,
            url,
            model,
            reranker,
            reset_db,
            yes,
        } => {
            if reset_db {
                let lm = open_controller(effective_db, global)?;
                let db_path = lm.db_path().to_path_buf();
                let removed = lm.clear_all(true)?;
                drop(lm);
                remove_db_files(&db_path);
                CliView::render_clear(removed, true, &db_path, false)?;
                if backend.is_none()
                    && onnx_model.is_none()
                    && download.is_none()
                    && url.is_none()
                    && model.is_none()
                    && reranker.is_none()
                {
                    return Ok(());
                }
            }

            if let Some(ref dl) = download {
                match dl.to_lowercase().as_str() {
                    "all" => {
                        for m in ["bge-small", "minilm", "nomic"] {
                            let _ = lightmem::OnnxEmbeddingProvider::new(Some(m))?;
                        }
                        let _ = lightmem::OnnxQaReranker::ensure_model_downloaded(Some(
                            "minilm-squad2",
                        ))?;
                    }
                    "qa" | "minilm-squad2" | "deepset/minilm-uncased-squad2" => {
                        let _ = lightmem::OnnxQaReranker::ensure_model_downloaded(Some(
                            "minilm-squad2",
                        ))?;
                    }
                    "tinyroberta" | "tinyroberta-squad2" | "deepset/tinyroberta-squad2" => {
                        let _ = lightmem::OnnxQaReranker::ensure_model_downloaded(Some(
                            "tinyroberta-squad2",
                        ))?;
                    }
                    "bge-small" | "bge-small-en-v1.5" | "xenova/bge-small-en-v1.5" => {
                        let _ = lightmem::OnnxEmbeddingProvider::new(Some("bge-small"))?;
                    }
                    "minilm" | "all-minilm-l6-v2" | "xenova/all-minilm-l6-v2" => {
                        let _ = lightmem::OnnxEmbeddingProvider::new(Some("minilm"))?;
                    }
                    "nomic" | "nomic-embed-text" | "nomic-ai/nomic-embed-text-v1.5" => {
                        let _ = lightmem::OnnxEmbeddingProvider::new(Some("nomic"))?;
                    }
                    other => anyhow::bail!(
                        "Unknown model '{}' for --download. Choose 'bge-small', 'minilm', 'nomic', 'minilm-squad2', 'tinyroberta-squad2', or 'all'.",
                        other
                    ),
                }
            }

            let mut cfg = LightMemConfig::try_load()?;
            let previous_identity = cfg.embedding_identity().ok();
            let mut changed = false;

            if let Some(b) = backend {
                cfg.backend = b;
                changed = true;
            }
            if let Some(om) = onnx_model {
                cfg.onnx_model = Some(om);
                changed = true;
            }
            if let Some(u) = url {
                cfg.ollama_url = u;
                changed = true;
            }
            if let Some(m) = model {
                cfg.embedding_model = m;
                changed = true;
            }
            if let Some(r) = reranker {
                let trimmed = r.trim();
                let lower = trimmed.to_lowercase();
                if lower.starts_with("ollama:")
                    || lower.starts_with("onnx:")
                    || matches!(
                        lower.as_str(),
                        "onnx"
                            | "qa"
                            | "precision"
                            | "ollama"
                            | "top1"
                            | "minilm-squad2"
                            | "tinyroberta-squad2"
                    )
                    || lower.contains(':')
                {
                    cfg.reranker = LightMemConfig::normalize_reranker(trimmed, None);
                    changed = true;
                } else {
                    anyhow::bail!(
                        "Invalid reranker '{}'. Choose 'minilm-squad2', 'tinyroberta-squad2', 'onnx', 'ollama' ('ollama:<model>'), or 'top1'.",
                        r
                    );
                }
            }

            if changed {
                let path = effective_db
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| LightMemConfig::resolve_db_path(global));
                let lm = LightMem::open_at(&path, cfg.clone())?;
                if previous_identity.as_deref() != Some(lm.requested_embedding_identity())
                    && !migrate_embeddings(&lm, yes, false, false)?
                {
                    return Ok(());
                }
                cfg.save()?;
            }
            CliView::render_config(&cfg, changed);
        }

        Commands::Reindex { yes } => {
            let lm = open_controller(effective_db, global)?;
            migrate_embeddings(&lm, yes, false, true)?;
        }

        Commands::Stats { json } => {
            let lm = open_controller(effective_db, global)?;
            let stats = lm.stats()?;
            CliView::render_stats(&stats, lm.db_path(), json)?;
        }

        Commands::Conflicts {
            yes,
            reranker,
            min_similarity,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            if !migrate_embeddings(&lm, false, json, false)? {
                return Ok(());
            }

            if yes || json || !io::stdin().is_terminal() {
                let conflicts =
                    lm.find_conflicts_with_options(yes, min_similarity, reranker.as_deref())?;
                CliView::render_conflicts(&conflicts, yes, lm.db_path(), json)?;
            } else {
                let conflicts =
                    lm.find_conflicts_with_options(false, min_similarity, reranker.as_deref())?;
                if conflicts.is_empty() {
                    CliView::render_conflicts(&conflicts, false, lm.db_path(), false)?;
                    return Ok(());
                }

                let mut retired_ids = std::collections::HashSet::new();
                let mut force_all = false;
                let mut merged_count = 0usize;

                for (i, c) in conflicts.iter().enumerate() {
                    if retired_ids.contains(&c.older_memory.id)
                        || retired_ids.contains(&c.newer_memory.id)
                    {
                        continue;
                    }

                    CliView::render_conflict_pair(i + 1, conflicts.len(), c, false);

                    let should_merge = if force_all {
                        true
                    } else {
                        print!("  Merge these memories? [y]es / [n]o (next) / [a]ll (--yes) / [q]uit: ");
                        io::stdout().flush()?;
                        let mut input = String::new();
                        io::stdin().read_line(&mut input)?;
                        match input.trim().to_lowercase().as_str() {
                            "y" | "yes" => true,
                            "a" | "all" => {
                                force_all = true;
                                true
                            }
                            "q" | "quit" => {
                                println!("  Stopped conflict review ({} merged).", merged_count);
                                break;
                            }
                            _ => false,
                        }
                    };

                    if should_merge {
                        let merged = lm.merge_conflict_pair(c, reranker.as_deref())?;
                        retired_ids.insert(c.older_memory.id.clone());
                        merged_count += 1;
                        println!(
                            "  ✓ Merged into {} (retired older {}): {}\n",
                            &merged.id.to_string()[..8],
                            &c.older_memory.id.to_string()[..8],
                            merged.content
                        );
                    } else {
                        println!("  ↷ Skipped (moving to next).\n");
                    }
                }

                println!("✦ Conflict check complete: {} merged.", merged_count);
            }
        }

        Commands::Link {
            source,
            target,
            relation,
            weight,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let (src_id, dst_id) = lm.link(&source, &target, Some(&relation), Some(weight))?;
            CliView::render_link(&src_id, &dst_id, &relation, weight, json)?;
        }

        Commands::Unlink {
            source,
            target,
            relation,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let removed = lm.unlink(&source, &target, relation.as_deref())?;
            CliView::render_unlink(&source, &target, relation.as_deref(), removed, json)?;
        }

        Commands::Related { id, hops, json } => {
            let lm = open_controller(effective_db, global)?;
            let related = lm.related(&id, hops)?;
            CliView::render_related(&id, &related, json)?;
        }

        Commands::Graph { focus, hops, json } => {
            let lm = open_controller(effective_db, global)?;
            let snapshot = lm.graph(focus.as_deref(), Some(hops))?;
            CliView::render_graph(&snapshot, json)?;
        }

        Commands::Autolink {
            min_similarity,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let new_links = lm.autolink(Some(min_similarity))?;
            let snapshot = lm.graph(None, None)?;
            CliView::render_autolink(new_links, snapshot.nodes.len(), snapshot.edges.len(), json)?;
        }

        Commands::Connect {
            platform,
            workspace,
            path,
            list,
            json,
        } => {
            if list {
                let platforms = lightmem::ConnectService::list_platforms(workspace);
                CliView::render_platforms_list(&platforms, workspace, json)?;
            } else {
                let results = lightmem::ConnectService::connect(
                    platform.as_deref(),
                    workspace,
                    path.as_deref(),
                )?;
                CliView::render_connect(&results, json)?;
            }
        }

        Commands::Completions { shell } => {
            use clap::builder::PossibleValuesParser;
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            cmd = cmd.mut_subcommand("remember", |sub| {
                sub.mut_arg("ttl", |a| {
                    a.value_parser(PossibleValuesParser::new(["1h", "24h", "7d", "30d"]))
                })
            });
            cmd = cmd.mut_subcommand("config", |sub| {
                sub.mut_arg("backend", |a| {
                    a.value_parser(PossibleValuesParser::new(["onnx", "ollama", "hash"]))
                })
                .mut_arg("onnx_model", |a| {
                    a.value_parser(PossibleValuesParser::new(["bge-small", "minilm", "nomic"]))
                })
                .mut_arg("download", |a| {
                    a.value_parser(PossibleValuesParser::new([
                        "bge-small",
                        "minilm",
                        "nomic",
                        "minilm-squad2",
                        "tinyroberta-squad2",
                        "all",
                    ]))
                })
                .mut_arg("reranker", |a| {
                    a.value_parser(PossibleValuesParser::new([
                        "minilm-squad2",
                        "tinyroberta-squad2",
                        "onnx",
                        "ollama",
                        "ollama:qwen2.5:3b",
                        "ollama:qwen2.5:1.5b",
                        "ollama:llama3.2:1b",
                        "top1",
                    ]))
                })
            });
            cmd = cmd.mut_subcommand("answer", |sub| {
                sub.mut_arg("reranker", |a| {
                    a.value_parser(PossibleValuesParser::new([
                        "onnx",
                        "ollama",
                        "top1",
                        "minilm-squad2",
                        "tinyroberta-squad2",
                    ]))
                })
            });
            cmd = cmd.mut_subcommand("conflicts", |sub| {
                sub.mut_arg("reranker", |a| {
                    a.value_parser(PossibleValuesParser::new([
                        "onnx",
                        "ollama",
                        "top1",
                        "minilm-squad2",
                        "tinyroberta-squad2",
                    ]))
                })
            });
            clap_complete::generate(shell, &mut cmd, "lmem", &mut io::stdout());
        }

        Commands::Uninstall { purge_data, yes } => {
            handle_uninstall(purge_data, yes)?;
        }
    }

    Ok(())
}

fn handle_uninstall(purge_data: bool, yes: bool) -> Result<()> {
    println!("\n  \x1b[1;38;2;220;38;38m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M\x1b[0m  \x1b[38;2;113;113;122mUninstaller\x1b[0m");
    println!("  \x1b[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
    println!("  \x1b[1;38;2;220;38;38m▸\x1b[0m \x1b[38;2;228;228;231mRemoving LightMem engine, binaries & shell completions\x1b[0m");
    println!("  \x1b[38;2;113;113;122m────────────────────────────────────────────────\x1b[0m\n");

    if !yes && std::io::stdin().is_terminal() {
        print!("  \x1b[1;33m? Proceed with uninstalling LightMem? [y/N]:\x1b[0m ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        if trimmed != "y" && trimmed != "yes" {
            println!("\n  \x1b[38;2;113;113;122mAborted by user.\x1b[0m\n");
            return Ok(());
        }
        println!();
    }

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    // 1. Clean completion hooks from ~/.zshrc and ~/.bashrc
    clean_rc_file(&home.join(".zshrc"), "lightmem/completions")?;
    clean_rc_file(&home.join(".bashrc"), "lightmem/completions")?;

    // Purge zcompdump cache
    if let Ok(entries) = std::fs::read_dir(&home) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(".zcompdump") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    // 2. Remove completion directory
    let comp_dir = home.join(".lightmem").join("completions");
    if comp_dir.exists() {
        let _ = std::fs::remove_dir_all(&comp_dir);
        println!("  \x1b[1;32m◈\x1b[0m Removed shell completions: ~/.lightmem/completions");
    }

    // 3. Remove standalone binaries
    let bin_candidates = [
        home.join(".local/bin/lmem"),
        home.join(".local/bin/lmem.exe"),
        home.join(".cargo/bin/lmem"),
        home.join(".cargo/bin/lmem.exe"),
        home.join(".lightmem/bin/lmem"),
        home.join(".lightmem/bin/lmem.exe"),
    ];
    let mut removed_count = 0;
    for bin in &bin_candidates {
        if bin.exists() {
            if std::fs::remove_file(bin).is_ok() {
                println!("  \x1b[1;32m◈\x1b[0m Removed binary: {}", bin.display());
                removed_count += 1;
            }
        }
    }
    let lightmem_bin_dir = home.join(".lightmem").join("bin");
    if lightmem_bin_dir.exists() {
        let _ = std::fs::remove_dir_all(&lightmem_bin_dir);
    }
    if removed_count == 0 {
        println!("  \x1b[38;2;113;113;122m▸ No standalone binaries found in ~/.local/bin or ~/.cargo/bin\x1b[0m");
    }

    // 4. Uninstall Python pip package if present
    let pip_cmds: &[&[&str]] = &[
        &["pip3", "uninstall", "-y", "lmem"],
        &["pip", "uninstall", "-y", "lmem"],
        &["python3", "-m", "pip", "uninstall", "-y", "lmem"],
        &["python", "-m", "pip", "uninstall", "-y", "lmem"],
    ];
    for cmd in pip_cmds {
        if let Ok(output) = std::process::Command::new(cmd[0]).args(&cmd[1..]).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.contains("Successfully uninstalled") {
                    println!(
                        "  \x1b[1;32m◈\x1b[0m Uninstalled lmem Python package ({})",
                        cmd[0]
                    );
                    break;
                }
            }
        }
    }

    // 5. Data handling
    let lightmem_dir = home.join(".lightmem");
    if purge_data {
        if lightmem_dir.exists() {
            let _ = std::fs::remove_dir_all(&lightmem_dir);
            println!(
                "  \x1b[1;31m◈\x1b[0m Purged all data & models: {}",
                lightmem_dir.display()
            );
        }
    } else {
        let vault_file = lightmem_dir.join("memories.db");
        if vault_file.exists() {
            println!(
                "\n  \x1b[1;36mℹ\x1b[0m Persistent vault preserved at: {}",
                vault_file.display()
            );
            println!("    (To delete your memories vault as well, pass: --purge-data)");
        }
    }

    // 5. Try to delete own executable if it was not already in bin_candidates
    if let Ok(current_exe) = std::env::current_exe() {
        if current_exe.exists() && !bin_candidates.contains(&current_exe) {
            let _ = std::fs::remove_file(&current_exe);
        }
    }

    println!("\n  \x1b[1;32m✔\x1b[0m LightMem has been successfully uninstalled.");
    println!("    Restart your shell or run 'rehash' to complete.\n");
    Ok(())
}

fn clean_rc_file(path: &Path, pattern: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };
    if !content.contains(pattern) {
        return Ok(());
    }
    let mut cleaned_lines = Vec::new();
    let mut skip = false;
    for line in content.lines() {
        if line.contains("# LightMem CLI completions") {
            skip = true;
            continue;
        }
        if skip {
            if line.contains("compinit")
                || line.contains("menu select")
                || line.contains("lmem.bash")
                || line.contains("menu-complete")
                || line.contains("show-all-if-ambiguous")
                || line.contains("lightmem/completions")
            {
                continue;
            }
            if line.trim().is_empty() {
                skip = false;
                continue;
            }
            skip = false;
        }
        if line.contains(pattern) {
            continue;
        }
        cleaned_lines.push(line);
    }
    let new_content = cleaned_lines.join("\n") + "\n";
    std::fs::write(path, new_content)?;
    println!(
        "  \x1b[1;32m◈\x1b[0m Cleaned completion hooks from {}",
        path.display()
    );
    Ok(())
}
