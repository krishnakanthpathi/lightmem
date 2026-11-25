mod commands;
mod terminal;

use anyhow::Result;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use clap::{Parser, Subcommand};
use lightmem::{parse_ttl_duration, CliView, HistoryService, MemoryStatus, MemoryType};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lmem")]
#[command(
    about = "❖ LightMem - Ultra-fast, lightweight agent memory engine",
    long_about = None,
    before_help = CliView::banner_string(None, None)
)]
#[command(disable_version_flag = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Force use of global database (~/.lightmem/memories.db)
    #[arg(short = 'g', long, global = true)]
    pub global: bool,

    /// Registered user/agent/harness connection (or LIGHTMEM_CONNECTION)
    #[arg(long, global = true)]
    pub connection: Option<String>,

    /// Override the connection scope: user, project, or agent
    #[arg(long, global = true)]
    pub scope: Option<lightmem::MemoryScope>,

    /// Conversation/session ID (associates new memories or filters recall, list, answer, and export)
    #[arg(long, global = true)]
    pub session: Option<String>,

    /// Explicit path to SQLite database file
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,

    /// Remote LightMem backend server URL (e.g. http://192.168.1.100:8080)
    #[arg(long, global = true)]
    pub remote: Option<String>,

    /// Secret API key for remote LightMem backend authentication
    #[arg(long, global = true)]
    pub api_key: Option<String>,

    /// Print version information (use with --json for machine-readable JSON)
    #[arg(short = 'V', long = "version")]
    pub version: bool,

    /// Output as JSON across all subcommands
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// View all managed memory vaults, active connections, and storage telemetry
    #[command(visible_alias = "vaults")]
    Status,

    /// Register and inspect users, agents, harnesses, and named connections
    Identity {
        #[command(subcommand)]
        action: IdentityCommand,
    },

    /// Show the active connection, scope, source identity, and database
    Context,

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

        /// Optional model override when --extract is enabled (e.g. 'ollama:gemma4:31b-cloud', 'gpt-4o-mini')
        #[arg(
            short = 'm',
            long = "model",
            value_name = "MODEL",
            visible_alias = "answer-model",
            alias = "reranker",
            short_alias = 'r'
        )]
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

        /// Filter by tag (repeatable or comma-separated)
        #[arg(long = "tag", visible_alias = "tags")]
        tag: Vec<String>,

        /// Filter by extracted entity ('name' or 'name=val')
        #[arg(long = "entity", value_parser = lightmem::EntityFilter::parse)]
        entity: Option<lightmem::EntityFilter>,

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

        /// Toggle: Run precision QA extractive answering on recalled memories
        #[arg(long)]
        precision: bool,

        /// Expand recall hits with their 1-hop connected neighbors in the knowledge graph
        #[arg(long)]
        multi_hop: bool,

        /// Export verified attachment binary file(s) of recalled memories to a target directory or file path
        #[arg(short = 'o', long = "save-to", visible_alias = "output")]
        save_to: Option<PathBuf>,

        /// Materialize verified attachment(s) and print only their absolute file path(s) (one per line)
        #[arg(short = 'p', long = "path", visible_alias = "paths")]
        path: bool,

        /// Open recalled attachment file(s) immediately using system default application
        #[arg(long)]
        open: bool,
    },

    /// List memories chronologically with pagination (limit, offset, page, total)
    List {
        /// Filter by category
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Filter by tag (repeatable or comma-separated)
        #[arg(long = "tag", visible_alias = "tags")]
        tag: Vec<String>,

        /// Filter by extracted entity ('name' or 'name=val')
        #[arg(long = "entity", value_parser = lightmem::EntityFilter::parse)]
        entity: Option<lightmem::EntityFilter>,

        /// Filter by attachment media kind (image, audio, video, document, code, archive, binary, or any)
        #[arg(short = 'm', long = "media")]
        media: Option<String>,

        /// Filter by status (active | expired | all)
        #[arg(long, default_value = "active")]
        status: ListStatusFilter,

        /// Sort order (newest | oldest | updated | confidence)
        #[arg(long, default_value = "newest")]
        sort: lightmem::ListSort,

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
        #[arg(short = 'o', long = "save-to", visible_alias = "output")]
        save_to: Option<PathBuf>,

        /// Materialize verified attachment(s) and print only their absolute file path(s) (one per line)
        #[arg(short = 'p', long = "path", visible_alias = "paths")]
        path: bool,

        /// Open listed attachment file(s) immediately using system default application
        #[arg(long)]
        open: bool,
    },

    /// Deeply inspect one or more memory records, their metadata, attachments, and knowledge graph links
    #[command(alias = "get", alias = "show")]
    Inspect {
        /// Memory ID(s) (UUID or prefix) or title(s)
        #[arg(required = true, num_args = 1..)]
        terms: Vec<String>,

        /// Export verified attachment binary file(s) from the SQLite vault to a target file or directory path on disk
        #[arg(
            short = 'o',
            long = "save-to",
            visible_alias = "output",
            visible_alias = "export-attachments"
        )]
        save_to: Option<PathBuf>,

        /// Materialize verified attachment(s) and print only their absolute file path(s) (one per line)
        #[arg(short = 'p', long = "path", visible_alias = "paths")]
        path: bool,

        /// Open inspected attachment file(s) immediately using system default application
        #[arg(long)]
        open: bool,
    },

    /// Update an existing memory's content, title, category, tags, TTL, provenance, or attachments in-place
    #[command(alias = "edit")]
    Update {
        /// Memory ID (UUID or prefix) or exact title
        id: String,

        /// New content for the memory
        #[arg(short = 'c', long)]
        content: Option<String>,

        /// New short title
        #[arg(long)]
        title: Option<String>,

        /// New memory category
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Replace all tags with comma-separated tags
        #[arg(long)]
        tags: Option<String>,

        /// Add one or more tags without removing existing tags
        #[arg(long = "add-tag", visible_alias = "add-tags")]
        add_tags: Vec<String>,

        /// Remove one or more tags without replacing remaining tags
        #[arg(
            long = "remove-tag",
            visible_alias = "rm-tag",
            visible_alias = "remove-tags"
        )]
        remove_tags: Vec<String>,

        /// New confidence score (0.0 to 1.0)
        #[arg(long)]
        confidence: Option<f32>,

        /// Set or update expiration TTL duration (e.g. '24h', '7d')
        #[arg(long, conflicts_with = "clear_ttl", value_parser = parse_ttl_duration)]
        ttl: Option<chrono::Duration>,

        /// Clear existing expiration TTL so the memory remains active
        #[arg(long = "clear-ttl", visible_alias = "no-ttl")]
        clear_ttl: bool,

        /// Update provenance / verification level
        #[arg(short = 'p', long)]
        provenance: Option<String>,

        /// Attach additional files to the memory
        #[arg(short = 'f', long = "file", visible_alias = "attach")]
        files: Vec<PathBuf>,

        /// Re-extract structured entities via configured LLM
        #[arg(long = "refresh-entities")]
        refresh_entities: bool,
    },

    /// Deduplicate identical or near-duplicate memories by merging tags, titles, and timestamps
    #[command(alias = "dedup")]
    Deduplicate,

    /// Forget or expire one or more memories by ID/prefix/title (or all memories with --all)
    Forget {
        /// Memory ID(s), prefix(es), or unique exact title(s) to forget (omit when using --all)
        #[arg(num_args = 0..)]
        ids: Vec<String>,

        /// Forget all memories in the database
        #[arg(long, conflicts_with = "ids")]
        all: bool,

        /// Permanently delete instead of soft-retiring
        #[arg(long)]
        hard: bool,

        /// Approve forgetting all memories without interactive confirmation
        #[arg(short = 'y', long, visible_alias = "force")]
        yes: bool,
    },

    /// Restore an archived memory by ID, prefix, or exact title (clears its old TTL)
    Restore { term: String },

    /// Delete / wipe the entire memory database (aliases: purge, reset)
    #[command(visible_aliases = ["purge", "reset"])]
    Clear {
        /// Soft-retire all memories instead of permanently deleting the database
        #[arg(long)]
        soft: bool,

        /// Approve wiping without interactive confirmation
        #[arg(short = 'y', long, visible_alias = "force")]
        yes: bool,
    },

    /// Export memories to Open Knowledge Format (OKF), lossless JSON backup, or compressed DB transfer bundle (.zip)
    Export {
        /// Target export format: json, okf, or bundle
        #[arg(long, value_parser = ["json", "okf", "bundle"], conflicts_with_all = ["okf", "bundle"])]
        format: Option<String>,

        /// Export to Open Knowledge Format bundle (shorthand for --format okf)
        #[arg(long, conflicts_with = "bundle")]
        okf: bool,

        /// Export full database transfer bundle (shorthand for --format bundle)
        #[arg(long, conflicts_with = "okf")]
        bundle: bool,

        /// Output file path
        #[arg(short = 'o', long, visible_alias = "save-to")]
        output: Option<PathBuf>,

        /// Filter exported memories by category (json/okf only)
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Filter exported memories by tag (repeatable or comma-separated; json/okf only)
        #[arg(long = "tag", visible_alias = "tags")]
        tags: Vec<String>,

        /// Filter exported memories by status (active | expired | all; json/okf only)
        #[arg(long)]
        status: Option<ListStatusFilter>,

        /// Filter exported memories by extracted entity ('name' or 'name=val'; json/okf only)
        #[arg(long = "entity", value_parser = lightmem::EntityFilter::parse)]
        entity: Option<lightmem::EntityFilter>,

        /// Emit operation summary as machine-readable JSON (kept for backwards compatibility)
        #[arg(long = "output-json", visible_alias = "print-json", hide = true)]
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
        #[arg(long, conflicts_with = "merge")]
        replace: bool,

        /// Merge bundle into existing memories instead of requiring an empty vault or complete replacement
        #[arg(long, conflicts_with = "replace")]
        merge: bool,
    },

    /// Manage portable compressed transfer bundles (.zip) with DB, video/photo files, and embeddings
    Bundle {
        #[command(subcommand)]
        action: BundleCommand,
    },

    /// Ask a question and synthesize/extract the factual answer (via local ONNX Extractive QA or Ollama)
    Answer {
        /// The question to answer
        question: String,

        /// Toggle: Force use of precision local extractive QA
        #[arg(long)]
        precision: bool,

        /// Answer model or provider override for this query (e.g. 'openai:gpt-4o-mini', 'gemini', 'ollama:qwen2.5', 'onnx', 'top1')
        #[arg(
            short = 'm',
            long = "model",
            value_name = "MODEL",
            visible_alias = "answer-model",
            alias = "reranker",
            short_alias = 'r'
        )]
        reranker: Option<String>,

        /// Filter candidate memories by category
        #[arg(short = 't', long = "type")]
        category: Option<MemoryType>,

        /// Filter candidate memories by tag (repeatable or comma-separated)
        #[arg(long = "tag", visible_alias = "tags")]
        tag: Vec<String>,

        /// Filter candidate memories by extracted entity ('name' or 'name=val')
        #[arg(long = "entity", value_parser = lightmem::EntityFilter::parse)]
        entity: Option<lightmem::EntityFilter>,

        /// Point-in-time view (YYYY-MM-DD or RFC3339)
        #[arg(long, value_parser = parse_as_of_date)]
        as_of: Option<DateTime<Utc>>,

        /// Filter candidate memories to a specific calendar day (YYYY-MM-DD)
        #[arg(long, value_parser = parse_exact_date)]
        date: Option<NaiveDate>,

        /// Max candidate memories to retrieve for answering
        #[arg(short = 'l', long, default_value = "10")]
        limit: usize,
    },

    #[command(hide = true, name = "__connections")]
    CompletionConnections,

    /// Inspect local configuration and index readiness without downloads or writes
    Doctor,

    /// View or switch the active answer model and provider
    Model {
        /// Target provider or model (e.g. 'openai', 'gemini', 'groq', 'openrouter', 'deepseek', 'mistral', 'ollama', 'local', 'custom')
        provider_or_model: Option<String>,

        /// Optional API key when configuring a cloud provider
        #[arg(
            long = "api-key",
            visible_alias = "key",
            visible_alias = "model-api-key"
        )]
        api_key: Option<String>,

        /// Optional custom endpoint URL (required for 'custom' provider)
        #[arg(long = "endpoint", visible_alias = "url")]
        endpoint: Option<String>,
    },

    /// View shared settings, preview changes, or test the saved embedding backend
    Config {
        /// Reload configuration from disk and display updated settings
        #[arg(long, visible_alias = "refresh")]
        reload: bool,

        /// Optional sub-action: 'reload', 'refresh', 'path', 'reset', 'get', or 'set'
        #[arg(index = 1)]
        action: Option<String>,

        /// Config field name for 'config get <KEY>' or 'config set <KEY> <VALUE>'
        #[arg(index = 2)]
        action_key: Option<String>,

        /// Config value for 'config set <KEY> <VALUE>'
        #[arg(index = 3)]
        action_value: Option<String>,

        /// Change a setting using short terminal prompts; no full-screen interface
        #[arg(long, conflicts_with_all = ["yes", "preset", "backend", "onnx_model", "download", "url", "model", "reranker", "entity_extraction", "ollama_api_key", "reset_db", "test", "remote", "api_key", "clear_remote", "dry_run", "reload"])]
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

        /// OpenAI-compatible answer server URL or endpoint (e.g. https://api.openai.com, http://localhost:11434)
        #[arg(long = "endpoint", visible_alias = "url")]
        url: Option<String>,

        /// Setup preset: local, ollama, openai, gemini, groq, openrouter, deepseek, mistral, custom, or demo
        #[arg(long, visible_alias = "profile", visible_alias = "provider")]
        preset: Option<String>,

        /// Vector embedding model name (e.g. 'bge-small', 'nomic-embed-text')
        #[arg(long = "embedding-model", alias = "model")]
        model: Option<String>,

        /// Dedicated endpoint URL for Ollama or custom vector embeddings (separate from answer endpoint)
        #[arg(long)]
        embedding_endpoint: Option<String>,

        /// Dedicated API key for remote vector embedding endpoint
        #[arg(long)]
        embedding_api_key: Option<String>,

        /// Model for answers and entity extraction (e.g. 'gpt-4o-mini', 'gemini-1.5-flash', 'ollama:qwen2.5:3b')
        #[arg(
            long = "answer-model",
            value_name = "MODEL",
            alias = "reranker",
            visible_alias = "openai-model",
            visible_alias = "chat-model"
        )]
        reranker: Option<String>,

        /// Toggle entity extraction ('on' or 'off') using the configured generative model
        #[arg(long)]
        entity_extraction: Option<String>,

        /// Save an answer generation model API key (OpenAI, Gemini, Groq, Ollama, etc.); use 'none' to remove
        #[arg(
            long = "model-api-key",
            value_name = "KEY",
            visible_alias = "ollama-api-key",
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
        #[arg(short = 'y', long)]
        yes: bool,

        /// Remote LightMem backend server URL (e.g. http://my-server:8080)
        #[arg(long, visible_alias = "remote-url", conflicts_with = "clear_remote")]
        remote: Option<String>,

        /// Secret API key for authenticating with the remote LightMem server
        #[arg(
            long = "api-key",
            visible_alias = "remote-api-key",
            conflicts_with = "clear_remote"
        )]
        api_key: Option<String>,

        /// Clear saved remote server configuration and revert to local mode
        #[arg(long, conflicts_with_all = ["remote", "api_key"])]
        clear_remote: bool,
    },

    /// Guided terminal setup with a configuration preview before saving
    Setup {
        /// Setup preset: local, ollama, openai, gemini, groq, openrouter, deepseek, mistral, custom, or demo
        #[arg(long)]
        preset: Option<String>,

        /// OpenAI / Ollama server URL (e.g. http://localhost:11434, https://api.openai.com)
        #[arg(long = "endpoint", visible_alias = "url")]
        url: Option<String>,

        /// Answer engine spec (e.g. openai:gpt-4o-mini, ollama:qwen2.5:3b, minilm-squad2, or top1)
        #[arg(long = "model", visible_alias = "answer-model")]
        model: Option<String>,

        /// Optional API key when setting up a cloud provider
        #[arg(
            long = "api-key",
            visible_alias = "key",
            visible_alias = "model-api-key"
        )]
        api_key: Option<String>,

        /// Override entity extraction toggle ('on' or 'off') independently of the preset
        #[arg(long)]
        entity_extraction: Option<String>,

        /// Preview setup without saving, downloading, or reindexing
        #[arg(long)]
        dry_run: bool,

        /// Approve prompts without confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Rebuild embeddings for the configured model, with progress
    Reindex {
        /// Explicitly approve migration without an interactive prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Display storage statistics and active database path
    Stats,

    /// Find overlapping or conflicting memories via vector similarity + token overlap, and interactively (or with --yes) merge them
    Conflicts {
        /// Force-merge all detected conflict/overlap pairs without prompting
        #[arg(short = 'y', long, visible_alias = "resolve")]
        yes: bool,

        /// Override model used for merging (e.g. 'openai:gpt-4o-mini', 'ollama:qwen2.5', 'top1')
        #[arg(
            short = 'm',
            long = "model",
            value_name = "MODEL",
            visible_alias = "answer-model",
            alias = "reranker",
            short_alias = 'r'
        )]
        reranker: Option<String>,

        /// Minimum similarity threshold to flag as overlapping/conflicting (0.0 - 1.0)
        #[arg(long, default_value = "0.78")]
        min_similarity: f32,
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
    },

    /// List incoming and outgoing relationship links for a specific memory
    Links {
        /// Memory ID, title, or prefix
        id: String,
    },

    /// List all distinct tags in the active vault and their memory counts
    Tags {
        /// Optional search query to filter tags
        #[arg(default_value = "")]
        query: String,
    },

    /// List extracted entities or query memories referencing an entity
    #[command(visible_alias = "entity")]
    Entities {
        /// Optional entity name (or 'name=value') to filter
        #[arg(index = 1)]
        name: Option<String>,

        /// Entity value filter (shows matching memory records)
        #[arg(long)]
        val: Option<String>,

        /// Show matching memory records instead of entity summary counts
        #[arg(long)]
        memories: bool,
    },

    /// Traverse and list multi-hop connected memories from a starting node
    Related {
        /// Starting memory ID, title, or prefix
        id: String,

        /// Maximum hop distance to traverse (1 to 5)
        #[arg(short = 'n', long, default_value = "2")]
        hops: usize,
    },

    /// Terminal network tree or JSON knowledge graph
    Graph {
        /// Focus the graph on a specific memory neighborhood (ID or title)
        #[arg(short = 'f', long)]
        focus: Option<String>,

        /// Maximum hop radius when focused on a memory neighborhood
        #[arg(short = 'n', long, default_value = "2")]
        hops: usize,
    },

    /// Automatically discover and connect memories via title mentions, shared tags, and vector similarity
    #[command(alias = "auto-link")]
    Autolink {
        /// Minimum cosine similarity threshold for semantic links (0.1 to 1.0)
        #[arg(long, default_value = "0.75")]
        min_similarity: f32,
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

        /// Override model used for extraction (e.g. 'ollama:qwen2.5:3b', 'gpt-4o-mini')
        #[arg(
            short = 'm',
            long = "model",
            value_name = "MODEL",
            visible_alias = "answer-model",
            alias = "reranker",
            short_alias = 'r'
        )]
        reranker: Option<String>,
    },

    /// Connect and deploy LightMem agent skill to AI environments (Antigravity, Codex, Hermes, Cursor, Claude, Agents)
    #[command(visible_aliases = ["skill", "skills", "agent"])]
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
    Version,

    /// Switch to local mode (clears saved remote URL and API key)
    #[command(visible_aliases = ["disconnect"])]
    Local,

    /// Connect to a remote LightMem server or switch to remote mode
    #[command(visible_aliases = ["connect-server", "server-connect"])]
    Remote {
        /// Remote LightMem backend server URL (e.g. http://localhost:8080)
        url: Option<String>,

        /// Secret API key for remote server authentication
        #[arg(long)]
        api_key: Option<String>,
    },

    /// Launch a persistent HTTP backend server for remote multi-device memory access
    #[command(visible_aliases = ["server"])]
    Serve {
        /// Host interface to bind (defaults to 127.0.0.1; specify 0.0.0.0 for LAN/remote access)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to listen on
        #[arg(short = 'p', long, default_value = "8080")]
        port: u16,

        /// Secret API key required for client requests (auto-generated if omitted)
        #[arg(long = "api-key", visible_alias = "server-key")]
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
    },

    /// View or clear the history of executed CLI commands
    #[command(visible_alias = "hist")]
    History {
        /// Optional search query or subcommand filter (e.g. 'recall', 'remember', or keywords)
        #[arg(index = 1)]
        query: Option<String>,

        /// Maximum number of history entries to display
        #[arg(short = 'l', short_alias = 'n', long, default_value = "20")]
        limit: usize,

        /// Filter commands by status: 'success' (ok) or 'error' (failed)
        #[arg(short = 's', long)]
        status: Option<String>,

        /// Clear all recorded command history
        #[arg(long)]
        clear: bool,
    },
}

