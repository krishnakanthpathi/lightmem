mod terminal;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use clap::{Parser, Subcommand};
use colored::*;
use lightmem::{
    encode_base64, parse_ttl_duration, CliErrorCard, CliView, HistoryService, LightMem,
    LightMemConfig, MemoryStatus, MemoryType, MultimodalInput,
};
use serde::{Deserialize, Serialize};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "lmem")]
#[command(
    about = "❖ LightMem - Ultra-fast, lightweight agent memory engine",
    long_about = None,
    before_help = CliView::banner_string(None, None)
)]
#[command(disable_version_flag = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Force use of global database (~/.lightmem/memories.db)
    #[arg(short = 'g', long, global = true)]
    global: bool,

    /// Registered user/agent/harness connection (or LIGHTMEM_CONNECTION)
    #[arg(long, global = true)]
    connection: Option<String>,

    /// Override the connection scope: user, project, or agent
    #[arg(long, global = true)]
    scope: Option<lightmem::MemoryScope>,

    /// Source conversation/session ID for new memories
    #[arg(long, global = true)]
    session: Option<String>,

    /// Explicit path to SQLite database file
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Remote LightMem backend server URL (e.g. http://192.168.1.100:8080)
    #[arg(long, global = true)]
    remote: Option<String>,

    /// Secret API key for remote LightMem backend authentication
    #[arg(long, global = true)]
    api_key: Option<String>,

    /// Print version information (use with --json for machine-readable JSON)
    #[arg(short = 'V', long = "version")]
    version: bool,

    /// Output as JSON when used with top-level flags like --version
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// View all managed memory vaults, active connections, and storage telemetry
    #[command(visible_alias = "vaults")]
    Status {
        /// Output vault status as JSON
        #[arg(long)]
        json: bool,
    },
    /// Register and inspect users, agents, harnesses, and named connections
    Identity {
        #[command(subcommand)]
        action: IdentityCommand,
        #[arg(long, global = true)]
        json: bool,
    },
    /// Show the active connection, scope, source identity, and database
    Context {
        #[arg(long)]
        json: bool,
    },
    /// Store a memory principle, fact, or attached file/image/audio/document
    Remember {
        /// Content of the memory (optional when attaching files via -f/--file)
        content: Option<String>,

        /// Attach one or more files, images, audio clips, or documents as isolated SQLite BLOBs with metadata
        #[arg(short = 'f', long = "file", visible_alias = "attach")]
        files: Vec<PathBuf>,

        /// Explicitly invoke the configured LLM to extract text/descriptions from attached files (off by default; metadata-only unless specified)
        #[arg(short = 'e', long = "extract")]
        extract: bool,

        /// Optional LLM model override when --extract is enabled (e.g. 'ollama:gemma4:31b-cloud')
        #[arg(short = 'r', long = "reranker")]
        reranker: Option<String>,

        /// Memory category (fact, decision, instruction, preference, learning, goal, commitment, artifact, event, relationship, observation, error, context, password)
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Short title (defaults to first line or filename)
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

        /// Origin or verification level (explicit_statement, validated, observed, corrected, inferred, imported, attachment_metadata, attachment_llm_extracted)
        #[arg(short = 'p', long)]
        provenance: Option<String>,

        /// Skip entity extraction even when explicitly enabled in configuration
        #[arg(long = "no-entities", visible_alias = "no-extract-entities")]
        no_entities: bool,

        /// Output created memory as JSON
        #[arg(long)]
        json: bool,
    },

    /// Recall memories via hybrid semantic + keyword search
    Recall {
        /// Search query string (optional if --image is supplied)
        #[arg(default_value = "")]
        query: String,

        /// Search multimodal vector memory with an image query
        #[arg(long = "image", visible_alias = "img")]
        image: Option<PathBuf>,

        /// Filter by category
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Filter by attachment media kind (image, audio, video, document, code, archive, binary, or any)
        #[arg(short = 'm', long = "media")]
        media: Option<String>,

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

        /// Export verified attachment binary file(s) of recalled memories to a target directory or file path
        #[arg(short = 'o', long = "save-to")]
        save_to: Option<PathBuf>,

        /// Materialize verified attachment(s) and print only their absolute file path(s) (one per line)
        #[arg(short = 'p', long = "path", visible_alias = "paths")]
        path: bool,

        /// Open recalled attachment file(s) immediately using system default application
        #[arg(long)]
        open: bool,

        /// Output results as JSON for agent consumption
        #[arg(long)]
        json: bool,
    },

    /// List memories chronologically with pagination (limit, offset, page, total)
    List {
        /// Filter by category
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Filter by attachment media kind (image, audio, video, document, code, archive, binary, or any)
        #[arg(short = 'm', long = "media")]
        media: Option<String>,

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
        #[arg(long)]
        page: Option<usize>,

        /// Export verified attachment binary file(s) of listed memories to a target directory or file path
        #[arg(short = 'o', long = "save-to")]
        save_to: Option<PathBuf>,

        /// Materialize verified attachment(s) and print only their absolute file path(s) (one per line)
        #[arg(short = 'p', long = "path", visible_alias = "paths")]
        path: bool,

        /// Output paginated result as JSON (includes total, limit, offset, page, total_pages, has_more, items)
        #[arg(long)]
        json: bool,
    },

    /// Deeply inspect one or more memory records, their metadata, attachments, and knowledge graph links
    #[command(alias = "get", alias = "show")]
    Inspect {
        /// Memory ID(s) (UUID or prefix) or title(s)
        #[arg(required = true, num_args = 1..)]
        terms: Vec<String>,

        /// Export verified attachment binary file(s) from the SQLite vault to a target file or directory path on disk
        #[arg(short = 'o', long = "save-to", visible_alias = "export-attachments")]
        save_to: Option<PathBuf>,

        /// Materialize verified attachment(s) and print only their absolute file path(s) (one per line)
        #[arg(short = 'p', long = "path", visible_alias = "paths")]
        path: bool,

        /// Open inspected attachment file(s) immediately using system default application
        #[arg(long)]
        open: bool,

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

    /// Restore an archived memory by ID, prefix, or exact title (clears its old TTL)
    Restore {
        term: String,
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

    /// Export memories to Open Knowledge Format (OKF), lossless JSON backup, or compressed DB transfer bundle (.zip)
    Export {
        /// Export to Open Knowledge Format bundle
        #[arg(long)]
        okf: bool,

        /// Lossless JSON backup format including expired memories and links
        #[arg(long, conflicts_with_all = ["okf", "bundle"])]
        json: bool,

        /// Export full database transfer bundle (.zip with DB, all video/photo files, embeddings, and manifest)
        #[arg(long, conflicts_with_all = ["okf", "json"])]
        bundle: bool,

        /// Output file path
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,

        /// Emit operation summary as machine-readable JSON
        #[arg(long = "output-json", visible_alias = "print-json")]
        output_json: bool,
    },

    /// Import external memories from file (.json, .jsonl, .okf/.md, or .zip transfer bundle) or directory
    Import {
        /// Path to import file, OKF directory, or .zip transfer bundle
        file: PathBuf,

        /// Opt in to heuristic title/category enrichment
        #[arg(long)]
        enrich: bool,

        /// Explicitly allow overwriting/replacing existing memories with matching IDs
        #[arg(long)]
        replace: bool,

        /// Merge bundle into existing memories instead of requiring an empty vault or complete replacement
        #[arg(long)]
        merge: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Manage portable compressed transfer bundles (.zip) with DB, video/photo files, and embeddings
    Bundle {
        #[command(subcommand)]
        action: BundleCommand,
        #[arg(long, global = true)]
        json: bool,
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

    /// Inspect local configuration and index readiness without downloads or writes
    Doctor {
        #[arg(long)]
        json: bool,
    },

    #[command(hide = true, name = "__connections")]
    CompletionConnections,

    /// View shared settings, preview changes, or test the saved embedding backend
    Config {
        /// Reload configuration from disk and display updated settings
        #[arg(long, visible_alias = "refresh")]
        reload: bool,

        /// Optional sub-action such as 'reload' or 'refresh'
        #[arg(index = 1)]
        action: Option<String>,

        /// Change a setting using short terminal prompts; no full-screen interface
        #[arg(long, conflicts_with_all = ["json", "yes", "preset", "backend", "onnx_model", "download", "url", "model", "reranker", "entity_extraction", "ollama_api_key", "reset_db", "test", "remote", "api_key", "clear_remote", "dry_run", "reload"])]
        edit: bool,
        /// Set backend engine: 'onnx', 'ollama', or 'hash'
        #[arg(long, value_parser = ["onnx", "ollama", "hash"])]
        backend: Option<String>,

        /// ONNX embedding model name ('bge-small', 'minilm', 'nomic') or path to directory with custom model.onnx
        #[arg(long)]
        onnx_model: Option<String>,

        /// Pre-download ONNX model(s) into ~/.lightmem/models ('bge-small', 'minilm', 'nomic', 'minilm-squad2', 'tinyroberta-squad2', 'qa', or 'all')
        #[arg(long)]
        download: Option<String>,

        /// OpenAI-compatible server URL or endpoint (e.g. http://localhost:11434, https://api.openai.com)
        #[arg(long, visible_alias = "endpoint")]
        url: Option<String>,

        /// Setup preset: demo (no downloads), local (ONNX), ollama-cloud, or ollama-local
        #[arg(long, visible_alias = "profile")]
        preset: Option<String>,

        /// Ollama vector embedding model name (explicit alias: --embedding-model; for the answer/generative LLM use --reranker or --answer-model)
        #[arg(long, visible_alias = "embedding-model")]
        model: Option<String>,

        /// OpenAI-compatible chat model for answers and entity extraction (e.g. 'qwen2.5-coder:1.5b', 'gpt-4o-mini', 'ollama:qwen2.5:3b')
        #[arg(
            long,
            visible_alias = "answer-model",
            visible_alias = "openai-model",
            visible_alias = "chat-model"
        )]
        reranker: Option<String>,

        /// Toggle entity extraction ('on' or 'off') using the configured generative model
        #[arg(long)]
        entity_extraction: Option<String>,

        /// Save an OpenAI / LLM endpoint key; use 'none' to remove it (environment keys still apply)
        #[arg(
            long,
            visible_alias = "model-api-key",
            visible_alias = "openai-api-key"
        )]
        ollama_api_key: Option<String>,

        /// Preview proposed settings and migration requirements without writes or downloads
        #[arg(long, conflicts_with_all = ["download", "reset_db", "test"])]
        dry_run: bool,

        /// Test saved embeddings with disposable text; no memory is saved (may download ONNX assets)
        #[arg(long, conflicts_with_all = ["preset", "backend", "onnx_model", "download", "url", "model", "reranker", "entity_extraction", "ollama_api_key", "reset_db", "remote", "api_key", "clear_remote", "reload"])]
        test: bool,

        /// Permanently delete the selected local vault; requires --yes
        #[arg(long, requires = "yes", conflicts_with_all = ["preset", "backend", "onnx_model", "download", "url", "model", "reranker", "entity_extraction", "ollama_api_key", "remote", "api_key", "clear_remote", "reload"])]
        reset_db: bool,

        /// Approve migration of the selected database when the embedding model changes
        #[arg(long)]
        yes: bool,

        /// Remote LightMem backend server URL (e.g. http://my-server:8080)
        #[arg(long, visible_alias = "remote-url")]
        remote: Option<String>,

        /// Secret API key for authenticating with the remote server
        #[arg(long)]
        api_key: Option<String>,

        /// Clear saved remote server configuration and revert to local mode
        #[arg(long)]
        clear_remote: bool,

        /// Output configuration as JSON
        #[arg(long)]
        json: bool,
    },

    /// Guided terminal setup with a configuration preview before saving
    Setup {
        /// Setup preset: demo (no downloads), local (ONNX), ollama-local, or ollama-cloud
        #[arg(long)]
        preset: Option<String>,

        /// OpenAI / Ollama server URL (e.g. http://localhost:11434)
        #[arg(long, visible_alias = "endpoint")]
        url: Option<String>,

        /// Answer engine spec (e.g. ollama:qwen2.5:3b, minilm-squad2, or top1)
        #[arg(long, visible_alias = "answer-model")]
        model: Option<String>,

        /// Override entity extraction toggle ('on' or 'off') independently of the preset
        #[arg(long)]
        entity_extraction: Option<String>,

        /// Preview setup without saving, downloading, or reindexing
        #[arg(long)]
        dry_run: bool,

        /// Approve prompts without confirmation
        #[arg(short = 'y', long)]
        yes: bool,

        /// Output configuration as JSON
        #[arg(long)]
        json: bool,
    },

    /// Rebuild embeddings for the configured model, with progress
    Reindex {
        /// Explicitly approve migration without an interactive prompt
        #[arg(long)]
        yes: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
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

    /// Ingest conversation or notes, extract atomic candidate memories, and confirm before saving
    #[command(alias = "observed", alias = "extract")]
    Observe {
        /// Conversation text, note content, or path to text/markdown/jsonl file (reads from stdin if omitted or '-')
        input: Option<String>,

        /// Path to a conversation or transcript file to observe
        #[arg(short = 'f', long)]
        file: Option<PathBuf>,

        /// Save all extracted memories without interactive confirmation prompt
        #[arg(short = 'y', long, visible_alias = "resolve")]
        yes: bool,

        /// Dry run: extract and display candidate memories without saving to the database
        #[arg(long)]
        dry_run: bool,

        /// Override category for all extracted memories
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Comma-separated tags to attach to all extracted memories
        #[arg(long)]
        tags: Option<String>,

        /// Minimum confidence threshold for extracted facts (0.0 to 1.0)
        #[arg(short = 'c', long, default_value = "0.85")]
        confidence: f32,

        /// Override reranker/extractor model (e.g. 'ollama:qwen2.5:3b')
        #[arg(short = 'r', long)]
        reranker: Option<String>,

        /// Output extracted observations as JSON
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

        /// Configure Model Context Protocol (MCP) server across detected client configs (Claude, Cursor, Antigravity)
        #[arg(long)]
        mcp: bool,
    },

    #[cfg(feature = "mcp")]
    /// Run the native Model Context Protocol (MCP) stdio JSON-RPC 2.0 server
    Mcp,

    /// Generate shell completion scripts (zsh, bash, fish) with interactive tab/arrow navigation
    Completions {
        /// Target shell (zsh, bash, fish, elvish, powershell)
        shell: clap_complete::Shell,
    },

    /// Display version information and build metadata
    Version {
        /// Output version information as JSON
        #[arg(long)]
        json: bool,
    },

    /// Launch an embedded local documentation and landing page web server (zero external processes)
    #[command(visible_aliases = ["doc", "web"])]
    Docs {
        /// Port to bind the local HTTP server (defaults to auto-selecting 8080..8100)
        #[arg(short = 'p', long)]
        port: Option<u16>,

        /// Open the landing page instead of the documentation page
        #[arg(long)]
        landing: bool,

        /// Do not automatically open browser
        #[arg(long)]
        no_browser: bool,
    },

    /// Switch to local mode (clears saved remote URL and API key)
    #[command(visible_alias = "disconnect")]
    Local {
        /// Output status as JSON
        #[arg(long)]
        json: bool,
    },

    /// Connect to a remote LightMem server or switch to remote mode
    Remote {
        /// Remote LightMem backend server URL (e.g. http://localhost:8080)
        url: Option<String>,

        /// Secret API key for remote server authentication
        #[arg(long)]
        api_key: Option<String>,

        /// Output status as JSON
        #[arg(long)]
        json: bool,
    },

    /// Launch a persistent HTTP backend server for remote multi-device memory access
    #[command(visible_aliases = ["server"])]
    Serve {
        /// Host interface to bind (defaults to 0.0.0.0 for LAN/remote access)
        #[arg(long, default_value = "0.0.0.0")]
        host: String,

        /// Port to listen on
        #[arg(short = 'p', long, default_value = "8080")]
        port: u16,

        /// Secret API key required for client requests (auto-generated if omitted)
        #[arg(long)]
        api_key: Option<String>,

        /// Run server without requiring authentication (development only)
        #[arg(long)]
        no_auth: bool,

        /// Run backend server in background as a daemon
        #[arg(short = 'd', long)]
        daemon: bool,

        /// Stop running background daemon server
        #[arg(long)]
        stop: bool,

        /// Check status of running background daemon server
        #[arg(long)]
        status: bool,
    },

    /// Uninstall LightMem binaries, shell completions, and optionally purge data
    Uninstall {
        /// Purge database (~/.lightmem/memories.db) and downloaded models
        #[arg(short = 'a', long = "purge-data")]
        purge_data: bool,

        /// Non-interactive mode (proceed without confirmation)
        #[arg(short = 'y', long = "yes")]
        yes: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// View or clear the history of executed CLI commands
    #[command(visible_alias = "hist")]
    History {
        /// Optional search query or subcommand filter (e.g. 'recall', 'remember', or keywords)
        #[arg(index = 1)]
        query: Option<String>,

        /// Maximum number of history entries to display
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,

        /// Filter commands by status: 'success' (ok) or 'error' (failed)
        #[arg(short = 's', long)]
        status: Option<String>,

        /// Clear all recorded command history
        #[arg(long)]
        clear: bool,

        /// Output results as JSON
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

#[derive(Subcommand, Debug)]
enum BundleCommand {
    /// Export active vault into a single compressed transfer .zip file containing the database and all video/photo media files
    Export {
        /// Output file path (.zip or .lmbundle)
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Import and unpack a transfer .zip file, restoring the database and extracting all video/photo files
    Import {
        /// Path to .zip transfer bundle
        file: PathBuf,

        /// Overwrite/replace the entire existing vault with the transfer bundle
        #[arg(long)]
        replace: bool,

        /// Merge bundle into existing memories instead of replacing
        #[arg(long)]
        merge: bool,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },

    /// Inspect a transfer .zip file's manifest and video/photo media summary without unpacking
    Info {
        /// Path to .zip transfer bundle
        file: PathBuf,

        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum IdentityCommand {
    UserAdd {
        id: String,
        #[arg(long)]
        name: Option<String>,
    },
    AgentAdd {
        id: String,
        #[arg(long)]
        user: String,
        #[arg(long)]
        name: Option<String>,
    },
    HarnessAdd {
        id: String,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
    },
    ConnectionAdd {
        id: String,
        #[arg(long)]
        user: String,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        harness: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value = "user")]
        memory_scope: lightmem::MemoryScope,
    },
    List {
        #[arg(default_value = "connections")]
        kind: String,
    },
    Show {
        id: String,
    },
    /// View all managed memory vaults, active connections, and storage telemetry
    Status,
}

fn run_identity(action: &IdentityCommand, json: bool, default_db: PathBuf) -> Result<()> {
    let registry = lightmem::IdentityRegistry::open_default()?;
    match action {
        IdentityCommand::Status => {
            let overview = registry.status_overview(default_db)?;
            CliView::render_vault_status_overview(&overview, json)?;
        }
        IdentityCommand::UserAdd { id, name } => {
            let value = serde_json::to_value(registry.add_user(id, name.as_deref())?)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        IdentityCommand::AgentAdd { id, user, name } => {
            let value = serde_json::to_value(registry.add_agent(id, user, name.as_deref())?)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        IdentityCommand::HarnessAdd { id, path, name } => {
            let value =
                serde_json::to_value(registry.add_harness(id, path.clone(), name.as_deref())?)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        IdentityCommand::ConnectionAdd {
            id,
            user,
            agent,
            harness,
            project,
            memory_scope,
        } => {
            let value = serde_json::to_value(registry.add_connection(
                id,
                user,
                agent,
                harness,
                project.clone(),
                memory_scope.clone(),
            )?)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        IdentityCommand::List { kind } => {
            let value = registry.list(kind)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        IdentityCommand::Show { id } => {
            let value = serde_json::to_value(registry.resolve(id, None, None)?)?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
    }
    Ok(())
}

fn handle_remote_command(
    client: &lightmem::RemoteClient,
    command: &Commands,
    json: bool,
) -> Result<bool> {
    match command {
        Commands::Remember {
            content,
            files,
            extract,
            reranker,
            category,
            title,
            tags,
            confidence,
            ttl,
            supersede,
            provenance,
            no_entities,
            json: cmd_json,
        } => {
            anyhow::ensure!(!extract && reranker.is_none(), "Remote remember does not support --extract or --reranker. Configure extraction on the server or omit these options.");
            let mut attachment_payloads = Vec::new();
            for fpath in files {
                let data = std::fs::read(fpath).with_context(|| {
                    format!("Failed to read attachment file: {}", fpath.display())
                })?;
                let fname = fpath
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                let (mime, _) = lightmem::detect_mime_and_kind(&fname, &data);
                let data_base64 = lightmem::encode_base64(&data);
                attachment_payloads.push(lightmem::AttachmentPayload {
                    name: fname,
                    mime: Some(mime),
                    data_base64,
                });
            }
            let req = lightmem::RememberRequest {
                content: content.clone(),
                category: *category,
                tags: tags
                    .as_ref()
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
                source: provenance.clone(),
                title: title.clone(),
                ttl: ttl.map(|d| format!("{}ms", d.num_milliseconds())),
                confidence: Some(*confidence),
                supersede: *supersede,
                no_entities: Some(*no_entities),
                attachments: if attachment_payloads.is_empty() {
                    None
                } else {
                    Some(attachment_payloads)
                },
            };
            let response = client.remember_detailed(&req)?;
            if *cmd_json || json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                CliView::render_remembered(&response.memory, Path::new(&client.base_url), false)?;
                for warning in response.warnings {
                    eprintln!("Warning: {}", warning);
                }
            }
            Ok(true)
        }

        Commands::Recall {
            query,
            image,
            category,
            media,
            as_of,
            date,
            limit,
            min_similarity,
            precision,
            multi_hop,
            save_to,
            path,
            open,
            json: cmd_json,
        } => {
            anyhow::ensure!(image.is_none() && save_to.is_none() && !path && !open, "Remote recall does not support --image, --save-to, --path, or --open. Use local mode to download attachments.");
            if *precision {
                anyhow::ensure!(media.is_none() && !multi_hop && min_similarity.is_none(), "Precision recall returns an answer; media, multi-hop, and similarity filters are not supported. Use recall without --precision for filtered results.");
                let answer = match client.answer(&lightmem::AnswerRequest {
                    question: query.clone(),
                    category: *category,
                    as_of: *as_of,
                    date: *date,
                    limit: *limit,
                    ..Default::default()
                }) {
                    Ok(ans) => ans,
                    Err(err) => {
                        CliView::render_llm_or_error(&err, *cmd_json || json);
                        return Err(AlreadyHandledError.into());
                    }
                };
                CliView::render_answer(&answer, *cmd_json || json)?;
                return Ok(true);
            }
            let req = lightmem::RecallRequest {
                query: query.clone(),
                category: *category,
                media: media.clone(),
                as_of: *as_of,
                date: *date,
                limit: *limit,
                min_similarity: *min_similarity,
                precision: *precision,
                multi_hop: *multi_hop,
            };
            let results = client.recall(&req)?;
            if *cmd_json || json {
                println!("{}", serde_json::to_string_pretty(&results)?);
            } else {
                CliView::render_recall(query, &results, Path::new(&client.base_url), false)?;
            }
            Ok(true)
        }

        Commands::Answer {
            question,
            precision,
            reranker,
            category,
            as_of,
            date,
            limit,
            json: cmd_json,
        } => {
            let req = lightmem::AnswerRequest {
                question: question.clone(),
                precision: *precision,
                reranker: reranker.clone(),
                category: *category,
                as_of: *as_of,
                date: *date,
                limit: *limit,
            };
            let result = match client.answer(&req) {
                Ok(ans) => ans,
                Err(err) => {
                    CliView::render_llm_or_error(&err, *cmd_json || json);
                    return Err(AlreadyHandledError.into());
                }
            };
            CliView::render_answer(&result, *cmd_json || json)?;
            Ok(true)
        }

        Commands::List {
            category,
            media,
            status,
            as_of,
            date,
            limit,
            offset,
            page,
            save_to,
            path,
            json: cmd_json,
        } => {
            anyhow::ensure!(save_to.is_none() && !path, "Remote list does not support --save-to or --path. Use local mode to download attachments.");
            let paginated = client.list(
                *limit,
                *offset,
                *page,
                *category,
                Some(*status),
                media.clone(),
                *date,
                *as_of,
            )?;
            CliView::render_paginated_list(
                &paginated,
                Path::new(&client.base_url),
                *cmd_json || json,
            )?;
            Ok(true)
        }

        Commands::Inspect {
            terms,
            save_to,
            path,
            open,
            json: cmd_json,
        } => {
            anyhow::ensure!(save_to.is_none() && !path && !open, "Remote inspect does not support --save-to, --path, or --open. Use local mode to download attachments.");
            let as_json = *cmd_json || json;
            let mut inspected = Vec::new();
            for term in terms {
                if let Some(record) = client.get(term)? {
                    let links = client.links(&record.id)?;
                    if as_json {
                        inspected.push(serde_json::json!({"memory":record,"links":links}));
                    } else {
                        CliView::render_inspect(
                            &record,
                            &links,
                            Path::new(&client.base_url),
                            false,
                        )?;
                    }
                }
            }
            if as_json {
                let value = if terms.len() > 1 {
                    serde_json::json!(inspected)
                } else {
                    inspected
                        .into_iter()
                        .next()
                        .unwrap_or(serde_json::Value::Null)
                };
                println!("{}", serde_json::to_string_pretty(&value)?);
            }

            Ok(true)
        }

        Commands::Forget {
            id,
            all,
            hard,
            json: cmd_json,
        } => {
            let ok = client.forget(id.as_deref(), *all, *hard)?;
            CliView::render_forget(id.as_deref().unwrap_or("*"), ok, *hard, *cmd_json || json)?;
            Ok(true)
        }

        Commands::Clear {
            soft,
            json: cmd_json,
        } => {
            let removed = client.clear(*soft)?;
            CliView::render_clear(
                removed,
                !*soft,
                Path::new(&client.base_url),
                *cmd_json || json,
            )?;
            Ok(true)
        }

        Commands::Stats { json: cmd_json } => {
            let stats = client.stats()?;
            CliView::render_stats(&stats, Path::new(&client.base_url), *cmd_json || json)?;
            Ok(true)
        }

        Commands::Link {
            source,
            target,
            relation,
            weight,
            json: cmd_json,
        } => {
            let link = client.link(source, target, Some(relation), Some(*weight))?;
            CliView::render_link(
                &link.source_id,
                &link.target_id,
                &link.relation,
                link.weight,
                *cmd_json || json,
            )?;
            Ok(true)
        }

        Commands::Unlink {
            source,
            target,
            relation,
            json: cmd_json,
        } => {
            let unlinked = client.unlink(source, target, relation.as_deref())?;
            CliView::render_unlink(
                source,
                target,
                relation.as_deref(),
                unlinked,
                *cmd_json || json,
            )?;
            Ok(true)
        }

        Commands::Related {
            id,
            hops,
            json: cmd_json,
        } => {
            let results = client.related(id, *hops)?;
            CliView::render_related(id, &results, *cmd_json || json)?;
            Ok(true)
        }

        Commands::Graph {
            focus,
            hops,
            json: cmd_json,
        } => {
            let snapshot = client.graph(focus.as_deref(), Some(*hops))?;
            CliView::render_graph(&snapshot, *cmd_json || json)?;
            Ok(true)
        }

        Commands::Context { json: cmd_json } => {
            let stats_res = client.stats();
            let health_res = client.health();
            let is_online = health_res.is_ok();
            let info = serde_json::json!({
                "mode": "remote",
                "remote_url": client.base_url,
                "status": if is_online { "online" } else { "unreachable" },
                "api_key_configured": client.api_key.is_some(),
                "total_memories": stats_res.as_ref().map(|s| s.total_memories).unwrap_or(0),
                "active_memories": stats_res.as_ref().map(|s| s.active_memories).unwrap_or(0),
            });
            if *cmd_json || json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!();
                println!("  \x1b[1;38;2;220;38;38m❖\x1b[0m \x1b[1mLightMem Remote Context\x1b[0m");
                println!("  \x1b[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
                println!(
                    "  \x1b[38;2;113;113;122m├─\x1b[0m {:<20} \x1b[38;2;96;165;250m{}\x1b[0m",
                    "Remote Endpoint", client.base_url
                );
                println!(
                    "  \x1b[38;2;113;113;122m├─\x1b[0m {:<20} {}",
                    "Server Status",
                    if is_online {
                        "\x1b[1;32m● Online\x1b[0m"
                    } else {
                        "\x1b[1;31m○ Unreachable\x1b[0m"
                    }
                );
                println!(
                    "  \x1b[38;2;113;113;122m├─\x1b[0m {:<20} {}",
                    "Authentication",
                    if client.api_key.is_some() {
                        "\x1b[38;2;234;179;8mBearer Token (Configured)\x1b[0m"
                    } else {
                        "\x1b[38;2;148;163;184mNone\x1b[0m"
                    }
                );
                if let Ok(st) = stats_res {
                    println!(
                        "  \x1b[38;2;113;113;122m└─\x1b[0m {:<20} \x1b[1m{} active\x1b[0m ({} total)",
                        "Remote Vault",
                        st.active_memories,
                        st.total_memories
                    );
                } else {
                    println!("  \x1b[38;2;113;113;122m└─\x1b[0m {:<20} \x1b[38;2;239;68;68mCould not fetch remote stats\x1b[0m", "Remote Vault");
                }
                println!();
            }
            Ok(true)
        }

        Commands::Config { .. }
        | Commands::Setup { .. }
        | Commands::Docs { .. }
        | Commands::Bundle {
            action: BundleCommand::Info { .. },
            ..
        }
        | Commands::Serve { .. }
        | Commands::Version { .. }
        | Commands::Completions { .. }
        | Commands::Connect { mcp: false, .. }
        | Commands::Uninstall { .. }
        | Commands::History { .. }
        | Commands::Local { .. }
        | Commands::Remote { .. } => Ok(false),
        cmd => {
            let cmd_name = match cmd {
                Commands::Bundle {
                    action: BundleCommand::Export { .. },
                    ..
                } => "bundle export",
                Commands::Bundle {
                    action: BundleCommand::Import { .. },
                    ..
                } => "bundle import",
                Commands::Clear { .. } => "clear",
                Commands::Restore { .. } => "restore",
                Commands::Inspect { .. } => "inspect",
                Commands::Import { .. } => "import",
                Commands::Export { .. } => "export",
                Commands::Conflicts { .. } => "conflicts",
                Commands::Autolink { .. } => "autolink",
                Commands::Connect { mcp: true, .. } => "connect --mcp",
                _ => "this command",
            };
            let reason_msg = format!("This command ('{cmd_name}') is not supported in remote mode. No local vault was opened.");
            if json {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "error": "unsupported_remote_command",
                        "command": cmd_name,
                        "remote_url": client.base_url,
                        "message": reason_msg,
                        "hint": "Use --db PATH to explicitly select a local vault, or switch to local mode with 'lmem local'."
                    })
                );
            } else {
                CliView::render_error(
                    &CliErrorCard {
                        title: "Command Not Supported in Remote Mode",
                        reason: &reason_msg,
                        details: &[
                            ("Command", cmd_name),
                            ("Active Mode", &format!("Remote ({})", client.base_url)),
                        ],
                        suggestions: &[
                            ("Switch to local:", "lmem local"),
                            (
                                "Explicit local vault:",
                                &format!("lmem --db ~/.lightmem/memories.db {cmd_name}"),
                            ),
                        ],
                    },
                    false,
                );
            }
            std::process::exit(1);
        }
    }
}

fn open_controller(
    db_path: Option<&Path>,
    global: bool,
    connection: Option<&lightmem::ResolvedConnection>,
) -> Result<LightMem> {
    if let Some(connection) = connection {
        return LightMem::open_resolved(connection);
    }
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
    let att_dir = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("attachments");
    if att_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&att_dir);
    }
}

fn handle_remote_network_error(remote_url: &str, err: &anyhow::Error, json: bool) -> ! {
    let err_str = format!("{err:#}");
    let is_auth = err_str.contains("401")
        || err_str.contains("Unauthorized")
        || err_str.contains("Authentication failed");

    if json {
        let payload = if is_auth {
            serde_json::json!({
                "status": "error",
                "error": "unauthorized",
                "code": 401,
                "remote_url": remote_url,
                "message": "Invalid or missing API key for remote server",
                "hint": format!("Run 'lmem remote {remote_url} --api-key <KEY>' or 'lmem remote' to configure an API key.")
            })
        } else {
            serde_json::json!({
                "status": "error",
                "error": "connection_failed",
                "remote_url": remote_url,
                "message": err_str,
                "hint": "Run 'lmem local' to switch back to local vault."
            })
        };
        eprintln!("{payload}");
        std::process::exit(1);
    }

    if is_auth {
        eprintln!(
            "\n{} {}",
            "⚠️".yellow(),
            format!("Authentication failed (401 Unauthorized) at {remote_url}")
                .red()
                .bold()
        );
        eprintln!(
            "  {} {:<12} The remote server requires a valid API key.",
            "◈".dimmed(),
            "Reason:".white().bold()
        );
        eprintln!();
        eprintln!("  {} {}", "💡".dimmed(), "How to fix:".white().bold());
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Set API key:",
            format!("lmem remote {remote_url} --api-key \"<YOUR_API_KEY>\"").cyan()
        );
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Interactive:",
            "lmem remote".cyan()
        );
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Disconnect:",
            "lmem local  (switch back to local vault)".yellow()
        );
        eprintln!();
    } else {
        eprintln!(
            "\n{} {}",
            "⚠️".yellow(),
            format!("Cannot connect to remote server at {remote_url}")
                .yellow()
                .bold()
        );
        eprintln!(
            "  {} {:<12} Remote server is offline or unreachable.",
            "◈".dimmed(),
            "Reason:".white().bold()
        );
        eprintln!();
        eprintln!("  {} {}", "💡".dimmed(), "How to fix:".white().bold());
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Disconnect:",
            "lmem local  (switch back to local vault)".yellow()
        );
        eprintln!(
            "    {} {:<14} {}",
            "•".dimmed(),
            "Start daemon:",
            "lmem serve -d --port <PORT>".cyan()
        );
        eprintln!();
    }
    std::process::exit(1);
}

fn parse_reranker_setting(raw: &str) -> Result<String> {
    let lower = raw.trim().to_lowercase();
    if let Some(model) = lower.strip_prefix("onnx:") {
        anyhow::ensure!(
            matches!(
                model,
                "minilm-squad2" | "tinyroberta-squad2" | "minilm" | "tinyroberta"
            ),
            "Unknown ONNX answer model: {}",
            model
        );
    }
    anyhow::ensure!(
        matches!(
            lower.as_str(),
            "onnx"
                | "qa"
                | "precision"
                | "ollama"
                | "openai"
                | "openrouter"
                | "gemini"
                | "groq"
                | "deepseek"
                | "mistral"
                | "llm"
                | "top1"
                | "minilm-squad2"
                | "tinyroberta-squad2"
        ) || lower.starts_with("onnx:")
            || lower.contains(':'),
        "Invalid answer engine '{}'. Use minilm-squad2, tinyroberta-squad2, top1, or a model spec like ollama:<model>, openai:<model>, openrouter:<model>, gemini:<model>, groq:<model>.",
        raw
    );
    let normalized = LightMemConfig::normalize_reranker(raw, None);
    anyhow::ensure!(
        !normalized.ends_with(':'),
        "Answer engine needs a model after ':'"
    );
    Ok(normalized)
}

/// Read the selected vault without creating it or applying schema migrations.
fn configuration_plan(
    previous: &LightMemConfig,
    proposed: &LightMemConfig,
    db_path: &Path,
) -> Result<serde_json::Value> {
    use rusqlite::OptionalExtension;
    let identity = proposed.embedding_identity()?;
    let mut count = 0_i64;
    let mut stored: Option<String> = None;
    if db_path.exists() {
        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .with_context(|| format!("Cannot inspect selected vault {}", db_path.display()))?;
        count = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
        let has_state: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='embedding_state')",
            [], |row| row.get(0),
        )?;
        if has_state {
            stored = conn
                .query_row(
                    "SELECT identity FROM embedding_state WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
        }
    }
    let before = serde_json::to_value(previous)?;
    let after = serde_json::to_value(proposed)?;
    let keys: std::collections::BTreeSet<_> = before
        .as_object()
        .context("Invalid configuration")?
        .keys()
        .chain(after.as_object().context("Invalid configuration")?.keys())
        .collect();
    let mut changes: Vec<_> = keys
        .into_iter()
        .filter(|key| before.get(*key) != after.get(*key))
        .cloned()
        .collect();
    if changes.iter().any(|k| k == "openai_api_key")
        && !changes.iter().any(|k| k == "ollama_api_key")
    {
        changes.push("ollama_api_key".to_string());
    }
    if changes.iter().any(|k| k == "openai_endpoint") && !changes.iter().any(|k| k == "ollama_url")
    {
        changes.push("ollama_url".to_string());
    }
    if changes.iter().any(|k| k == "openai_model") && !changes.iter().any(|k| k == "reranker") {
        changes.push("reranker".to_string());
    }
    if changes.iter().any(|k| k == "reranker") && !changes.iter().any(|k| k == "openai_model") {
        changes.push("openai_model".to_string());
    }
    changes.sort();
    Ok(serde_json::json!({
        "dry_run": true,
        "config_scope": "shared across vaults in this configuration root",
        "config_file": LightMemConfig::config_file(),
        "selected_db": db_path,
        "vault_exists": db_path.exists(),
        "total_memories": count,
        "changed_fields": changes,
        "stored_embedding_identity": stored,
        "requested_embedding_identity": identity,
        "migration_required": count > 0 && stored.as_deref() != Some(identity.as_str()),
        "embedding_change": previous.embedding_identity()? != identity,
        "other_vaults": "Not reindexed by this command; check each before using changed embeddings",
        "configuration": proposed.redacted_json()?,
    }))
}

