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

        /// Automatically supersede (soft-retire) older conflicting memories on the same subject & slot
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
        category: Option<MemoryType>,

        /// Filter by status (active | expired)
        #[arg(long, default_value = "active")]
        status: MemoryStatus,

        /// Point-in-time view (YYYY-MM-DD)
        #[arg(long, value_parser = parse_as_of_date)]
        as_of: Option<DateTime<Utc>>,

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

        /// Opt in to native Needle title/category enrichment
        #[arg(long)]
        enrich: bool,
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
        category: Option<MemoryType>,

        /// Point-in-time view (YYYY-MM-DD or RFC3339)
        #[arg(long, value_parser = parse_as_of_date)]
        as_of: Option<DateTime<Utc>>,

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

        /// Pre-download ONNX model(s) into ~/.lightmem/models ('bge-small', 'minilm', 'nomic', or 'all')
        #[arg(long)]
        download: Option<String>,

        /// Ollama server URL (e.g. http://localhost:11434)
        #[arg(long)]
        url: Option<String>,

        /// Ollama embedding model name (e.g. nomic-embed-text)
        #[arg(long)]
        model: Option<String>,

        /// Default reranker: 'top1' (0ms instant) or 'needle' (Native Needle 3 C-FFI)
        #[arg(long)]
        reranker: Option<String>,

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

    /// Smart-merge duplicate memories (union tags, keep max confidence & earliest created_at) and remove redundant copies
    Dedup {
        /// Output deduplication result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Detect contradictory active memories on the same subject & slot (and optionally retire older ones)
    Conflicts {
        /// Automatically soft-retire older conflicting memories so only the latest truth remains active
        #[arg(long)]
        resolve: bool,

        /// Output conflict report as JSON
        #[arg(long)]
        json: bool,
    },

    /// Generate shell completion scripts (zsh, bash, fish) with interactive tab/arrow navigation
    Completions {
        /// Target shell (zsh, bash, fish, elvish, powershell)
        shell: clap_complete::Shell,
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
        let config = LightMemConfig::try_load()?;
        LightMem::open_at(path, config)
    } else {
        LightMem::open_default(global)
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
            limit,
            min_similarity,
            precision,
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
                    lm.answer_with_reranker(&query, cat, as_of_dt, limit, Some("needle"))?;
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
            let cat = category;
            let st = Some(status);
            let as_of_dt = as_of;

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
            limit,
            json,
        } => {
            let lm = open_controller(effective_db, global)?;
            let cat = category;
            let as_of_dt = as_of;

            if !migrate_embeddings(&lm, false, json, false)? {
                return Ok(());
            }
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
            download,
            url,
            model,
            reranker,
            yes,
        } => {
            if let Some(ref dl) = download {
                let targets: Vec<&str> = match dl.to_lowercase().as_str() {
                    "all" => vec!["bge-small", "minilm", "nomic"],
                    "bge-small" | "bge-small-en-v1.5" | "xenova/bge-small-en-v1.5" => {
                        vec!["bge-small"]
                    }
                    "minilm" | "all-minilm-l6-v2" | "xenova/all-minilm-l6-v2" => vec!["minilm"],
                    "nomic" | "nomic-embed-text" | "nomic-ai/nomic-embed-text-v1.5" => {
                        vec!["nomic"]
                    }
                    other => anyhow::bail!(
                        "Unknown model '{}' for --download. Choose 'bge-small', 'minilm', 'nomic', or 'all'.",
                        other
                    ),
                };
                for m in targets {
                    let _ = lightmem::OnnxEmbeddingProvider::new(Some(m))?;
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
                let lower = r.to_lowercase();
                if lower == "needle" || lower == "precision" || lower == "top1" {
                    cfg.reranker = lower;
                    changed = true;
                } else {
                    anyhow::bail!("Invalid reranker '{}'. Choose 'top1' or 'needle'.", r);
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

        Commands::Dedup { json } => {
            let lm = open_controller(effective_db, global)?;
            if !migrate_embeddings(&lm, false, json, false)? {
                return Ok(());
            }
            let merged = lm.deduplicate()?;
            CliView::render_dedup(merged, lm.db_path(), json)?;
        }

        Commands::Conflicts { resolve, json } => {
            let lm = open_controller(effective_db, global)?;
            let conflicts = lm.find_conflicts(resolve)?;
            CliView::render_conflicts(&conflicts, resolve, lm.db_path(), json)?;
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
                        "all",
                    ]))
                })
                .mut_arg("reranker", |a| {
                    a.value_parser(PossibleValuesParser::new(["needle", "top1"]))
                })
            });
            cmd = cmd.mut_subcommand("answer", |sub| {
                sub.mut_arg("reranker", |a| {
                    a.value_parser(PossibleValuesParser::new(["needle", "top1"]))
                })
            });
            clap_complete::generate(shell, &mut cmd, "lmem", &mut io::stdout());
        }
    }

    Ok(())
}
