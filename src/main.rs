use anyhow::Result;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use clap::{Parser, Subcommand};
use lightmem::{CliView, LightMem, LightMemConfig, MemoryStatus, MemoryType};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "lmem")]
#[command(
    about = "❖ LightMem - Ultra-fast, lightweight agent memory engine",
    long_about = None,
    before_help = CliView::banner_string(None, None)
)]
#[command(version = "0.1.0")]
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
        category: Option<String>,

        /// Short title (defaults to first line)
        #[arg(long)]
        title: Option<String>,

        /// Comma-separated tags
        #[arg(long)]
        tags: Option<String>,

        /// Confidence score (0.0 to 1.0)
        #[arg(short = 'c', long, default_value = "0.9")]
        confidence: f32,

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
        category: Option<String>,

        /// Point-in-time recall: what was active as of date (YYYY-MM-DD or RFC3339)
        #[arg(long)]
        as_of: Option<String>,

        /// Max results to return
        #[arg(short = 'l', long, default_value = "10")]
        limit: usize,

        /// Minimum similarity threshold (0.0 - 1.0)
        #[arg(long)]
        min_similarity: Option<f32>,

        /// Toggle: Run Needle 3 precision reranker & factual slot extraction on recalled memories
        #[arg(long, alias = "needle")]
        precision: bool,

        /// Output results as JSON for agent consumption
        #[arg(long)]
        json: bool,
    },

    /// List memories chronologically with pagination (limit, offset, page, total)
    List {
        /// Filter by category
        #[arg(short = 't', long = "type")]
        category: Option<String>,

        /// Filter by status (active | expired)
        #[arg(long, default_value = "active")]
        status: String,

        /// Point-in-time view (YYYY-MM-DD)
        #[arg(long)]
        as_of: Option<String>,

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

    /// Forget or expire a memory
    Forget {
        /// Memory ID to forget
        id: String,

        /// Permanently delete instead of soft-retiring
        #[arg(long)]
        hard: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Export memories to Open Knowledge Format (OKF)
    Export {
        /// Export to Open Knowledge Format bundle
        #[arg(long)]
        okf: bool,

        /// Output file path
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },

    /// Import external memories from file (.json, .jsonl, or .okf/.md) or directory
    Import {
        /// Path to import file or OKF directory
        file: PathBuf,
    },

    /// Ask a question and synthesize/extract the factual answer (with toggleable Native Needle 3 C-FFI reranker)
    Answer {
        /// The question to answer
        question: String,

        /// Toggle: Force use of Needle 3 precision reranker & factual slot extractor
        #[arg(long, alias = "needle")]
        precision: bool,

        /// Override reranker mode for this query ("top1" or "needle")
        #[arg(short = 'r', long)]
        reranker: Option<String>,

        /// Filter candidate memories by category
        #[arg(short = 't', long = "type")]
        category: Option<String>,

        /// Point-in-time view (YYYY-MM-DD or RFC3339)
        #[arg(long)]
        as_of: Option<String>,

        /// Max candidate memories to retrieve for reranking
        #[arg(short = 'l', long, default_value = "5")]
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

        /// ONNX model name ('bge-small', 'minilm', 'nomic') or path to directory with custom model.onnx
        #[arg(long)]
        onnx_model: Option<String>,

        /// Ollama server URL (e.g. http://100.75.149.115:7777)
        #[arg(long)]
        url: Option<String>,

        /// Ollama embedding model name (e.g. nomic-embed-text)
        #[arg(long)]
        model: Option<String>,

        /// Default reranker: 'top1' (0ms instant) or 'needle' (Native Needle 3 C-FFI)
        #[arg(long)]
        reranker: Option<String>,
    },

    /// Display storage statistics and active database path
    Stats {
        /// Output statistics as JSON
        #[arg(long)]
        json: bool,
    },

    /// Smart-merge duplicate memories (union tags, keep max confidence & earliest created_at) and remove redundant copies
    Dedup {
        /// Output deduplication result as JSON
        #[arg(long)]
        json: bool,
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

fn open_controller(db_path: Option<&Path>, global: bool) -> Result<LightMem> {
    if let Some(path) = db_path {
        let config = LightMemConfig::load();
        LightMem::open_at(path, config)
    } else {
        LightMem::open_default(global)
    }
}

fn main() -> Result<()> {
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
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category.and_then(|c| c.parse::<MemoryType>().ok());
            let tag_vec = tags
                .map(|t| {
                    t.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();

            let memory = lm.remember(&content, cat, title, tag_vec, Some(confidence))?;
            CliView::render_remembered(&memory, lm.db_path(), json)?;
        }

        Commands::Recall {
            query,
            category,
            as_of,
            limit,
            min_similarity,
            precision,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category.and_then(|c| c.parse::<MemoryType>().ok());
            let as_of_dt = as_of.as_deref().and_then(|s| parse_as_of_date(s).ok());

            if precision {
                let result = lm.answer_with_reranker(&query, cat, as_of_dt, limit, Some("needle"))?;
                CliView::render_answer(&result, json)?;
            } else {
                let results = lm.recall(&query, cat, as_of_dt, limit, min_similarity)?;
                CliView::render_recall(&query, &results, lm.db_path(), json)?;
            }
        }

        Commands::List {
            category,
            status,
            as_of,
            limit,
            offset,
            page,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category.and_then(|c| c.parse::<MemoryType>().ok());
            let st = status.parse::<MemoryStatus>().ok();
            let as_of_dt = as_of.as_deref().and_then(|s| parse_as_of_date(s).ok());

            let paginated = if let Some(p) = page {
                lm.list_page(cat, st, as_of_dt, p, limit)?
            } else {
                lm.list_paginated(cat, st, as_of_dt, limit, offset)?
            };
            CliView::render_paginated_list(&paginated, lm.db_path(), json)?;
        }

        Commands::Forget { id, hard, json } => {
            let lm = open_controller(effective_db, global)?;
            let ok = lm.forget(&id, hard)?;
            CliView::render_forget(&id, ok, hard, json)?;
        }

        Commands::Export { okf: _, output } => {
            let lm = open_controller(effective_db, global)?;
            let exported_path = lm.export_okf(output.as_deref())?;
            CliView::render_export(&exported_path);
        }

        Commands::Import { file } => {
            let lm = open_controller(effective_db, global)?;
            let count = lm.import_file(&file)?;
            CliView::render_import(count, &file);
        }

        Commands::Answer {
            question,
            precision,
            reranker,
            category,
            as_of,
            limit,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category.and_then(|c| c.parse::<MemoryType>().ok());
            let as_of_dt = as_of.as_deref().and_then(|s| parse_as_of_date(s).ok());

            let override_mode = if precision {
                Some("needle")
            } else {
                reranker.as_deref()
            };

            let result = lm.answer_with_reranker(&question, cat, as_of_dt, limit, override_mode)?;
            CliView::render_answer(&result, json)?;
        }

        Commands::Config {
            backend,
            onnx_model,
            url,
            model,
            reranker,
        } => {
            let mut cfg = LightMemConfig::load();
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
                let lower = r.to_lowercase();
                if lower == "needle" || lower == "precision" || lower == "top1" {
                    cfg.reranker = lower;
                    changed = true;
                } else {
                    eprintln!("Invalid reranker '{}'. Choose 'top1' or 'needle'.", r);
                }
            }

            if changed {
                cfg.save()?;
            }
            CliView::render_config(&cfg, changed);
        }

        Commands::Stats { json } => {
            let lm = open_controller(effective_db, global)?;
            let stats = lm.stats()?;
            CliView::render_stats(&stats, lm.db_path(), json)?;
        }

        Commands::Dedup { json } => {
            let lm = open_controller(effective_db, global)?;
            let merged = lm.deduplicate()?;
            CliView::render_dedup(merged, lm.db_path(), json)?;
        }
    }

    Ok(())
}
