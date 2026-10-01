use anyhow::Result;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use clap::{Parser, Subcommand};
use colored::*;
use lightmem_core::{LightMem, LightMemConfig, MemoryStatus, MemoryType};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lmem")]
#[command(about = "🧠 LightMem - Ultra-fast, lightweight agent memory engine", long_about = None)]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to custom database file (or via LIGHTMEM_DB env var)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Force use of global database (~/.lightmem/memories.db)
    #[arg(short = 'g', long, global = true)]
    global: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Store a memory principle or fact
    Remember {
        /// Content of the memory
        content: String,

        /// Memory category (fact, decision, instruction, preference, learning, goal, commitment, artifact, event, error)
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

        /// Output results as JSON for agent consumption
        #[arg(long)]
        json: bool,
    },

    /// List memories chronologically
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

        /// Max results
        #[arg(short = 'l', long, default_value = "20")]
        limit: usize,

        /// Output list as JSON
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

    /// Import external memories from file (.json or .okf/.md)
    Import {
        /// Path to import file
        file: PathBuf,
    },

    /// Ask a question and synthesize/extract the factual answer (with toggleable Needle 3 precision reranker)
    Answer {
        /// The question to answer
        question: String,

        /// Toggle: Force use of Needle 3 precision reranker & factual slot extractor
        #[arg(long)]
        needle: bool,

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

        /// Default reranker: 'top1' (0ms instant) or 'needle' (Needle 3 precision SLM)
        #[arg(long)]
        reranker: Option<String>,
    },

    /// Display storage statistics and active database path
    Stats {
        /// Output statistics as JSON
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

fn open_engine(db_path: Option<&std::path::Path>, global: bool) -> Result<LightMem> {
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

    match cli.command {
        Commands::Remember {
            content,
            category,
            title,
            tags,
            confidence,
            json,
        } => {
            let lm = open_engine(effective_db, global)?;
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

            if json {
                println!("{}", serde_json::to_string_pretty(&memory)?);
                return Ok(());
            }

            println!(
                "{} Stored memory [{}] in {}\n  ID: {}\n  Title: {}",
                "✔".green().bold(),
                memory.category.as_str().cyan(),
                lm.db_path().display().to_string().dimmed(),
                memory.id.yellow(),
                memory.title.bold()
            );
        }

        Commands::Recall {
            query,
            category,
            as_of,
            limit,
            min_similarity,
            json,
        } => {
            let lm = open_engine(effective_db, global)?;
            let cat = category.and_then(|c| c.parse::<MemoryType>().ok());
            let as_of_dt = as_of.as_deref().and_then(|s| parse_as_of_date(s).ok());

            let results = lm.recall(&query, cat, as_of_dt, limit, min_similarity)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&results)?);
                return Ok(());
            }

            if results.is_empty() {
                println!("{}", "No relevant memories found.".dimmed());
                return Ok(());
            }

            println!(
                "{} Found {} memories for \"{}\" in {}:",
                "🔍".bold(),
                results.len().to_string().cyan().bold(),
                query.bold(),
                lm.db_path().display().to_string().dimmed()
            );
            println!();

            for (idx, r) in results.iter().enumerate() {
                let m = &r.memory;
                println!(
                    "{} [{}] {} {}",
                    format!("{}.", idx + 1).dimmed(),
                    m.category.as_str().cyan().bold(),
                    m.title.bold(),
                    format!("(score: {:.2})", r.score).dimmed()
                );
                println!("   {}", m.content.replace('\n', "\n   ").dimmed());
                if !m.tags.is_empty() {
                    println!("   {} {}", "Tags:".dimmed(), m.tags.join(", ").blue());
                }
                println!("   {} {}", "ID:".dimmed(), m.id.dimmed());
                println!();
            }
        }

        Commands::List {
            category,
            status,
            as_of,
            limit,
            json,
        } => {
            let lm = open_engine(effective_db, global)?;
            let cat = category.and_then(|c| c.parse::<MemoryType>().ok());
            let st = status.parse::<MemoryStatus>().ok();
            let as_of_dt = as_of.as_deref().and_then(|s| parse_as_of_date(s).ok());

            let memories = lm.list(cat, st, as_of_dt, limit)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&memories)?);
                return Ok(());
            }

            if memories.is_empty() {
                println!("{}", "No memories found.".dimmed());
                return Ok(());
            }

            println!(
                "{} Displaying {} memories from {}:",
                "📋".bold(),
                memories.len().to_string().cyan(),
                lm.db_path().display().to_string().dimmed()
            );
            println!();

            for m in memories {
                println!(
                    "• [{}] {} {}",
                    m.category.as_str().cyan().bold(),
                    m.title.bold(),
                    format!("({})", m.id.chars().take(8).collect::<String>()).dimmed()
                );
                println!("  {}", m.content.lines().next().unwrap_or("").dimmed());
            }
        }

        Commands::Forget { id, hard, json } => {
            let lm = open_engine(effective_db, global)?;
            let ok = lm.forget(&id, hard)?;

            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "id": id,
                        "success": ok,
                        "hard_deleted": hard,
                    })
                );
                return Ok(());
            }

            if ok {
                let action = if hard { "Permanently deleted" } else { "Retired (soft-expired)" };
                println!("{} {} memory {}", "✔".green(), action, id.yellow());
            } else {
                println!("{} Memory ID '{}' not found", "✖".red(), id);
            }
        }

        Commands::Export { okf: _, output } => {
            let lm = open_engine(effective_db, global)?;
            let exported_path = lm.export_okf(output.as_deref())?;
            println!(
                "{} Exported OKF bundle to: {}",
                "✔".green().bold(),
                exported_path.display().to_string().cyan().bold()
            );
        }

        Commands::Import { file } => {
            let lm = open_engine(effective_db, global)?;
            let count = lm.import_file(&file)?;
            println!(
                "{} Successfully imported {} memories from {:?}",
                "✔".green().bold(),
                count.to_string().cyan().bold(),
                file
            );
        }

        Commands::Answer {
            question,
            needle,
            category,
            as_of,
            limit,
            json,
        } => {
            let lm = open_engine(effective_db, global)?;
            let cat = category.and_then(|c| c.parse::<MemoryType>().ok());
            let as_of_dt = as_of.as_deref().and_then(|s| parse_as_of_date(s).ok());

            let result = lm.answer(&question, cat, as_of_dt, limit, needle)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&result)?);
                return Ok(());
            }

            println!(
                "{} Synthesized Answer for \"{}\" [reranker: {}]:",
                "💡".bold(),
                question.bold(),
                result.reranker_used.cyan().bold()
            );
            println!();
            println!("  {}", result.answer.green().bold());
            println!();

            if let Some(m) = &result.selected_memory {
                println!(
                    "  {} Source Memory: [{}] {} {}",
                    "📌".dimmed(),
                    m.category.as_str().cyan(),
                    m.title.bold(),
                    format!("(ID: {})", m.id.chars().take(8).collect::<String>()).dimmed()
                );
                println!("     {}", m.content.replace('\n', "\n     ").dimmed());
            }
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
                if lower == "needle" || lower == "top1" {
                    cfg.reranker = lower;
                    changed = true;
                } else {
                    eprintln!("Invalid reranker '{}'. Choose 'top1' or 'needle'.", r);
                }
            }

            if changed {
                cfg.save()?;
                println!("{} Configuration updated successfully!", "✔".green().bold());
            }

            println!("Current Configuration:");
            println!("  Backend:          {}", cfg.backend.cyan().bold());
            println!(
                "  ONNX Model/Path:  {}",
                cfg.onnx_model.as_deref().unwrap_or("bge-small").cyan()
            );
            println!("  Reranker:         {}", cfg.reranker.cyan().bold());
            println!("  Ollama URL:       {}", cfg.ollama_url.dimmed());
            println!("  Ollama Model:     {}", cfg.embedding_model.dimmed());
            println!("  Config File:      {}", LightMemConfig::config_file().display());
        }

        Commands::Stats { json } => {
            let lm = open_engine(effective_db, global)?;
            let stats = lm.stats()?;

            if json {
                println!("{}", serde_json::to_string_pretty(&stats)?);
                return Ok(());
            }

            println!("{} Storage Statistics", "📊".bold());
            println!("  Database:          {}", lm.db_path().display().to_string().cyan());
            println!("  Total Memories:    {}", stats.total_memories.to_string().bold());
            println!("  Active Memories:   {}", stats.active_memories.to_string().green());
            println!("  Expired Memories:  {}", stats.expired_memories.to_string().yellow());
            println!("  Embedded Vectors:  {}", stats.total_vectors.to_string().blue());
            println!();
            println!("  Breakdown by Category:");
            for (cat, count) in stats.by_category {
                println!("    • {:<14} {}", cat.cyan(), count);
            }
        }
    }

    Ok(())
}
