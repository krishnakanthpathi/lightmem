//! Loopback-only management workspace. The HTTP API owns authentication and origin checks.
use super::api_server::{send_error, send_json, HttpRequest};
use crate::{
    BundleManifest, ConnectService, IdentityRegistry, LightMem, LightMemConfig, MemoryContext,
    MemoryScope,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, RwLock,
};

pub(crate) fn asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    match path {
        "/logo.png" => Some((include_bytes!("../../static/logo.png"), "image/png")),
        _ => None,
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct Job {
    id: String,
    kind: String,
    status: String,
    done: usize,
    total: usize,
    message: String,
}

struct RestorePreview {
    id: String,
    _directory: tempfile::TempDir,
    path: PathBuf,
    revision: i64,
    vault: PathBuf,
}

pub(crate) struct WorkspaceState {
    controller: RwLock<Arc<LightMem>>,
    initial_path: PathBuf,
    initial_context: Option<MemoryContext>,
    pub busy: AtomicBool,
    job: Mutex<Option<Job>>,
    restore: Mutex<Option<RestorePreview>>,
    last_backup: Mutex<Option<String>>,
    model_test: Mutex<Option<Value>>,
    base_config: Mutex<Option<Vec<u8>>>,
    pub(crate) mutation: Mutex<()>,
}

impl WorkspaceState {
    pub(crate) fn new(controller: Arc<LightMem>) -> Self {
        Self {
            initial_path: controller.db_path().to_path_buf(),
            initial_context: controller.storage().source_context().cloned(),
            controller: RwLock::new(controller),
            busy: AtomicBool::new(false),
            job: Mutex::new(None),
            restore: Mutex::new(None),
            last_backup: Mutex::new(None),
            model_test: Mutex::new(None),
            base_config: Mutex::new(std::fs::read(LightMemConfig::config_file()).ok()),
            mutation: Mutex::new(()),
        }
    }

    pub(crate) fn controller(&self) -> Result<Arc<LightMem>> {
        let guard = self
            .controller
            .read()
            .map_err(|_| anyhow::anyhow!("Workspace state unavailable"))?;
        Ok(Arc::clone(&guard))
    }

    fn start_job(
        self: &Arc<Self>,
        kind: &str,
        work: impl FnOnce(&Arc<Self>) -> Result<String> + Send + 'static,
    ) -> Result<Job> {
        anyhow::ensure!(
            !self.busy.swap(true, Ordering::SeqCst),
            "A model or restore operation is already running"
        );
        let job = Job {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.into(),
            status: "running".into(),
            done: 0,
            total: 0,
            message: "Starting…".into(),
        };
        *self
            .job
            .lock()
            .map_err(|_| anyhow::anyhow!("Job state unavailable"))? = Some(job.clone());
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&state)))
                .unwrap_or_else(|_| {
                    Err(anyhow::anyhow!(
                        "Operation stopped unexpectedly; inspect the vault before retrying"
                    ))
                });
            if let Ok(mut slot) = state.job.lock() {
                if let Some(job) = slot.as_mut() {
                    match result {
                        Ok(message) => {
                            job.status = "complete".into();
                            job.message = message;
                        }
                        Err(e) => {
                            job.status = "failed".into();
                            job.message = format!("{e:#}");
                        }
                    }
                }
            }
            state.busy.store(false, Ordering::SeqCst);
        });
        Ok(job)
    }

    fn progress(&self, done: usize, total: usize) {
        if let Ok(mut slot) = self.job.lock() {
            if let Some(job) = slot.as_mut() {
                job.done = done;
                job.total = total;
                job.message = format!("Indexing {done} of {total} memories");
            }
        }
    }

    fn ensure_configuration_current(&self) -> Result<()> {
        let current = match std::fs::read(LightMemConfig::config_file()) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        anyhow::ensure!(current == *self.base_config.lock().map_err(|_| anyhow::anyhow!("Configuration state unavailable"))?, "Configuration changed outside this workspace. Reload configuration before applying settings.");
        Ok(())
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ConfigPatch {
    preset: Option<String>,
    backend: Option<String>,
    onnx_model: Option<String>,
    ollama_url: Option<String>,
    embedding_model: Option<String>,
    reranker: Option<String>,
    entity_extraction: Option<String>,
    ollama_api_key: Option<String>,
    #[serde(default)]
    clear_ollama_key: bool,
    #[serde(default)]
    migrate: bool,
}

fn candidate_config(controller: &LightMem, patch: &ConfigPatch) -> Result<LightMemConfig> {
    let mut cfg = controller.config().clone();
    if let Some(preset) = &patch.preset {
        cfg.apply_preset(preset)?;
    }
    if let Some(v) = &patch.backend {
        anyhow::ensure!(
            matches!(v.as_str(), "onnx" | "ollama" | "hash"),
            "Invalid embedding backend"
        );
        cfg.backend = v.clone();
    }
    if let Some(v) = &patch.onnx_model {
        anyhow::ensure!(
            matches!(v.as_str(), "bge-small" | "minilm" | "nomic"),
            "Select a supported ONNX model"
        );
        cfg.onnx_model = Some(v.clone());
    }
    if let Some(v) = &patch.ollama_url {
        let url = LightMemConfig::normalize_ollama_url(v);
        anyhow::ensure!(
            (url.starts_with("http://") || url.starts_with("https://"))
                && !url.contains(['\r', '\n', '@'])
                && url.len() <= 2048,
            "Use an HTTP(S) endpoint without embedded credentials"
        );
        cfg.ollama_url = url;
    }
    if let Some(v) = &patch.embedding_model {
        anyhow::ensure!(
            !v.trim().is_empty() && v.len() <= 200,
            "Embedding model is required"
        );
        cfg.embedding_model = v.trim().into();
    }
    if let Some(v) = &patch.reranker {
        anyhow::ensure!(
            matches!(
                v.as_str(),
                "minilm-squad2" | "tinyroberta-squad2" | "top1" | "ollama"
            ) || (v.starts_with("ollama:") && v.len() > 7 && v.len() <= 200),
            "Invalid answer engine"
        );
        cfg.reranker = v.clone();
    }
    if let Some(v) = &patch.entity_extraction {
        anyhow::ensure!(
            matches!(v.as_str(), "off" | "llm"),
            "Entity extraction must be off or llm"
        );
        cfg.entity_extraction = v.clone();
    }
    if let Some(key) = &patch.ollama_api_key {
        anyhow::ensure!(
            !key.trim().is_empty() && key != "[REDACTED]",
            "Leave the key omitted to keep it; use clear to remove it"
        );
        cfg.ollama_api_key = Some(key.trim().into());
    }
    if patch.clear_ollama_key {
        cfg.ollama_api_key = None;
    }
    anyhow::ensure!(
        cfg.backend != "ollama" || cfg.effective_ollama_url() != "https://ollama.com",
        "Ollama Cloud does not provide embeddings. Keep embeddings local."
    );
    cfg.embedding_identity()?;
    Ok(cfg)
}

fn body<T: serde::de::DeserializeOwned>(req: &HttpRequest) -> Result<T> {
    serde_json::from_slice(&req.body).context("Invalid request body")
}

fn status(state: &WorkspaceState) -> Result<Value> {
    let controller = state.controller()?;
    let registry = IdentityRegistry::open_default()?;
    let connections = registry.list_resolved_connections()?;
    let config = controller.config();
    let current_config = LightMemConfig::try_load()?;
    let config_changed = serde_json::to_value(config)? != serde_json::to_value(current_config)?;
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"), "target": "This computer", "db_path": controller.db_path(),
        "os": std::env::consts::OS, "binary_path": std::env::current_exe()?,
        "context": controller.storage().source_context(), "initial_path": state.initial_path,
        "stats": controller.stats()?, "config": config.redacted_json()?,
        "config_file": LightMemConfig::config_file(), "config_changed_externally": config_changed,
        "embedding_summary": config.active_embedding_summary(), "answer_summary": config.active_reranker_summary(),
        "needs_reindex": controller.embedding_migration_needed()?, "connections": connections,
        "platforms": ConnectService::list_platforms(false), "model_cache": LightMemConfig::config_dir().join("models"),
        "busy": state.busy.load(Ordering::SeqCst), "job": *state.job.lock().map_err(|_| anyhow::anyhow!("Job state unavailable"))?,
        "last_backup_this_session": *state.last_backup.lock().map_err(|_| anyhow::anyhow!("Backup state unavailable"))?,
        "model_test": *state.model_test.lock().map_err(|_| anyhow::anyhow!("Test state unavailable"))?,
        "environment": { "offline": std::env::var_os("LIGHTMEM_OFFLINE").is_some(), "qa_disabled": std::env::var_os("LIGHTMEM_QA_DISABLE").is_some(), "ollama_key_from_environment": config.ollama_api_key.is_none() && config.effective_ollama_api_key().is_some(), "remote_url_configured": config.effective_remote_url().is_some() },
        "storage_protection": "Local SQLite with file permissions; not encrypted at rest"
    }))
}