fn apply_configuration(
    previous: &LightMemConfig,
    proposed: &LightMemConfig,
    db_path: &Path,
    yes: bool,
    json: bool,
) -> Result<bool> {
    let identity = proposed.embedding_identity()?;
    if previous.embedding_identity()? != identity && db_path.exists() {
        let lm = LightMem::open_at(db_path, proposed.clone())?;
        if !migrate_embeddings(&lm, yes, json, false)? {
            return Ok(false);
        }
    }
    proposed.save()?;
    Ok(true)
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

fn open_file_in_default_app(path: &Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .with_context(|| format!("Failed to open {} with 'open'", path.display()))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .with_context(|| format!("Failed to open {} with 'start'", path.display()))?;
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .with_context(|| format!("Failed to open {} with 'xdg-open'", path.display()))?;
    }
    Ok(())
}

fn open_files_in_default_app(paths: &[PathBuf]) {
    for p in paths {
        if let Err(err) = open_file_in_default_app(p) {
            eprintln!("Warning: {}", err);
        }
    }
}

#[derive(Debug)]
struct AlreadyHandledError;

impl std::fmt::Display for AlreadyHandledError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "")
    }
}

impl std::error::Error for AlreadyHandledError {}

fn should_record_history(cli: &Cli) -> bool {
    if cli.version {
        return false;
    }
    !matches!(
        &cli.command,
        None | Some(Commands::Completions { .. })
            | Some(Commands::CompletionConnections)
            | Some(Commands::Doctor { .. })
            | Some(Commands::History { .. })
            | Some(Commands::Config { .. })
            | Some(Commands::Setup { .. })
    )
}