pub fn parse_as_of_date(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
    let naive_dt = date.and_hms_opt(23, 59, 59).unwrap();
    Ok(Utc.from_utc_datetime(&naive_dt))
}

pub fn parse_exact_date(s: &str) -> Result<NaiveDate> {
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
pub enum BundleCommand {
    /// Export active vault into a single compressed transfer .zip file containing the database and all video/photo media files
    Export {
        /// Output file path (.zip or .lmbundle)
        #[arg(short = 'o', long, visible_alias = "save-to")]
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
        #[arg(long, conflicts_with = "merge")]
        replace: bool,

        /// Merge bundle into existing memories instead of replacing
        #[arg(long, conflicts_with = "replace")]
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum ListStatusFilter {
    Active,
    Expired,
    All,
}

impl ListStatusFilter {
    pub fn into_memory_status(self) -> Option<MemoryStatus> {
        match self {
            ListStatusFilter::Active => Some(MemoryStatus::Active),
            ListStatusFilter::Expired => Some(MemoryStatus::Expired),
            ListStatusFilter::All => None,
        }
    }
}

pub fn parse_tag_list(tags: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for raw in tags {
        for part in raw.split(',') {
            let trimmed = part.trim();
            if !trimmed.is_empty() && !out.iter().any(|t: &String| t.eq_ignore_ascii_case(trimmed))
            {
                out.push(trimmed.to_string());
            }
        }
    }
    out
}

#[derive(Subcommand)]
pub enum IdentityCommand {
    UserAdd {
        id: String,
        #[arg(long)]
        name: Option<String>,
    },
    UserRemove {
        id: String,
    },
    AgentAdd {
        id: String,
        #[arg(long)]
        user: String,
        #[arg(long)]
        name: Option<String>,
    },
    AgentRemove {
        id: String,
    },
    HarnessAdd {
        id: String,
        #[arg(short = 'p', long)]
        path: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
    },
    HarnessRemove {
        id: String,
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
    ConnectionUpdate {
        id: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, conflicts_with = "project")]
        clear_project: bool,
        #[arg(long)]
        memory_scope: Option<lightmem::MemoryScope>,
    },
    ConnectionRemove {
        id: String,
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

fn should_record_history(cli: &Cli) -> bool {
    if cli.version {
        return false;
    }
    !matches!(
        &cli.command,
        None | Some(Commands::Completions { .. })
            | Some(Commands::CompletionConnections)
            | Some(Commands::Doctor)
            | Some(Commands::History { .. })
            | Some(Commands::Config { .. })
            | Some(Commands::Model { .. })
            | Some(Commands::Setup { .. })
    )
}

fn extract_subcommand_name(cli: &Cli) -> String {
    match &cli.command {
        Some(Commands::Status) => "status",
        Some(Commands::Identity { .. }) => "identity",
        Some(Commands::Context) => "context",
        Some(Commands::Remember { .. }) => "remember",
        Some(Commands::Recall { .. }) => "recall",
        Some(Commands::List { .. }) => "list",
        Some(Commands::Inspect { .. }) => "inspect",
        Some(Commands::Update { .. }) => "update",
        Some(Commands::Deduplicate) => "deduplicate",
        Some(Commands::Forget { .. }) => "forget",
        Some(Commands::Restore { .. }) => "restore",
        Some(Commands::Clear { .. }) => "clear",
        Some(Commands::Export { .. }) => "export",
        Some(Commands::Import { .. }) => "import",
        Some(Commands::Bundle { .. }) => "bundle",
        Some(Commands::Answer { .. }) => "answer",
        Some(Commands::Doctor) => "doctor",
        Some(Commands::Model { .. }) => "model",
        Some(Commands::Config { .. }) => "config",
        Some(Commands::Setup { .. }) => "setup",
        Some(Commands::Reindex { .. }) => "reindex",
        Some(Commands::Stats) => "stats",
        Some(Commands::Conflicts { .. }) => "conflicts",
        Some(Commands::Link { .. }) => "link",
        Some(Commands::Unlink { .. }) => "unlink",
        Some(Commands::Links { .. }) => "links",
        Some(Commands::Tags { .. }) => "tags",
        Some(Commands::Entities { .. }) => "entities",
        Some(Commands::Related { .. }) => "related",
        Some(Commands::Graph { .. }) => "graph",
        Some(Commands::Autolink { .. }) => "autolink",
        Some(Commands::Observe { .. }) => "observe",
        Some(Commands::Connect { .. }) => "connect",
        #[cfg(feature = "mcp")]
        Some(Commands::Mcp) => "mcp",
        Some(Commands::Completions { .. }) => "completions",
        Some(Commands::CompletionConnections) => "__connections",
        Some(Commands::Version) => "version",
        Some(Commands::Local) => "local",
        Some(Commands::Remote { .. }) => "remote",
        Some(Commands::Serve { .. }) => "serve",
        Some(Commands::Uninstall { .. }) => "uninstall",
        Some(Commands::History { .. }) => "history",
        None => "intro",
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
    let run_res = commands::run(cli);
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
        if !err.is::<commands::AlreadyHandledError>() {
            if CliView::try_render_known_error(&err, json) {
                std::process::exit(1);
            }
            if json {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "error": format!("{err:#}"),
                    })
                );
            } else {
                eprintln!("Error: {err}");
            }
        }
        std::process::exit(1);
    }
}