pub(crate) fn handle(mut stream: TcpStream, req: &HttpRequest, state: &Arc<WorkspaceState>) {
    // Configuration/registry/restore actions are available only on a loopback-bound server.
    if !stream.local_addr().is_ok_and(|a| a.ip().is_loopback())
        || !stream.peer_addr().is_ok_and(|a| a.ip().is_loopback())
    {
        send_error(
            stream,
            403,
            "Forbidden",
            "Management requires a loopback connection.",
        );
        return;
    }
    // Bundle downloads are streamed so large media vaults do not require another full in-memory copy.
    if req.method == "POST" && req.path == "/v1/workspace/backup" {
        let result = (|| -> Result<_> {
            anyhow::ensure!(
                !state.busy.load(Ordering::SeqCst),
                "Wait for the current operation to finish"
            );
            let dir = tempfile::tempdir()?;
            let path = state
                .controller()?
                .export_bundle(Some(&dir.path().join("lightmem-backup.zip")))?;
            let file = std::fs::File::open(path)?;
            Ok((dir, file))
        })();
        match result {
            Ok((_dir, mut file)) => {
                use std::io::Write;
                let length = file.metadata().map(|m| m.len()).unwrap_or(0);
                let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Disposition: attachment; filename=lightmem-backup.zip\r\nContent-Length: {length}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n");
                if stream.write_all(header.as_bytes()).is_ok()
                    && std::io::copy(&mut file, &mut stream).is_ok()
                {
                    if let Ok(mut time) = state.last_backup.lock() {
                        *time = Some(chrono::Utc::now().to_rfc3339());
                    }
                }
            }
            Err(e) => send_error(stream, 400, "Bad Request", &format!("{e:#}")),
        }
        return;
    }
    let result = dispatch(req, state);
    match result {
        Ok(value) => send_json(stream, 200, "OK", &value),
        Err(e) => send_error(stream, 400, "Bad Request", &format!("{e:#}")),
    }
}