fn extract_subcommand_name(cli: &Cli) -> String {
    match &cli.command {
        Some(Commands::Status { .. }) => "status",
        Some(Commands::Identity { .. }) => "identity",
        Some(Commands::Context { .. }) => "context",
        Some(Commands::Remember { .. }) => "remember",
        Some(Commands::Recall { .. }) => "recall",
        Some(Commands::Answer { .. }) => "answer",
        Some(Commands::Forget { .. }) => "forget",
        Some(Commands::Restore { .. }) => "restore",
        Some(Commands::Clear { .. }) => "clear",
        Some(Commands::Deduplicate { .. }) => "dedup",
        Some(Commands::Inspect { .. }) => "inspect",
        Some(Commands::List { .. }) => "list",
        Some(Commands::Config { .. }) => "config",
        Some(Commands::Setup { .. }) => "setup",
        Some(Commands::Reindex { .. }) => "reindex",
        Some(Commands::Conflicts { .. }) => "conflicts",
        Some(Commands::Stats { .. }) => "stats",
        Some(Commands::Export { .. }) => "export",
        Some(Commands::Import { .. }) => "import",
        Some(Commands::Bundle { .. }) => "bundle",
        Some(Commands::Link { .. }) => "link",
        Some(Commands::Unlink { .. }) => "unlink",
        Some(Commands::Related { .. }) => "related",
        Some(Commands::Graph { .. }) => "graph",
        Some(Commands::Autolink { .. }) => "autolink",
        Some(Commands::Observe { .. }) => "observe",
        Some(Commands::Connect { .. }) => "connect",
        Some(Commands::Mcp) => "mcp",
        Some(Commands::Doctor { .. }) => "doctor",
        Some(Commands::Serve { .. }) => "serve",
        Some(Commands::Local { .. }) => "local",
        Some(Commands::Remote { .. }) => "remote",
        Some(Commands::Docs { .. }) => "docs",
        Some(Commands::Uninstall { .. }) => "uninstall",
        Some(Commands::History { .. }) => "history",
        Some(Commands::Completions { .. }) => "completions",
        Some(Commands::CompletionConnections) => "__connections",
        Some(Commands::Version { .. }) => "version",
        None => "stats",
    }
    .to_string()
}