fn dispatch(req: &HttpRequest, state: &Arc<WorkspaceState>) -> Result<Value> {
    if req.method == "GET" && req.path == "/v1/workspace/status" {
        return status(state);
    }
    anyhow::ensure!(req.method == "POST", "Unknown workspace endpoint");
    anyhow::ensure!(
        !state.busy.load(Ordering::SeqCst),
        "Wait for the current model or restore operation to finish"
    );
    let controller = state.controller()?;
    match req.path.as_str() {
        "/v1/workspace/config/preview" | "/v1/workspace/config" => {
            state.ensure_configuration_current()?;
            let patch: ConfigPatch = body(req)?;
            let cfg = candidate_config(&controller, &patch)?;
            let candidate = controller.reconfigured(cfg.clone())?;
            let needs_reindex = candidate.embedding_migration_needed()?;
            if req.path.ends_with("/preview") {
                return Ok(
                    json!({"config":cfg.redacted_json()?, "needs_reindex":needs_reindex, "records":controller.stats()?.total_memories, "global_settings":true, "vault":controller.db_path()}),
                );
            }
            anyhow::ensure!(
                !needs_reindex || patch.migrate,
                "Changing embeddings requires explicit reindex approval for this vault"
            );
            let job = state.start_job("configuration", move |state| {
                if needs_reindex {
                    candidate.reindex(|done, total| state.progress(done, total))?;
                }
                cfg.save()?;
                *state
                    .base_config
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Configuration state unavailable"))? =
                    Some(std::fs::read(LightMemConfig::config_file())?);
                *state
                    .controller
                    .write()
                    .map_err(|_| anyhow::anyhow!("Workspace state unavailable"))? =
                    Arc::new(candidate);
                *state
                    .model_test
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Test state unavailable"))? = None;
                Ok("Configuration applied. Other vaults may need reindexing when opened.".into())
            })?;
            Ok(json!(job))
        }
        "/v1/workspace/reindex" => {
            let job = state.start_job("reindex", move |state| {
                let count = controller.reindex(|done, total| state.progress(done, total))?;
                Ok(format!("Reindexed {count} memories"))
            })?;
            Ok(json!(job))
        }
        "/v1/workspace/models/download" => {
            #[derive(Deserialize)]
            struct Download {
                model: String,
            }
            let request: Download = body(req)?;
            anyhow::ensure!(
                matches!(
                    request.model.as_str(),
                    "bge-small" | "minilm" | "nomic" | "minilm-squad2" | "tinyroberta-squad2"
                ),
                "Unknown model"
            );
            let job = state.start_job("model download", move |_| {
                match request.model.as_str() {
                    "minilm-squad2" | "tinyroberta-squad2" => {
                        crate::OnnxQaReranker::ensure_model_downloaded(Some(&request.model))?;
                    }
                    _ => {
                        crate::OnnxEmbeddingProvider::new(Some(&request.model))?;
                    }
                }
                Ok(format!("{} is ready in the model cache", request.model))
            })?;
            Ok(json!(job))
        }
        "/v1/workspace/test" => {
            let dimensions = controller.test_embedding_provider()?;
            *state
                .model_test
                .lock()
                .map_err(|_| anyhow::anyhow!("Test state unavailable"))? =
                Some(json!({"at":chrono::Utc::now().to_rfc3339(), "dimensions":dimensions}));
            Ok(
                json!({"ok":true,"message":format!("Search backend responded successfully ({dimensions} dimensions). No memory was stored.")}),
            )
        }
        "/v1/workspace/memory/restore" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct RestoreMemory {
                id: String,
            }
            let request: RestoreMemory = body(req)?;
            Ok(json!(controller.restore_memory(&request.id)?))
        }
        "/v1/workspace/vault" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Select {
                connection: Option<String>,
                scope: Option<MemoryScope>,
            }
            let request: Select = body(req)?;
            let cfg = controller.config().clone();
            let selected = if let Some(id) = request.connection {
                let resolved =
                    IdentityRegistry::open_default()?.resolve(&id, request.scope, None)?;
                let mut selected = LightMem::open_at(&resolved.db_path, cfg)?;
                selected = selected.reconfigured_with_context(resolved.context)?;
                selected
            } else {
                anyhow::ensure!(request.scope.is_none(), "Scope requires a connection");
                let selected = LightMem::open_at(&state.initial_path, cfg)?;
                if let Some(context) = &state.initial_context {
                    selected.reconfigured_with_context(context.clone())?
                } else {
                    selected
                }
            };
            *state
                .controller
                .write()
                .map_err(|_| anyhow::anyhow!("Workspace state unavailable"))? = Arc::new(selected);
            *state
                .restore
                .lock()
                .map_err(|_| anyhow::anyhow!("Restore state unavailable"))? = None;
            *state
                .model_test
                .lock()
                .map_err(|_| anyhow::anyhow!("Test state unavailable"))? = None;
            status(state)
        }
        "/v1/workspace/connections" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Register {
                id: String,
                user: String,
                agent: String,
                harness: String,
                project: Option<String>,
                scope: MemoryScope,
            }
            let request: Register = body(req)?;
            for id in [&request.id, &request.user, &request.agent, &request.harness] {
                crate::models::identity::validate_id(id)?;
            }
            if let Some(project) = &request.project {
                crate::models::identity::validate_id(project)?;
            }
            let registry = IdentityRegistry::open_default()?;
            registry.add_user(&request.user, None)?;
            registry.add_agent(&request.agent, &request.user, None)?;
            Ok(json!(registry.add_connection(
                &request.id,
                &request.user,
                &request.agent,
                &request.harness,
                request.project,
                request.scope
            )?))
        }
        "/v1/workspace/connections/install" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Install {
                id: String,
                mcp: bool,
                confirm: bool,
            }
            let request: Install = body(req)?;
            anyhow::ensure!(
                request.confirm,
                "Confirm installation after reviewing the target tool and configuration path"
            );
            let resolved = IdentityRegistry::open_default()?.resolve(&request.id, None, None)?;
            Ok(json!(ConnectService::connect_bound(
                &resolved,
                None,
                false,
                None,
                request.mcp
            )?))
        }
        "/v1/workspace/restore/preview" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Upload {
                data_base64: String,
            }
            let upload: Upload = body(req)?;
            let bytes = crate::decode_base64(&upload.data_base64).map_err(anyhow::Error::msg)?;
            anyhow::ensure!(
                bytes.len() <= 40 * 1024 * 1024,
                "Workspace uploads are limited to 40 MiB; use the CLI for larger bundles"
            );
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("restore.zip");
            std::fs::write(&path, bytes)?;
            let manifest: BundleManifest = controller.inspect_bundle(&path)?;
            let id = uuid::Uuid::new_v4().to_string();
            *state
                .restore
                .lock()
                .map_err(|_| anyhow::anyhow!("Restore state unavailable"))? =
                Some(RestorePreview {
                    id: id.clone(),
                    _directory: dir,
                    path,
                    revision: controller.storage().revision()?,
                    vault: controller.db_path().into(),
                });
            Ok(
                json!({"preview_id":id,"manifest":manifest,"destination":controller.db_path(),"current_records":controller.stats()?.total_memories}),
            )
        }
        "/v1/workspace/restore" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Restore {
                preview_id: String,
                mode: String,
                confirm_vault: String,
            }
            let request: Restore = body(req)?;
            anyhow::ensure!(
                matches!(request.mode.as_str(), "merge" | "replace"),
                "Choose merge or replace"
            );
            let mut slot = state
                .restore
                .lock()
                .map_err(|_| anyhow::anyhow!("Restore state unavailable"))?;
            let preview = slot
                .as_ref()
                .context("Inspect a bundle before restoring it")?;
            anyhow::ensure!(
                preview.id == request.preview_id
                    && preview.vault == controller.db_path()
                    && preview.vault.to_string_lossy() == request.confirm_vault,
                "Restore target or preview changed; inspect the bundle again"
            );
            anyhow::ensure!(
                preview.revision == controller.storage().revision()?,
                "Vault changed since preview; inspect the bundle again"
            );
            let preview = slot.take().unwrap();
            let job = state.start_job("restore", move |_| {
                // Capture the directory guard as well as the path for the lifetime of the job.
                let _directory = preview._directory;
                let result = controller.import_bundle(
                    &preview.path,
                    request.mode == "replace",
                    request.mode == "merge",
                )?;
                Ok(format!(
                    "Imported {} memories. Check model/index readiness before searching.",
                    result.memories_imported
                ))
            })?;
            Ok(json!(job))
        }
        _ => anyhow::bail!("Unknown workspace endpoint"),
    }
}