fn main() {
    #[cfg(windows)]
    {
        let _ = colored::control::set_virtual_terminal(true);
    }
    let cli = Cli::parse();
    let json = cli.json || std::env::args().any(|a| a == "--json");
    let should_record = should_record_history(&cli);
    let subcommand = extract_subcommand_name(&cli);
    let args: Vec<String> = std::env::args().collect();
    let sanitized_cmd = HistoryService::sanitize_args(&args);

    let start_time = std::time::Instant::now();
    let run_res = run(cli);
    let duration = start_time.elapsed();

    if should_record {
        let status = if run_res.is_ok() { "success" } else { "error" };
        let err_msg = run_res.as_ref().err().map(|e| e.to_string());
        HistoryService::record(
            &sanitized_cmd,
            &subcommand,
            status,
            duration.as_millis() as u64,
            err_msg.as_deref(),
        );
    }

    if let Err(err) = run_res {
        if !err.is::<AlreadyHandledError>() {
            if CliView::try_render_known_error(&err, json) {
                std::process::exit(1);
            }
            eprintln!("Error: {err}");
        }
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    if cli.version {
        return CliView::render_version(cli.json);
    }
    if let Some(Commands::Completions { shell }) = &cli.command {
        return print_completions(*shell);
    }
    if matches!(cli.command, Some(Commands::CompletionConnections)) {
        for id in terminal::connection_ids()? {
            println!("{id}");
        }
        return Ok(());
    }
    if let Some(Commands::Doctor { json }) = &cli.command {
        return terminal::doctor(
            &cli.db,
            cli.global,
            cli.connection.as_deref(),
            cli.scope.clone(),
            cli.session.as_deref(),
            cli.remote.as_deref(),
            *json || cli.json,
        );
    }
    if let Some(Commands::Local { json }) = &cli.command {
        let mut cfg = LightMemConfig::load();
        let was_remote = cfg.remote_url.is_some() || cfg.api_key.is_some();
        cfg.remote_url = None;
        cfg.api_key = None;
        cfg.save()?;

        let default_vault = LightMemConfig::resolve_db_path(cli.global);
        if *json || cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "ok",
                    "mode": "local",
                    "vault": default_vault.display().to_string(),
                    "previous_remote_cleared": was_remote,
                })
            );
        } else {
            println!(
                "{} {}",
                "✓".green().bold(),
                "Switched to local mode.".white().bold()
            );
            println!(
                "  {} {:<10} {}",
                "◫".dimmed(),
                "Vault".dimmed(),
                CliView::format_path(&default_vault).bold()
            );
            if std::env::var("LIGHTMEM_REMOTE_URL").is_ok() || std::env::var("LIGHTMEM_URL").is_ok()
            {
                println!(
                    "  {} Note: Environment variable LIGHTMEM_REMOTE_URL/LIGHTMEM_URL is active.",
                    "⚠️".yellow()
                );
                println!("    Unset the environment variable or use --db to target local vaults explicitly.");
            }
        }
        return Ok(());
    }
    if let Some(Commands::Remote { url, api_key, json }) = &cli.command {
        let mut cfg = LightMemConfig::load();
        let (target_url, new_api_key) = match url {
            Some(u) => {
                let clean = terminal::normalize_http_url(u)?;
                (clean, api_key.clone().or_else(|| cfg.api_key.clone()))
            }
            None => {
                if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
                    let (u, k) =
                        terminal::prompt_remote(cfg.remote_url.as_deref(), cfg.api_key.as_deref())?;
                    (u, api_key.clone().or(k))
                } else {
                    let default_u = cfg
                        .remote_url
                        .clone()
                        .unwrap_or_else(|| "http://localhost:8080".to_string());
                    let clean = terminal::normalize_http_url(&default_u)?;
                    (clean, api_key.clone().or_else(|| cfg.api_key.clone()))
                }
            }
        };

        cfg.remote_url = Some(target_url.clone());
        if let Some(ref k) = new_api_key {
            if k.eq_ignore_ascii_case("none") || k.trim().is_empty() {
                cfg.api_key = None;
            } else {
                cfg.api_key = Some(k.clone());
            }
        }
        cfg.save()?;

        let client = lightmem::RemoteClient::new(&target_url, cfg.effective_api_key())
            .with_timeout(std::time::Duration::from_millis(1200));
        let probe_res = client.stats();
        let is_connected = probe_res.is_ok();

        if *json || cli.json {
            println!(
                "{}",
                serde_json::json!({
                    "status": "ok",
                    "mode": "remote",
                    "remote_url": target_url,
                    "has_api_key": cfg.api_key.is_some(),
                    "connected": is_connected,
                })
            );
        } else {
            println!(
                "{} {}",
                "✓".green().bold(),
                "Switched to remote mode.".white().bold()
            );
            println!(
                "  {} {:<10} {}",
                "◫".dimmed(),
                "Remote".dimmed(),
                target_url.bold()
            );
            if let Ok(stats) = probe_res {
                println!(
                    "  {} {:<10} {}",
                    "◈".dimmed(),
                    "Status".dimmed(),
                    format!("connected ({} active memories)", stats.active_memories).green()
                );
            } else if let Err(ref e) = probe_res {
                let err_str = e.to_string();
                if err_str.contains("401") || err_str.contains("Unauthorized") {
                    println!(
                        "  {} {:<10} {}",
                        "◈".dimmed(),
                        "Status".dimmed(),
                        "authentication failed (401 Unauthorized)".red().bold()
                    );
                    println!(
                        "  {} Warning: The remote server requires a valid API key.",
                        "⚠️".yellow()
                    );
                    println!(
                        "    Update it with: lmem remote {} --api-key \"<YOUR_KEY>\"",
                        target_url
                    );
                } else {
                    println!(
                        "  {} {:<10} {}",
                        "◈".dimmed(),
                        "Status".dimmed(),
                        "configured (server not responding yet)".yellow()
                    );
                    println!(
                        "  {} Note: Start server with 'lmem serve --daemon', or switch back with 'lmem local'.",
                        "ℹ".dimmed()
                    );
                }
            }
            if cfg.api_key.is_some() {
                println!(
                    "  {} {:<10} API key configured",
                    "✦".dimmed(),
                    "Auth".dimmed()
                );
            }
        }
        return Ok(());
    }
    let env_db = std::env::var("LIGHTMEM_DB")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from);
    let default_db = cli
        .db
        .as_deref()
        .map(Path::to_path_buf)
        .or_else(|| env_db.clone())
        .unwrap_or_else(|| LightMemConfig::resolve_db_path(cli.global));

    if let Some(Commands::Identity { action, json }) = &cli.command {
        return run_identity(action, *json || cli.json, default_db);
    }
    if let Some(Commands::Status { json }) = &cli.command {
        let registry = lightmem::IdentityRegistry::open_default()?;
        let overview = registry.status_overview(default_db)?;
        return CliView::render_vault_status_overview(&overview, *json || cli.json);
    }
    let connection_id = cli.connection.clone().or_else(|| {
        std::env::var("LIGHTMEM_CONNECTION")
            .ok()
            .filter(|v| !v.trim().is_empty())
    });
    anyhow::ensure!(
        connection_id.is_some() || (cli.scope.is_none() && cli.session.is_none()),
        "--scope and --session require --connection"
    );
    anyhow::ensure!(
        connection_id.is_none() || (cli.db.is_none() && env_db.is_none() && !cli.global),
        "A connection cannot be combined with --db, LIGHTMEM_DB, or --global"
    );
    let resolved_connection = connection_id
        .as_deref()
        .map(|id| {
            lightmem::IdentityRegistry::open_default()?.resolve(
                id,
                cli.scope.clone(),
                cli.session.clone(),
            )
        })
        .transpose()?;
    let connection = resolved_connection.as_ref();
    let effective_db = cli
        .db
        .as_deref()
        .or(env_db.as_deref())
        .or_else(|| connection.map(|c| c.db_path.as_path()));
    let global = cli.global;

    // Version and help remain usable if configuration is malformed; other commands report it.
    let cfg = if matches!(cli.command, Some(Commands::Version { .. })) {
        LightMemConfig::default()
    } else {
        LightMemConfig::try_load()?
    };
    anyhow::ensure!(
        cli.remote.is_none() || (cli.db.is_none() && env_db.is_none() && !cli.global),
        "--remote cannot be combined with --db, LIGHTMEM_DB, or --global"
    );
    let effective_remote_url = if cli.remote.is_some() {
        cli.remote
    } else if cli.db.is_some() || env_db.is_some() || cli.global {
        None
    } else {
        cfg.effective_remote_url()
    };
    let effective_api_key = cli.api_key.or_else(|| cfg.effective_api_key());

    if let Some(ref remote_url) = effective_remote_url {
        let local_admin = matches!(
            cli.command,
            Some(
                Commands::Config { .. }
                    | Commands::Setup { .. }
                    | Commands::Docs { .. }
                    | Commands::Serve { .. }
                    | Commands::Version { .. }
                    | Commands::Completions { .. }
                    | Commands::Connect { mcp: false, .. }
                    | Commands::Uninstall { .. }
                    | Commands::History { .. }
                    | Commands::Local { .. }
                    | Commands::Remote { .. }
                    | Commands::Bundle {
                        action: BundleCommand::Info { .. },
                        ..
                    }
            )
        );
        anyhow::ensure!(!LightMemConfig::offline_requested() || local_admin, "Offline mode blocks remote vault operations. Select a local database or clear remote routing with 'lmem local' (or 'lmem config --clear-remote').");
        anyhow::ensure!(connection.is_none() || local_admin, "Remote mode does not support local connections or scopes. Clear remote mode with 'lmem local' (or 'lmem config --clear-remote') before selecting a connection.");
        let client = lightmem::RemoteClient::new(remote_url, effective_api_key.clone());
        if cli.command.is_none() {
            let stats = match client.stats() {
                Ok(s) => s,
                Err(ref e) => handle_remote_network_error(remote_url, e, cli.json),
            };
            if cli.json {
                return CliView::render_stats(&stats, Path::new(remote_url), true);
            }
            CliView::render_intro(Path::new(remote_url), &stats);
            return Ok(());
        }
        if let Some(ref cmd) = cli.command {
            match handle_remote_command(&client, cmd, cli.json) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(e) => {
                    let err_str = format!("{e:#}");
                    let is_network_or_auth = err_str.contains("401")
                        || err_str.contains("Unauthorized")
                        || err_str.contains("Authentication failed")
                        || err_str.contains("Failed to reach remote server");
                    if is_network_or_auth && !err_str.contains("not supported in remote mode") {
                        handle_remote_network_error(remote_url, &e, cli.json);
                    }
                    return Err(e);
                }
            }
        }
    }

    let Some(command) = cli.command else {
        if cli.json {
            let lm = open_controller(effective_db, global, connection)?;
            let stats = lm.stats()?;
            return CliView::render_stats(&stats, lm.db_path(), true);
        }
        let lm = open_controller(effective_db, global, connection)?;
        let stats = lm.stats()?;
        CliView::render_intro(lm.db_path(), &stats);
        return Ok(());
    };

    match command {
        Commands::Identity { .. }
        | Commands::Status { .. }
        | Commands::Doctor { .. }
        | Commands::CompletionConnections
        | Commands::Local { .. }
        | Commands::Remote { .. } => unreachable!(),
        Commands::Context { json } => {
            let value = match connection {
                Some(connection) => serde_json::to_value(connection)?,
                None => {
                    serde_json::json!({"connection_id": null, "db_path": effective_db.map(Path::to_path_buf).unwrap_or_else(|| LightMemConfig::resolve_db_path(global))})
                }
            };
            CliView::render_context(&value, json || cli.json)?;
        }
        Commands::Remember {
            content,
            files,
            extract,
            reranker,
            category,
            title,
            tags,
            confidence,
            ttl,
            supersede,
            provenance,
            no_entities,
            json,
        } => {
            let has_text = content
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .is_some();
            anyhow::ensure!(
                has_text || !files.is_empty(),
                "Provide memory content or attach at least one file via --file / -f"
            );
            let lm = open_controller(effective_db, global, connection)?;
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
            let (memory, conflicts, entity_warning) = lm.remember_with_attachments_and_entities(
                content.as_deref(),
                &files,
                extract,
                !no_entities,
                reranker.as_deref(),
                cat,
                title,
                tag_vec,
                Some(confidence),
                ttl,
                supersede,
                provenance,
            )?;
            CliView::render_remembered_full(
                &memory,
                &conflicts,
                supersede,
                lm.db_path(),
                json,
                entity_warning.as_deref(),
            )?;
        }

        Commands::Recall {
            query,
            image,
            category,
            media,
            as_of,
            date,
            limit,
            min_similarity,
            precision,
            multi_hop,
            save_to,
            path,
            open,
            json,
        } => {
            if precision {
                anyhow::ensure!(image.is_none() && save_to.is_none() && !path && !open && media.is_none() && !multi_hop && min_similarity.is_none(), "Precision recall returns an answer; image, media, attachment, multi-hop and similarity options are not supported together. Use recall without --precision for filtered results.");
            }
            let lm = open_controller(effective_db, global, connection)?;
            let cat = category;
            let as_of_dt = as_of;

            if !migrate_embeddings(&lm, false, json || path, false)? {
                return Ok(());
            }
            if precision && !path {
                anyhow::ensure!(
                    !query.trim().is_empty(),
                    "Search query cannot be blank with --precision"
                );
                let result = match lm.answer_with_options(&query, cat, as_of_dt, date, limit, None)
                {
                    Ok(ans) => ans,
                    Err(err) => {
                        CliView::render_llm_or_error(&err, json);
                        return Err(AlreadyHandledError.into());
                    }
                };
                CliView::render_answer(&result, json)?;
            } else {
                let filter_attachments = media.is_some() || path || save_to.is_some() || open;
                let fetch_limit = if filter_attachments {
                    limit.max(100)
                } else {
                    limit
                };
                let img_b64 = if let Some(ref img_path) = image {
                    let data = std::fs::read(img_path).with_context(|| {
                        format!("Failed to read query image file: {}", img_path.display())
                    })?;
                    anyhow::ensure!(
                        !data.is_empty(),
                        "Query image file is empty: {}",
                        img_path.display()
                    );
                    Some(encode_base64(&data))
                } else {
                    None
                };
                let mm_input = if let Some(ref b64) = img_b64 {
                    MultimodalInput::with_image(&query, b64)
                } else {
                    MultimodalInput::text_only(&query)
                };
                let mut results = lm.recall_multimodal_expanded(
                    &mm_input,
                    cat,
                    as_of_dt,
                    date,
                    fetch_limit,
                    min_similarity,
                    multi_hop,
                )?;
                if filter_attachments {
                    let k = media.as_deref().unwrap_or("any").trim().to_lowercase();
                    results.retain(|r| {
                        !r.memory.attachments.is_empty()
                            && (k == "any"
                                || k == "all"
                                || r.memory.attachments.iter().any(|a| {
                                    a.media_kind.eq_ignore_ascii_case(&k)
                                        || a.mime_type.to_lowercase().contains(&k)
                                }))
                    });
                    results.truncate(limit);
                }
                let ids: Vec<String> = results.iter().map(|r| r.memory.id.clone()).collect();
                let display_query = if query.trim().is_empty() {
                    image
                        .as_ref()
                        .map(|p| format!("[image: {}]", p.display()))
                        .unwrap_or_default()
                } else {
                    query.clone()
                };
                if path {
                    let all_paths =
                        lm.materialize_memories_attachments(&ids, save_to.as_deref())?;
                    if open {
                        open_files_in_default_app(&all_paths);
                    }
                    if json {
                        println!("{}", serde_json::to_string_pretty(&all_paths)?);
                    } else {
                        for p in all_paths {
                            println!("{}", p.display());
                        }
                    }
                } else if let Some(ref target) = save_to {
                    let all_saved = lm.save_memories_attachments_to(&ids, target)?;
                    if open {
                        open_files_in_default_app(&all_saved);
                    }
                    if json {
                        let mut offset = 0usize;
                        let json_items: Vec<serde_json::Value> = results
                            .iter()
                            .map(|r| {
                                let count = r.memory.attachments.len();
                                let end = (offset + count).min(all_saved.len());
                                let saved_slice = &all_saved[offset..end];
                                offset = end;
                                let mut val = serde_json::to_value(r).unwrap_or_default();
                                if let Some(obj) = val.as_object_mut() {
                                    obj.insert(
                                        "saved_attachments".to_string(),
                                        serde_json::json!(saved_slice),
                                    );
                                }
                                val
                            })
                            .collect();
                        println!("{}", serde_json::to_string_pretty(&json_items)?);
                    } else {
                        CliView::render_recall(&display_query, &results, lm.db_path(), false)?;
                        if !all_saved.is_empty() {
                            println!();
                            for p in &all_saved {
                                println!(
                                    "  {} {} {}",
                                    CliView::emerald_bold("✓ Saved verified attachment to"),
                                    CliView::white_bold(&p.display().to_string()),
                                    CliView::slate("(SHA-256 verified)")
                                );
                            }
                        }
                    }
                } else {
                    if open {
                        let all_paths = lm.materialize_memories_attachments(&ids, None)?;
                        open_files_in_default_app(&all_paths);
                    }
                    CliView::render_recall(&display_query, &results, lm.db_path(), json)?;
                }
            }
        }

        Commands::List {
            category,
            media,
            status,
            as_of,
            date,
            limit,
            offset,
            page,
            save_to,
            path,
            json,
        } => {
            let lm = open_controller(effective_db, global, connection)?;
            let cat = category;
            let st = Some(status);
            let as_of_dt = as_of;
            let filter_attachments = media.is_some() || path || save_to.is_some();

            let effective_offset = if let Some(p) = page {
                anyhow::ensure!(p > 0 && limit > 0, "Page and limit must be positive");
                (p - 1).checked_mul(limit).context("Pagination overflow")?
            } else {
                offset
            };
            let paginated = if filter_attachments {
                lm.list_filtered(
                    cat,
                    st,
                    as_of_dt,
                    date,
                    Some(media.as_deref().unwrap_or("any")),
                    limit,
                    effective_offset,
                )?
            } else if let Some(p) = page {
                lm.list_page_with_date(cat, st, as_of_dt, date, p, limit)?
            } else {
                lm.list_paginated_with_date(cat, st, as_of_dt, date, limit, offset)?
            };

            let ids: Vec<String> = paginated.items.iter().map(|m| m.id.clone()).collect();
            if path {
                let all_paths = lm.materialize_memories_attachments(&ids, save_to.as_deref())?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&all_paths)?);
                } else {
                    for p in all_paths {
                        println!("{}", p.display());
                    }
                }
            } else if let Some(ref target) = save_to {
                let all_saved = lm.save_memories_attachments_to(&ids, target)?;
                if json {
                    let mut val = serde_json::to_value(&paginated)?;
                    if let Some(obj) = val.as_object_mut() {
                        obj.insert(
                            "saved_attachments".to_string(),
                            serde_json::json!(&all_saved),
                        );
                        if let Some(items_arr) = obj.get_mut("items").and_then(|v| v.as_array_mut())
                        {
                            let mut offset_idx = 0usize;
                            for (item_val, m) in items_arr.iter_mut().zip(paginated.items.iter()) {
                                let count = m.attachments.len();
                                let end = (offset_idx + count).min(all_saved.len());
                                let saved_slice = &all_saved[offset_idx..end];
                                offset_idx = end;
                                if let Some(item_obj) = item_val.as_object_mut() {
                                    item_obj.insert(
                                        "saved_attachments".to_string(),
                                        serde_json::json!(saved_slice),
                                    );
                                }
                            }
                        }
                    }
                    println!("{}", serde_json::to_string_pretty(&val)?);
                } else {
                    CliView::render_paginated_list(&paginated, lm.db_path(), false)?;
                    if !all_saved.is_empty() {
                        println!();
                        for p in &all_saved {
                            println!(
                                "  {} {} {}",
                                CliView::emerald_bold("✓ Saved verified attachment to"),
                                CliView::white_bold(&p.display().to_string()),
                                CliView::slate("(SHA-256 verified)")
                            );
                        }
                    }
                }
            } else {
                CliView::render_paginated_list(&paginated, lm.db_path(), json)?;
            }
        }

        Commands::Inspect {
            terms,
            save_to,
            path,
            open,
            json,
        } => {
            let lm = open_controller(effective_db, global, connection)?;
            if path {
                let mut ids = Vec::new();
                for term in &terms {
                    let Some(record) = lm.get(term)? else {
                        continue; // Skip missing memories in path mode.
                    };
                    ids.push(record.id);
                }
                if ids.is_empty() {
                    if json {
                        println!("[]");
                    } else {
                        eprintln!("Memory not found: {}", terms.join(", "));
                    }
                    std::process::exit(1);
                }
                let all_paths = lm.materialize_memories_attachments(&ids, save_to.as_deref())?;
                if open {
                    open_files_in_default_app(&all_paths);
                }
                if all_paths.is_empty() {
                    if json {
                        println!("[]");
                    } else {
                        eprintln!("No attachments found for: {}", terms.join(", "));
                    }
                    std::process::exit(1);
                }
                if json {
                    println!("{}", serde_json::to_string_pretty(&all_paths)?);
                } else {
                    for p in all_paths {
                        println!("{}", p.display());
                    }
                }
            } else if terms.len() == 1 {
                let term = &terms[0];
                if let Some(record) = lm.get(term)? {
                    let links = lm.get_links(&record.id).unwrap_or_default();
                    let saved_paths = if let Some(ref target) = save_to {
                        lm.save_attachments_to(&record.id, target)?
                    } else {
                        Vec::new()
                    };
                    if open {
                        if save_to.is_some() {
                            open_files_in_default_app(&saved_paths);
                        } else {
                            let all_paths = lm.materialize_memories_attachments(
                                std::slice::from_ref(&record.id),
                                None,
                            )?;
                            open_files_in_default_app(&all_paths);
                        }
                    }
                    if json && save_to.is_some() {
                        let obj = serde_json::json!({
                            "memory": record,
                            "links": links,
                            "saved_attachments": saved_paths,
                        });
                        println!("{}", serde_json::to_string_pretty(&obj)?);
                    } else {
                        CliView::render_inspect(&record, &links, lm.db_path(), json)?;
                        if !saved_paths.is_empty() && !json {
                            for p in &saved_paths {
                                println!(
                                    "  {} {} {}",
                                    CliView::emerald_bold("✓ Saved verified attachment to"),
                                    CliView::white_bold(&p.display().to_string()),
                                    CliView::slate("(SHA-256 verified)")
                                );
                            }
                            println!();
                        }
                    }
                } else {
                    if json {
                        println!("null");
                    } else {
                        eprintln!("Memory not found: {}", term);
                    }
                    std::process::exit(1);
                }
            } else {
                let mut records = Vec::new();
                for term in &terms {
                    if let Some(record) = lm.get(term)? {
                        records.push(record);
                    }
                }
                if records.is_empty() {
                    if json {
                        println!("[]");
                    } else {
                        eprintln!("Memory not found: {}", terms.join(", "));
                    }
                    std::process::exit(1);
                }
                let ids: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
                let all_saved = if let Some(ref target) = save_to {
                    lm.save_memories_attachments_to(&ids, target)?
                } else {
                    Vec::new()
                };
                if open {
                    if save_to.is_some() {
                        open_files_in_default_app(&all_saved);
                    } else {
                        let all_paths = lm.materialize_memories_attachments(&ids, None)?;
                        open_files_in_default_app(&all_paths);
                    }
                }
                let mut offset_idx = 0usize;
                let mut json_items = Vec::new();
                for record in records {
                    let links = lm.get_links(&record.id).unwrap_or_default();
                    let saved_paths = if save_to.is_some() {
                        let count = record.attachments.len();
                        let end = (offset_idx + count).min(all_saved.len());
                        let slice = all_saved[offset_idx..end].to_vec();
                        offset_idx = end;
                        slice
                    } else {
                        Vec::new()
                    };
                    if json {
                        if save_to.is_some() {
                            json_items.push(serde_json::json!({
                                "memory": record,
                                "links": links,
                                "saved_attachments": saved_paths,
                            }));
                        } else {
                            json_items.push(serde_json::json!({
                                "memory": record,
                                "links": links,
                            }));
                        }
                    } else {
                        CliView::render_inspect(&record, &links, lm.db_path(), false)?;
                        if !saved_paths.is_empty() {
                            for p in &saved_paths {
                                println!(
                                    "  {} {} {}",
                                    CliView::emerald_bold("✓ Saved verified attachment to"),
                                    CliView::white_bold(&p.display().to_string()),
                                    CliView::slate("(SHA-256 verified)")
                                );
                            }
                            println!();
                        }
                    }
                }
                if json {
                    println!("{}", serde_json::to_string_pretty(&json_items)?);
                }
            }
        }

        Commands::Deduplicate { json } => {
            let lm = open_controller(effective_db, global, connection)?;
            let merged = lm.deduplicate()?;
            CliView::render_dedup(merged, json)?;
        }

        Commands::Forget {
            id,
            all,
            hard,
            json,
        } => {
            let lm = open_controller(effective_db, global, connection)?;
            if all {
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

        Commands::Restore { term, json } => {
            let lm = open_controller(effective_db, global, connection)?;
            let memory = lm.restore_memory(&term)?;
            if !json {
                println!("Restored memory in {}", lm.db_path().display());
            }
            let links = lm.get_links(&memory.id)?;
            CliView::render_inspect(&memory, &links, lm.db_path(), json)?;
        }

        Commands::Clear { soft, json } => {
            let hard = !soft;
            let lm = open_controller(effective_db, global, connection)?;
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
            bundle,
            output,
            output_json,
        } => {
            let lm = open_controller(effective_db, global, connection)?;
            let is_bundle = bundle
                || output
                    .as_ref()
                    .and_then(|p| p.extension())
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("lmbundle"))
                    .unwrap_or(false);

            if is_bundle {
                let exported_path = lm.export_bundle(output.as_deref())?;
                let manifest = lm.inspect_bundle(&exported_path)?;
                CliView::render_bundle_export(&exported_path, &manifest, output_json || json)?;
            } else if json {
                let target = output.unwrap_or_else(|| PathBuf::from("lightmem-backup.json"));
                let exported_path = lm.export_json(&target)?;
                CliView::render_export(&exported_path, true, output_json)?;
            } else {
                let exported_path = lm.export_okf(output.as_deref())?;
                CliView::render_export(&exported_path, false, output_json)?;
            }
        }

        Commands::Import {
            file,
            enrich,
            replace,
            merge,
            json,
        } => {
            anyhow::ensure!(!enrich, "Import enrichment is not supported yet. Remove --enrich; use observe --dry-run to review extracted candidates.");
            let lm = open_controller(effective_db, global, connection)?;
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("lmbundle") {
                let res = lm.import_bundle(&file, replace, merge)?;
                CliView::render_bundle_import(&res, json)?;
            } else {
                if !migrate_embeddings(&lm, false, json, false)? {
                    return Ok(());
                }
                let count = lm.import_file_with_options(&file, enrich, replace)?;
                CliView::render_import(count, &file, json)?;
            }
        }

        Commands::Bundle { action, json } => match action {
            BundleCommand::Export {
                output,
                json: cmd_json,
            } => {
                let is_json = json || cmd_json;
                let lm = open_controller(effective_db, global, connection)?;
                let mut progress_bar = terminal::TerminalProgress::new("Packaging", 0);
                if !is_json {
                    eprintln!();
                    eprintln!(
                        "  {} {}",
                        CliView::crimson_bold("❖"),
                        CliView::white_bold("L I G H T M E M  Bundle Export")
                    );
                    eprintln!(
                        "  {}",
                        CliView::crimson(
                            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                        )
                    );
                    if let Some(out_p) = output.as_deref() {
                        eprintln!(
                            "  {} {:<16} {}",
                            CliView::slate("◫"),
                            CliView::white_bold("Destination:"),
                            out_p.display()
                        );
                    }
                    eprintln!(
                        "  {} {:<16} {}",
                        CliView::slate("◈"),
                        CliView::white_bold("Vault:"),
                        lm.db_path().display()
                    );
                    eprintln!();
                }

                let exported_path = lm.export_bundle_with_progress(output.as_deref(), |prog| {
                    if is_json {
                        return;
                    }
                    match prog {
                        lightmem::services::BundleProgress::StageStart {
                            stage,
                            total_stages,
                            title,
                        } => {
                            if stage == 1 || stage == 2 {
                                eprint!(
                                    "  \x1b[38;2;113;113;122m▸\x1b[0m [{}/{}] {:<38}",
                                    stage, total_stages, title
                                );
                                let _ = std::io::stderr().flush();
                            }
                        }
                        lightmem::services::BundleProgress::StageDone { stage, summary } => {
                            if stage == 1 || stage == 2 {
                                eprintln!(" \x1b[38;2;34;197;94m✓ {}\x1b[0m", summary);
                            } else if stage == 3 {
                                progress_bar.finish("Archive finalized");
                            }
                        }
                        lightmem::services::BundleProgress::FileProgress {
                            current,
                            total,
                            filename,
                            ..
                        } => {
                            progress_bar.set_total(total);
                            progress_bar.update(current, &filename);
                        }
                    }
                })?;

                let manifest = lm.inspect_bundle(&exported_path)?;
                if !is_json {
                    eprintln!();
                }
                CliView::render_bundle_export(&exported_path, &manifest, is_json)?;
            }
            BundleCommand::Import {
                file,
                replace,
                merge,
                json: cmd_json,
            } => {
                let is_json = json || cmd_json;
                let lm = open_controller(effective_db, global, connection)?;
                let mut progress_bar = terminal::TerminalProgress::new("Extracting", 0);
                if !is_json {
                    eprintln!();
                    eprintln!(
                        "  {} {}",
                        CliView::crimson_bold("❖"),
                        CliView::white_bold("L I G H T M E M  Bundle Import")
                    );
                    eprintln!(
                        "  {}",
                        CliView::crimson(
                            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                        )
                    );
                    eprintln!(
                        "  {} {:<16} {}",
                        CliView::slate("◫"),
                        CliView::white_bold("Bundle Archive:"),
                        file.display()
                    );
                    eprintln!(
                        "  {} {:<16} {}",
                        CliView::slate("◈"),
                        CliView::white_bold("Mode:"),
                        if replace {
                            "Overwrite / Replace active vault (--replace)"
                        } else if merge {
                            "Merge into active vault (--merge)"
                        } else {
                            "Import into empty vault"
                        }
                    );
                    eprintln!();
                }

                let res = lm.import_bundle_with_progress(&file, replace, merge, |prog| {
                    if is_json {
                        return;
                    }
                    match prog {
                        lightmem::services::BundleProgress::StageStart {
                            stage,
                            total_stages,
                            title,
                        } => {
                            if stage == 1 || stage == 3 {
                                eprint!(
                                    "  \x1b[38;2;113;113;122m▸\x1b[0m [{}/{}] {:<42}",
                                    stage, total_stages, title
                                );
                                let _ = std::io::stderr().flush();
                            }
                        }
                        lightmem::services::BundleProgress::StageDone { stage, summary } => {
                            if stage == 1 || stage == 3 {
                                eprintln!(" \x1b[38;2;34;197;94m✓ {}\x1b[0m", summary);
                            } else if stage == 2 {
                                progress_bar.finish("All attachments extracted");
                            }
                        }
                        lightmem::services::BundleProgress::FileProgress {
                            current,
                            total,
                            filename,
                            ..
                        } => {
                            progress_bar.set_total(total);
                            progress_bar.update(current, &filename);
                        }
                    }
                })?;

                if !is_json {
                    eprintln!();
                }
                CliView::render_bundle_import(&res, is_json)?;
            }
            BundleCommand::Info {
                file,
                json: cmd_json,
            } => {
                let manifest = lightmem::services::bundle::BundleService::inspect_bundle(&file)?;
                CliView::render_bundle_info(&manifest, &file, json || cmd_json)?;
            }
        },

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
            let lm = open_controller(effective_db, global, connection)?;
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
            let result = match lm.answer_with_options(
                &question,
                cat,
                as_of_dt,
                date,
                limit,
                effective_reranker,
            ) {
                Ok(ans) => ans,
                Err(err) => {
                    CliView::render_llm_or_error(&err, json);
                    return Err(AlreadyHandledError.into());
                }
            };
            CliView::render_answer(&result, json)?;
        }

        Commands::Config {
            reload,
            action,
            edit,
            preset,
            backend,
            onnx_model,
            download,
            url,
            model,
            reranker,
            entity_extraction,
            ollama_api_key,
            dry_run,
            test,
            reset_db,
            yes,
            remote,
            api_key,
            clear_remote,
            json,
        } => {
            let is_reload = reload
                || action
                    .as_deref()
                    .map(|s| s.eq_ignore_ascii_case("reload") || s.eq_ignore_ascii_case("refresh"))
                    .unwrap_or(false);
            if is_reload {
                let reloaded = LightMemConfig::try_load()?;
                if json {
                    CliView::render_config(&reloaded, false, true, effective_db, global)?;
                } else {
                    println!(
                        "\n  {} Configuration reloaded from disk.",
                        CliView::emerald_bold("✓")
                    );
                    terminal::render_config(&reloaded, false, effective_db, global)?;
                }
                return Ok(());
            }
            if let Some(act) = action.as_deref() {
                anyhow::bail!("Unknown action '{}'. Did you mean 'lmem config reload' or 'lmem config --help'?", act);
            }
            let path = effective_db
                .map(Path::to_path_buf)
                .unwrap_or_else(|| LightMemConfig::resolve_db_path(global));
            if edit {
                anyhow::ensure!(
                    !cli.json,
                    "Use config flags with --json; --edit requires a terminal."
                );
                let Some(proposed) = terminal::edit_configuration(&cfg)? else {
                    return Ok(());
                };
                terminal::validate(&proposed)?;
                let plan = configuration_plan(&cfg, &proposed, &path)?;
                CliView::render_configuration_plan(&proposed, &plan, false)?;
                if terminal::confirm(
                    "Save these shared settings and approve selected-vault migration?",
                    false,
                )? {
                    // Abort if another process edited the settings while the prompt was open.
                    terminal::ensure_unchanged(&cfg)?;
                    if apply_configuration(&cfg, &proposed, &path, true, false)? {
                        terminal::render_config(&proposed, true, effective_db, global)?;
                    }
                } else {
                    eprintln!("Cancelled. No settings changed.");
                }
                return Ok(());
            }
            if test {
                // Provider diagnostics must not open or change the selected vault.
                let scratch = tempfile::tempdir()?;
                let lm = LightMem::open_at(&scratch.path().join("diagnostic.db"), cfg.clone())?;
                let dimensions = lm.test_embedding_provider()
                    .context("Embedding test failed. Check 'lmem config', model assets, endpoint, and credentials")?;
                let result = serde_json::json!({"status":"ok", "backend":cfg.backend,
                    "dimensions":dimensions, "memory_saved":false,
                    "tests":"local saved embedding configuration; answer quality and remote vault health are separate"});
                if json {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                } else {
                    println!();
                    println!(
                        "  {} {} {}",
                        CliView::emerald_bold("✓"),
                        CliView::white_bold("Embedding Engine Verified:"),
                        CliView::violet_bold(&format!(
                            "{} dimensions ({})",
                            dimensions, cfg.backend
                        ))
                    );
                    println!(
                        "    {} Status: Online and operational (disposable test; no memory saved).",
                        CliView::crimson("◈")
                    );
                    println!(
                        "    {} Tip:    Test answer reranking with '{}'.",
                        CliView::gold("💡"),
                        CliView::violet("lmem answer \"<question>\"")
                    );
                    println!();
                }
                return Ok(());
            }
            if reset_db {
                let lm = open_controller(effective_db, global, connection)?;
                let db_path = lm.db_path().to_path_buf();
                let removed = lm.clear_all(true)?;
                drop(lm);
                remove_db_files(&db_path);
                CliView::render_clear(removed, true, &db_path, json)?;
                return Ok(());
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
                        let _ = lightmem::OnnxQaReranker::ensure_model_downloaded(Some("tinyroberta-squad2"))?;
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
            let previous = cfg.clone();
            let mut changed = false;

            if let Some(ref p) = preset {
                cfg.apply_preset(p)?;
                changed = true;
            }
            if let Some(mode) = entity_extraction {
                let trimmed = mode.trim();
                let normalized = if trimmed.eq_ignore_ascii_case("off")
                    || trimmed.eq_ignore_ascii_case("false")
                {
                    "off".to_string()
                } else {
                    "on".to_string()
                };
                cfg.entity_extraction = normalized;
                changed = true;
            }
            if let Some(key) = ollama_api_key {
                let key = key.trim();
                cfg.ollama_api_key = if key.is_empty() || key.eq_ignore_ascii_case("none") {
                    None
                } else {
                    Some(key.into())
                };
                changed = true;
            }

            if let Some(ref b) = backend {
                let target_url = url
                    .as_ref()
                    .map(|u| LightMemConfig::normalize_ollama_url(u))
                    .unwrap_or_else(|| cfg.effective_ollama_url());
                if b == "ollama" && target_url == "https://ollama.com" {
                    anyhow::bail!(
                        "Ollama Cloud (https://ollama.com) does not provide an embedding endpoint; it only hosts generative LLMs.\n\
                         ◈ For vector search, keep '--backend onnx' (local ONNX, recommended) or connect to a local Ollama with '--url http://localhost:11434'.\n\
                         ◈ To use Ollama Cloud for question answering and reranking, run:\n\
                           lmem config --reranker ollama"
                    );
                }
                cfg.backend = b.clone();
                changed = true;
            }
            if let Some(om) = onnx_model {
                cfg.onnx_model = Some(om);
                changed = true;
            }
            if let Some(ref u) = url {
                cfg.ollama_url = LightMemConfig::normalize_ollama_url(u);
                changed = true;
            }
            // If Ollama is pointed to Ollama Cloud, but embedding backend was 'ollama',
            // safely keep/switch embeddings on local ONNX so we never fail trying to call non-existent cloud /api/embeddings.
            if backend.is_none()
                && cfg.backend == "ollama"
                && cfg.ollama_url == "https://ollama.com"
            {
                eprintln!(
                    "◈ Notice: Ollama Cloud (https://ollama.com) provides generative models for answering/reranking.\n\
                     ◈ Automatically keeping embedding backend on local ONNX ('bge-small') for fast offline search."
                );
                cfg.backend = "onnx".to_string();
                changed = true;
            }
            if let Some(m) = model {
                let lower = m.trim().to_lowercase();
                let is_local_switch = lower == "local" || lower == "offline" || lower == "onnx";
                let has_provider_prefix = lower.starts_with("openai:")
                    || lower.starts_with("openrouter:")
                    || lower.starts_with("gemini:")
                    || lower.starts_with("groq:")
                    || lower.starts_with("deepseek:")
                    || lower.starts_with("mistral:")
                    || lower.starts_with("ollama:")
                    || lower.starts_with("llm:")
                    || lower.starts_with("onnx:");

                if is_local_switch {
                    cfg.apply_preset("local")?;
                    changed = true;
                } else if has_provider_prefix
                    || (reranker.is_none()
                        && cfg.backend != "ollama"
                        && backend.as_deref() != Some("ollama"))
                {
                    cfg.reranker = parse_reranker_setting(&m)?;
                    if cfg.entity_extraction != "off" && !cfg.entity_extraction.is_empty() {
                        cfg.entity_extraction = "on".to_string();
                    }
                    changed = true;
                } else {
                    cfg.embedding_model = m;
                    changed = true;
                }
            }
            if let Some(r) = reranker {
                cfg.reranker = parse_reranker_setting(&r)?;
                changed = true;
            }

            if let Some(ref r) = remote {
                let trimmed = r.trim();
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                    cfg.remote_url = None;
                } else {
                    cfg.remote_url = Some(trimmed.trim_end_matches('/').to_string());
                }
                changed = true;
            }
            if let Some(k) = api_key {
                let trimmed = k.trim();
                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
                    cfg.api_key = None;
                    if cfg.remote_url.is_none() && remote.is_none() {
                        cfg.ollama_api_key = None;
                    }
                } else {
                    cfg.api_key = Some(trimmed.to_string());
                    if cfg.remote_url.is_none() && remote.is_none() {
                        cfg.ollama_api_key = Some(trimmed.to_string());
                    }
                }
                changed = true;
            }
            if clear_remote {
                cfg.remote_url = None;
                cfg.api_key = None;
                changed = true;
            }

            terminal::validate(&cfg)?;
            if dry_run {
                let plan = configuration_plan(&previous, &cfg, &path)?;
                return CliView::render_configuration_plan(&cfg, &plan, json);
            }
            if changed && !apply_configuration(&previous, &cfg, &path, yes, json)? {
                return Ok(());
            }
            if json {
                CliView::render_config(&cfg, changed, true, effective_db, global)?;
            } else {
                terminal::render_config(&cfg, changed, effective_db, global)?;
            }
        }

        Commands::Setup {
            preset,
            url,
            model,
            entity_extraction,
            dry_run,
            yes,
            json,
        } => {
            let previous = LightMemConfig::try_load()?;
            let mut cfg = previous.clone();
            let interactive = io::stdin().is_terminal() && io::stderr().is_terminal() && !json;
            let explicit =
                preset.is_some() || url.is_some() || model.is_some() || entity_extraction.is_some();
            if let Some(preset) = preset {
                cfg.apply_preset(&preset)?;
            } else if !explicit {
                anyhow::ensure!(interactive && !yes,
                    "Choose settings explicitly: lmem setup --preset local --dry-run (or --preset demo for no downloads). Add --yes to save.");
                cfg = terminal::setup_configuration(&previous)?;
            }
            if let Some(url) = url {
                cfg.ollama_url = LightMemConfig::normalize_ollama_url(&url);
            }
            if let Some(model) = model {
                cfg.reranker = parse_reranker_setting(&model)?;
            }
            if let Some(mode) = entity_extraction {
                let trimmed = mode.trim();
                let normalized = if trimmed.eq_ignore_ascii_case("off")
                    || trimmed.eq_ignore_ascii_case("false")
                {
                    "off".to_string()
                } else {
                    "on".to_string()
                };
                cfg.entity_extraction = normalized;
            }
            let path = effective_db
                .map(Path::to_path_buf)
                .unwrap_or_else(|| LightMemConfig::resolve_db_path(global));
            terminal::validate(&cfg)?;
            let plan = configuration_plan(&previous, &cfg, &path)?;
            if dry_run {
                return CliView::render_configuration_plan(&cfg, &plan, json);
            }
            if !yes {
                anyhow::ensure!(interactive,
                    "Setup needs confirmation. Preview with --dry-run, then repeat with --yes to save. No settings changed.");
                CliView::render_configuration_plan(&cfg, &plan, false)?;
                eprint!("Save these shared settings and approve selected-vault migration if embeddings change? [y/N] ");
                io::stderr().flush()?;
                let mut response = String::new();
                io::stdin().read_line(&mut response)?;
                if !matches!(response.trim().to_lowercase().as_str(), "y" | "yes") {
                    eprintln!("Setup cancelled. Configuration and embeddings were not changed.");
                    return Ok(());
                }
            }
            terminal::ensure_unchanged(&previous)?;
            if !apply_configuration(&previous, &cfg, &path, true, json)? {
                return Ok(());
            }
            if json {
                CliView::render_config(&cfg, true, true, effective_db, global)?;
            } else {
                terminal::render_config(&cfg, true, effective_db, global)?;
                if interactive {
                    terminal::offer_downloads(&cfg)?;
                }
                terminal::completion_hint();
            }
        }

        Commands::Reindex { yes, json } => {
            let lm = open_controller(effective_db, global, connection)?;
            let migrated = migrate_embeddings(&lm, yes, json, true)?;
            if json {
                let obj = serde_json::json!({
                    "status": if migrated { "ok" } else { "cancelled" },
                    "embedding_identity": lm.requested_embedding_identity(),
                    "db": lm.db_path().display().to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&obj)?);
            }
        }

        Commands::Stats { json } => {
            let lm = open_controller(effective_db, global, connection)?;
            let stats = lm.stats()?;
            CliView::render_stats(&stats, lm.db_path(), json)?;
        }

        Commands::Conflicts {
            yes,
            reranker,
            min_similarity,
            json,
        } => {
            let lm = open_controller(effective_db, global, connection)?;
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
                            merged.id.chars().take(8).collect::<String>(),
                            c.older_memory.id.chars().take(8).collect::<String>(),
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
            let lm = open_controller(effective_db, global, connection)?;
            let (src_id, dst_id) = lm.link(&source, &target, Some(&relation), Some(weight))?;
            CliView::render_link(&src_id, &dst_id, &relation, weight, json)?;
        }

        Commands::Unlink {
            source,
            target,
            relation,
            json,
        } => {
            let lm = open_controller(effective_db, global, connection)?;
            let removed = lm.unlink(&source, &target, relation.as_deref())?;
            CliView::render_unlink(&source, &target, relation.as_deref(), removed, json)?;
        }

        Commands::Related { id, hops, json } => {
            let lm = open_controller(effective_db, global, connection)?;
            let related = lm.related(&id, hops)?;
            CliView::render_related(&id, &related, json)?;
        }

        Commands::Graph { focus, hops, json } => {
            let lm = open_controller(effective_db, global, connection)?;
            let snapshot = lm.graph(focus.as_deref(), Some(hops))?;
            CliView::render_graph(&snapshot, json)?;
        }

        Commands::Autolink {
            min_similarity,
            json,
        } => {
            let lm = open_controller(effective_db, global, connection)?;
            let new_links = lm.autolink(Some(min_similarity))?;
            let snapshot = lm.graph(None, None)?;
            CliView::render_autolink(new_links, snapshot.nodes.len(), snapshot.edges.len(), json)?;
        }

        Commands::Observe {
            input,
            file,
            yes,
            dry_run,
            category,
            tags,
            confidence,
            reranker,
            json,
        } => {
            let lm = open_controller(effective_db, global, connection)?;
            if !dry_run && !migrate_embeddings(&lm, false, json, false)? {
                return Ok(());
            }

            // 1. Resolve raw text from file, argument, or stdin
            let raw_text = if let Some(ref path) = file {
                std::fs::read_to_string(path).with_context(|| {
                    format!("Failed to read conversation file: {}", path.display())
                })?
            } else if let Some(ref arg) = input {
                if arg == "-" {
                    let mut buf = String::new();
                    io::stdin().read_to_string(&mut buf)?;
                    buf
                } else if Path::new(arg).is_file() {
                    std::fs::read_to_string(arg)
                        .with_context(|| format!("Failed to read file: {}", arg))?
                } else {
                    arg.clone()
                }
            } else if !io::stdin().is_terminal() {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                anyhow::bail!("Provide conversation text or a file path (or pipe via stdin): `lmem observe \"User: I prefer dark mode\"`");
            };

            let parsed_tags: Option<Vec<String>> = tags.map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            });

            let candidates = lm.observe_and_extract(
                &raw_text,
                category,
                parsed_tags.as_deref(),
                Some(confidence),
                reranker.as_deref(),
            )?;

            if candidates.is_empty() {
                CliView::render_observed_candidates(&[], dry_run, json)?;
                return Ok(());
            }

            // 2. Dry-run mode: display or output JSON without saving
            if dry_run {
                CliView::render_observed_candidates(&candidates, true, json)?;
                return Ok(());
            }

            // 3. Batch / Non-interactive confirmation mode (--yes)
            if yes {
                let saved_records = lm.save_observed_candidates(&candidates)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&saved_records)?);
                } else {
                    CliView::render_observed_candidates(&candidates, false, false)?;
                    CliView::render_observed_summary(saved_records.len(), 0, false, false)?;
                }
                return Ok(());
            }

            // 4. Non-interactive terminal without --yes (e.g. piped or JSON inspection)
            if json || !io::stdin().is_terminal() {
                CliView::render_observed_candidates(&candidates, true, json)?;
                return Ok(());
            }

            // 5. Interactive terminal step-by-step confirmation review
            CliView::render_observed_candidates(&candidates, false, false)?;
            println!(
                "  {}",
                "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                    .bright_black()
            );
            println!("  Reviewing candidate memories one by one:\n");

            let mut saved_count = 0usize;
            let mut skipped_count = 0usize;
            let mut force_all = false;

            for (i, c) in candidates.iter().enumerate() {
                println!(
                    "  {} [{}] {} {} {}",
                    CliView::crimson_bold(&format!("[Candidate {}/{}]", i + 1, candidates.len())),
                    CliView::violet_bold(c.category.as_str()),
                    CliView::white_bold(&c.title),
                    CliView::emerald_bold(&format!(
                        "({}% conf)",
                        (c.confidence * 100.0).round() as u32
                    )),
                    CliView::slate(&format!("[{}]", c.provenance))
                );
                println!(
                    "    Content: {}",
                    CliView::gold(&format!("\"{}\"", c.content))
                );

                let should_save = if force_all {
                    true
                } else {
                    print!("  Save this memory? [y]es / [n]o (skip) / [a]ll (--yes) / [q]uit: ");
                    io::stdout().flush()?;
                    let mut answer = String::new();
                    io::stdin().read_line(&mut answer)?;
                    match answer.trim().to_lowercase().as_str() {
                        "y" | "yes" => true,
                        "a" | "all" => {
                            force_all = true;
                            true
                        }
                        "q" | "quit" => {
                            println!(
                                "  Stopped candidate review ({} saved, {} skipped).",
                                saved_count, skipped_count
                            );
                            break;
                        }
                        _ => false,
                    }
                };

                if should_save {
                    let record = lm.save_observed_candidate(c)?;
                    saved_count += 1;
                    println!(
                        "  {} Saved memory {} ({}): \"{}\"\n",
                        CliView::emerald_bold("✓"),
                        CliView::gold(&record.id.chars().take(8).collect::<String>()),
                        CliView::violet_bold(record.category.as_str()),
                        record.title
                    );
                } else {
                    skipped_count += 1;
                    println!("  {} Skipped.\n", CliView::slate("↷"));
                }
            }

            CliView::render_observed_summary(saved_count, skipped_count, false, false)?;
        }

        Commands::Connect {
            platform,
            workspace,
            path,
            list,
            json,
            mcp,
        } => {
            if let Some(connection) = connection {
                if list {
                    CliView::render_platforms_list(
                        &lightmem::ConnectService::list_platforms(workspace),
                        workspace,
                        json,
                    )?;
                } else {
                    let results = lightmem::ConnectService::connect_bound(
                        connection,
                        platform.as_deref(),
                        workspace,
                        path.as_deref(),
                        mcp,
                    )?;
                    CliView::render_connect(&results, json)?;
                }
            } else if mcp {
                let results =
                    lightmem::ConnectService::connect_mcp(platform.as_deref(), workspace)?;
                CliView::render_connect(&results, json)?;
            } else if list {
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

        #[cfg(feature = "mcp")]
        Commands::Mcp => {
            let server = lightmem::McpServer::new(global, effective_db.map(Path::to_path_buf))
                .with_connection(connection.cloned());
            server.run_stdio()?;
        }

        Commands::Completions { shell } => print_completions(shell)?,

        Commands::Version { json } => {
            CliView::render_version(json)?;
        }

        Commands::Docs {
            port,
            landing,
            no_browser,
        } => {
            let options = lightmem::DocsOptions {
                port,
                open_landing: landing,
                no_browser,
            };
            lightmem::serve_docs(options)?;
        }

        Commands::Serve {
            host,
            port,
            api_key,
            no_auth,
            daemon,
            stop,
            status,
        } => {
            handle_serve(
                host,
                port,
                api_key,
                no_auth,
                daemon,
                stop,
                status,
                effective_db,
                global,
                connection,
                cli.db.as_deref(),
                cli.connection.as_deref(),
            )?;
        }

        Commands::Uninstall {
            purge_data,
            yes,
            json,
        } => {
            handle_uninstall(purge_data, yes, json)?;
        }

        Commands::History {
            query,
            limit,
            status,
            clear,
            json,
        } => {
            if clear {
                let count = HistoryService::clear()?;
                CliView::render_history_cleared(count, json || cli.json)?;
            } else {
                let (entries, total) =
                    HistoryService::list(query.as_deref(), status.as_deref(), limit)?;
                CliView::render_history(
                    &entries,
                    total,
                    &HistoryService::db_path(),
                    json || cli.json,
                )?;
            }
        }
    }

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct ServerDaemonInfo {
    pid: u32,
    host: String,
    port: u16,
    started_at: String,
}

#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn is_pid_alive(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid)])
        .output();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        text.contains(&pid.to_string())
    } else {
        false
    }
}

#[cfg(unix)]
fn terminate_pid(pid: u32) -> bool {
    unsafe {
        let _ = libc::kill(pid as i32, libc::SIGTERM);
    }
    for _ in 0..30 {
        if !is_pid_alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    unsafe {
        let _ = libc::kill(pid as i32, libc::SIGKILL);
    }
    !is_pid_alive(pid)
}

#[cfg(windows)]
fn terminate_pid(pid: u32) -> bool {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status();
    !is_pid_alive(pid)
}

#[allow(clippy::too_many_arguments)]
fn handle_serve(
    host: String,
    port: u16,
    api_key: Option<String>,
    no_auth: bool,
    daemon: bool,
    stop: bool,
    status: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&lightmem::ResolvedConnection>,
    cli_db: Option<&Path>,
    cli_connection: Option<&str>,
) -> Result<()> {
    let pid_file = LightMemConfig::config_dir().join("server.pid");

    if stop {
        if !pid_file.exists() {
            println!("\x1b[38;2;148;163;184mLightMem server daemon is not running (no PID file found).\x1b[0m");
            return Ok(());
        }
        let content = std::fs::read_to_string(&pid_file)?;
        let info: Result<ServerDaemonInfo, _> = serde_json::from_str(&content);
        let pid = match info {
            Ok(ref d) => d.pid,
            Err(_) => content
                .trim()
                .parse::<u32>()
                .context("Invalid PID in server.pid")?,
        };

        if is_pid_alive(pid) {
            if terminate_pid(pid) {
                let _ = std::fs::remove_file(&pid_file);
                println!(
                    "  \x1b[1;32m✓\x1b[0m LightMem server daemon stopped (PID: {}).",
                    pid
                );
            } else {
                anyhow::bail!(
                    "Failed to terminate LightMem server process with PID {}",
                    pid
                );
            }
        } else {
            let _ = std::fs::remove_file(&pid_file);
            println!(
                "\x1b[38;2;148;163;184mLightMem server daemon was not running (cleaned up stale PID file: {}).\x1b[0m",
                pid
            );
        }
        return Ok(());
    }

    if status {
        if !pid_file.exists() {
            println!(
                "  \x1b[1;38;2;220;38;38m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M  Daemon Status\x1b[0m"
            );
            println!(
                "  \x1b[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m"
            );
            println!("  \x1b[1;38;2;239;68;68m▸\x1b[0m State:         \x1b[1;31mSTOPPED\x1b[0m");
            println!("  \x1b[38;2;113;113;122m▸\x1b[0m Command:       lmem serve -d\n");
            return Ok(());
        }
        let content = std::fs::read_to_string(&pid_file)?;
        let info: Result<ServerDaemonInfo, _> = serde_json::from_str(&content);
        let (pid, host_str, port_num, started) = match info {
            Ok(ref d) => (d.pid, d.host.clone(), d.port, Some(d.started_at.clone())),
            Err(_) => {
                let p = content
                    .trim()
                    .parse::<u32>()
                    .context("Invalid PID in server.pid")?;
                (p, "127.0.0.1".to_string(), 8080, None)
            }
        };

        if is_pid_alive(pid) {
            let probe_url = format!("http://127.0.0.1:{}/v1/health", port_num);
            let responding = ureq::get(&probe_url)
                .timeout(std::time::Duration::from_millis(500))
                .call()
                .is_ok();
            println!(
                "\n  \x1b[1;38;2;34;197;94m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M  Daemon Status\x1b[0m"
            );
            println!(
                "  \x1b[38;2;34;197;94m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m"
            );
            println!(
                "  \x1b[1;38;2;34;197;94m▸\x1b[0m State:         \x1b[1;32mRUNNING\x1b[0m {}",
                if responding {
                    "\x1b[38;2;148;163;184m(HTTP 200 OK)\x1b[0m"
                } else {
                    "\x1b[38;2;234;179;8m(Starting...)\x1b[0m"
                }
            );
            println!(
                "  \x1b[38;2;113;113;122m▸\x1b[0m Process PID:       {}",
                pid
            );
            if host_str == "0.0.0.0" {
                println!(
                    "  \x1b[38;2;113;113;122m▸\x1b[0m Listening:         0.0.0.0:{} (all interfaces)",
                    port_num
                );
                println!("  \x1b[38;2;113;113;122m▸\x1b[0m Network Endpoints:");
                let endpoints = lightmem::services::api_server::discover_host_endpoints();
                for ep in &endpoints {
                    println!(
                        "    \x1b[38;2;113;113;122m•\x1b[0m \x1b[1m{:<12}\x1b[0m http://{}:{} ({})",
                        ep.label, ep.ip, port_num, ep.if_name
                    );
                }
            } else {
                println!(
                    "  \x1b[38;2;113;113;122m▸\x1b[0m Address:           http://{}:{}",
                    host_str, port_num
                );
            }
            if let Some(s) = started {
                println!("  \x1b[38;2;113;113;122m▸\x1b[0m Started:       {}", s);
            }
            println!(
                "  \x1b[38;2;113;113;122m▸\x1b[0m Logs:          {}/server.log",
                LightMemConfig::config_dir().display()
            );
            println!("  \x1b[38;2;113;113;122m▸\x1b[0m Stop:          lmem serve --stop\n");
        } else {
            let _ = std::fs::remove_file(&pid_file);
            println!(
                "  \x1b[1;38;2;220;38;38m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M  Daemon Status\x1b[0m"
            );
            println!(
                "  \x1b[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m"
            );
            println!(
                "  \x1b[1;38;2;239;68;68m▸\x1b[0m State:         \x1b[1;31mSTOPPED\x1b[0m \x1b[38;2;148;163;184m(stale PID {} cleaned up)\x1b[0m",
                pid
            );
            println!("  \x1b[38;2;113;113;122m▸\x1b[0m Command:       lmem serve -d\n");
        }
        return Ok(());
    }

    if daemon {
        anyhow::ensure!(
            no_auth || api_key.as_ref().is_none_or(|k| !k.trim().is_empty()),
            "API key cannot be blank. Omit it to generate one, or explicitly use --no-auth."
        );
        if pid_file.exists() {
            let content = std::fs::read_to_string(&pid_file).unwrap_or_default();
            let pid = serde_json::from_str::<ServerDaemonInfo>(&content)
                .map(|d| d.pid)
                .or_else(|_| content.trim().parse::<u32>())
                .unwrap_or(0);
            if pid > 0 && is_pid_alive(pid) {
                anyhow::bail!(
                    "LightMem server daemon is already running (PID: {}). Run 'lmem serve --stop' first.",
                    pid
                );
            }
            let _ = std::fs::remove_file(&pid_file);
        }

        let key = if no_auth {
            None
        } else {
            Some(api_key.unwrap_or_else(|| {
                let u = uuid::Uuid::new_v4().to_string().replace('-', "");
                format!("lmem_{}", u)
            }))
        };

        let config_dir = LightMemConfig::config_dir();
        std::fs::create_dir_all(&config_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config_dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let log_path = config_dir.join("server.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("Failed to open log file {}", log_path.display()))?;
        let err_file = log_file.try_clone()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            log_file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }

        let exe = std::env::current_exe()?;
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("serve")
            .arg("--host")
            .arg(&host)
            .arg("--port")
            .arg(port.to_string());

        if let Some(ref k) = key {
            cmd.arg("--api-key").arg(k);
        }
        if no_auth {
            cmd.arg("--no-auth");
        }
        if let Some(db) = cli_db {
            cmd.arg("--db").arg(db);
        }
        if global {
            cmd.arg("--global");
        }
        if let Some(conn) = cli_connection {
            cmd.arg("--connection").arg(conn);
        }
        if let Some(resolved) = connection {
            cmd.arg("--scope").arg(match resolved.context.scope {
                lightmem::MemoryScope::User => "user",
                lightmem::MemoryScope::Project => "project",
                lightmem::MemoryScope::Agent => "agent",
            });
            if let Some(session) = &resolved.context.session_id {
                cmd.arg("--session").arg(session);
            }
        }

        cmd.stdin(std::process::Stdio::null())
            .stdout(log_file)
            .stderr(err_file);

        #[cfg(unix)]
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }

        let child = cmd
            .spawn()
            .context("Failed to spawn background daemon process")?;
        let child_pid = child.id();

        // Brief delay to ensure it didn't exit immediately due to bind error
        std::thread::sleep(std::time::Duration::from_millis(300));

        if !is_pid_alive(child_pid) {
            let err_msg = std::fs::read_to_string(&log_path)
                .ok()
                .and_then(|logs| {
                    logs.lines()
                        .rev()
                        .take(5)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n")
                        .into()
                })
                .unwrap_or_default();
            if err_msg.contains("Address already in use")
                || err_msg.contains("os error 98")
                || err_msg.contains("os error 48")
                || err_msg.contains("os error 10048")
            {
                let alt_port = if port == 8080 { 8089 } else { port + 1 };
                eprintln!(
                    "\n{} {}",
                    "⚠️".yellow(),
                    format!("Cannot start daemon on {}:{}", host, port)
                        .yellow()
                        .bold()
                );
                eprintln!(
                    "  {} {:<12} Port {} is already in use by another process.",
                    "◈".dimmed(),
                    "Reason:".white().bold(),
                    port
                );
                eprintln!();
                eprintln!("  {} {}", "💡".dimmed(), "How to fix:".white().bold());
                eprintln!(
                    "    {} {:<18} {}",
                    "•".dimmed(),
                    "Use another port:",
                    format!("lmem serve -d --port {}", alt_port).cyan()
                );
                eprintln!(
                    "    {} {:<18} {}",
                    "•".dimmed(),
                    "Check active daemon:",
                    "lmem serve --status".yellow()
                );
                eprintln!(
                    "    {} {:<18} {}",
                    "•".dimmed(),
                    "Stop running daemon:",
                    "lmem serve --stop".yellow()
                );
                eprintln!();
                std::process::exit(1);
            }
            anyhow::bail!(
                "LightMem server daemon failed to start (process exited immediately).\nLogs:\n{}",
                err_msg
            );
        }

        let daemon_info = ServerDaemonInfo {
            pid: child_pid,
            host: host.clone(),
            port,
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        std::fs::write(&pid_file, serde_json::to_string_pretty(&daemon_info)?)?;

        println!(
            "\n  \x1b[1;38;2;34;197;94m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M  Daemon Started\x1b[0m"
        );
        println!(
            "  \x1b[38;2;34;197;94m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m"
        );
        println!(
            "  \x1b[1;38;2;34;197;94m▸\x1b[0m PID:               {}",
            child_pid
        );
        if host == "0.0.0.0" {
            println!(
                "  \x1b[38;2;113;113;122m▸\x1b[0m Listening:         0.0.0.0:{} (all interfaces)",
                port
            );
            println!("  \x1b[38;2;113;113;122m▸\x1b[0m Network Endpoints:");
            let endpoints = lightmem::services::api_server::discover_host_endpoints();
            for ep in &endpoints {
                println!(
                    "    \x1b[38;2;113;113;122m•\x1b[0m \x1b[1m{:<12}\x1b[0m http://{}:{} ({})",
                    ep.label, ep.ip, port, ep.if_name
                );
            }
        } else {
            println!(
                "  \x1b[38;2;113;113;122m▸\x1b[0m Listening:         http://{}:{}",
                host, port
            );
        }
        if let Some(ref k) = key {
            println!(
                "  \x1b[38;2;113;113;122m▸\x1b[0m API Key:           \x1b[1;38;2;234;179;8m{}\x1b[0m  (Keep this secret!)",
                k
            );
            println!("  \x1b[38;2;113;113;122m▸\x1b[0m Quick Connect (Examples):");
            if host == "0.0.0.0" {
                let endpoints = lightmem::services::api_server::discover_host_endpoints();
                for ep in &endpoints {
                    println!(
                        "    \x1b[38;2;113;113;122m•\x1b[0m {:<12} lmem remote http://{}:{} --api-key \"{}\"",
                        ep.label, ep.ip, port, k
                    );
                }
            } else {
                println!(
                    "    \x1b[38;2;113;113;122m•\x1b[0m {:<12} lmem remote http://{}:{} --api-key \"{}\"",
                    "Remote", host, port, k
                );
            }
        } else {
            println!("  \x1b[38;2;113;113;122m▸\x1b[0m Authentication:    None (--no-auth)");
        }
        println!(
            "  \x1b[38;2;113;113;122m▸\x1b[0m Health Probe:  http://127.0.0.1:{}/v1/health",
            port
        );
        println!(
            "  \x1b[38;2;113;113;122m▸\x1b[0m Logs:          {}",
            log_path.display()
        );
        println!("  \x1b[38;2;113;113;122m▸\x1b[0m Status:        lmem serve --status");
        println!("  \x1b[38;2;113;113;122m▸\x1b[0m Stop:          lmem serve --stop\n");

        return Ok(());
    }

    // Foreground mode
    let lm = open_controller(effective_db, global, connection)?;
    let options = lightmem::ServerOptions {
        host: host.clone(),
        port,
        api_key,
        no_auth,
    };
    if let Err(e) = lightmem::serve_api(Arc::new(lm), options) {
        let err_str = format!("{e:#}");
        if err_str.contains("Address already in use")
            || err_str.contains("os error 98")
            || err_str.contains("os error 48")
            || err_str.contains("os error 10048")
        {
            let alt_port = if port == 8080 { 8089 } else { port + 1 };
            eprintln!(
                "\n{} {}",
                "⚠️".yellow(),
                format!("Cannot start server on {}:{}", host, port)
                    .yellow()
                    .bold()
            );
            eprintln!(
                "  {} {:<12} Port {} is already in use by another process.",
                "◈".dimmed(),
                "Reason:".white().bold(),
                port
            );
            eprintln!();
            eprintln!("  {} {}", "💡".dimmed(), "How to fix:".white().bold());
            eprintln!(
                "    {} {:<18} {}",
                "•".dimmed(),
                "Use another port:",
                format!("lmem serve --port {}", alt_port).cyan()
            );
            eprintln!(
                "    {} {:<18} {}",
                "•".dimmed(),
                "Run as daemon:",
                format!("lmem serve -d --port {}", alt_port).cyan()
            );
            eprintln!(
                "    {} {:<18} {}",
                "•".dimmed(),
                "Check active daemon:",
                "lmem serve --status".yellow()
            );
            eprintln!(
                "    {} {:<18} {}",
                "•".dimmed(),
                "Stop running daemon:",
                "lmem serve --stop".yellow()
            );
            eprintln!();
            std::process::exit(1);
        }
        return Err(e);
    }
    Ok(())
}

fn handle_uninstall(purge_data: bool, yes: bool, json: bool) -> Result<()> {
    if json {
        anyhow::ensure!(
            yes,
            "Non-interactive uninstallation via --json requires --yes"
        );
    } else {
        println!("\n  \x1b[1;38;2;220;38;38m❖\x1b[0m  \x1b[1;38;2;248;250;252mL I G H T M E M\x1b[0m  \x1b[38;2;113;113;122mUninstaller\x1b[0m");
        println!("  \x1b[38;2;220;38;38m━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m");
        println!("  \x1b[1;38;2;220;38;38m▸\x1b[0m \x1b[38;2;228;228;231mRemoving LightMem engine, binaries & shell completions\x1b[0m");
        println!(
            "  \x1b[38;2;113;113;122m────────────────────────────────────────────────\x1b[0m\n"
        );

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
        if bin.exists() && std::fs::remove_file(bin).is_ok() {
            println!("  \x1b[1;32m◈\x1b[0m Removed binary: {}", bin.display());
            removed_count += 1;
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

    if json {
        let obj = serde_json::json!({
            "status": "uninstalled",
            "purged_data": purge_data,
        });
        println!("{}", serde_json::to_string_pretty(&obj)?);
        return Ok(());
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

fn print_completions(shell: clap_complete::Shell) -> Result<()> {
    use clap::builder::PossibleValuesParser;
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    cmd = cmd.mut_arg("scope", |a| {
        a.value_parser(PossibleValuesParser::new(["user", "project", "agent"]))
    });

    cmd = cmd.mut_subcommand("remember", |sub| {
        sub.mut_arg("ttl", |a| {
            a.value_parser(PossibleValuesParser::new(["1h", "24h", "7d", "30d"]))
        })
        .mut_arg("provenance", |a| {
            a.value_parser(PossibleValuesParser::new([
                "explicit_statement",
                "validated",
                "observed",
                "corrected",
                "inferred",
                "imported",
                "attachment_metadata",
                "attachment_llm_extracted",
            ]))
        })
    });
    cmd = cmd.mut_subcommand("config", |sub| {
        sub.mut_arg("preset", |a| {
            a.value_parser(PossibleValuesParser::new([
                "demo",
                "local",
                "ollama-cloud",
                "ollama-local",
            ]))
        })
        .mut_arg("backend", |a| {
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
                "qa",
                "all",
            ]))
        })
        .mut_arg("reranker", |a| {
            a.value_parser(PossibleValuesParser::new([
                "minilm-squad2",
                "tinyroberta-squad2",
                "onnx",
                "ollama",
                "ollama:bonsai-2:2b",
                "ollama:gemma4:31b-cloud",
                "ollama:qwen2.5:3b",
                "ollama:qwen2.5:1.5b",
                "ollama:llama3.2:1b",
                "top1",
            ]))
        })
    });
    cmd = cmd.mut_subcommand("setup", |sub| {
        sub.mut_arg("preset", |a| {
            a.value_parser(PossibleValuesParser::new([
                "demo",
                "local",
                "ollama-local",
                "ollama-cloud",
            ]))
        })
        .mut_arg("model", |a| {
            a.value_parser(PossibleValuesParser::new([
                "minilm-squad2",
                "tinyroberta-squad2",
                "ollama",
                "ollama:qwen2.5:3b",
                "top1",
            ]))
        })
    });
    cmd = cmd.mut_subcommand("answer", |sub| {
        sub.mut_arg("reranker", |a| {
            a.value_parser(PossibleValuesParser::new([
                "onnx",
                "ollama",
                "ollama:bonsai-2:2b",
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
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut cmd, "lmem", &mut script);
    let script = String::from_utf8(script)?;
    print!("{}", terminal::complete_connections(shell, script));
    Ok(())
}
