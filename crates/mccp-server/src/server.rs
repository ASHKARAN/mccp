use super::*;
use mccp_core::*;
use axum::{
    Router,
    Json,
    body::Bytes,
    extract::{
        Path,
        Query,
        Request,
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::HeaderMap,
    middleware::Next,
    response::IntoResponse,
    routing::{delete, get, patch, post, put},
};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use sysinfo::System;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_stream::StreamExt;
use tower_http::cors::CorsLayer;
use uuid::Uuid;
use sha2::Sha256;
use hmac::{Hmac, Mac};

/// Per-agent access control context (injected by auth middleware)
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub name: String,
    /// Project allowlist; `["*"]` means all projects
    pub projects: Vec<String>,
    pub can_write: bool,
}

impl AgentContext {
    pub fn anonymous() -> Self {
        Self { name: "anonymous".into(), projects: vec!["*".into()], can_write: true }
    }

    pub fn can_access(&self, project: &str) -> bool {
        self.projects.iter().any(|p| p == "*" || p == project)
    }
}

/// Application state shared across the server
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub query_engine: QueryEngine,
    /// Default project pipeline (current working directory)
    pub pipeline: Arc<mccp_indexer::IndexingPipeline>,
    pub code_intel: Arc<RwLock<Option<CodeIntelSnapshot>>>,

    pub started_at: chrono::DateTime<chrono::Utc>,
    pub http_addr: Arc<RwLock<Option<String>>>,

    pub ws_tx: broadcast::Sender<String>,
    pub logs: Arc<Mutex<VecDeque<LogLine>>>,
    pub sys: Arc<Mutex<System>>,

    pub projects: Arc<RwLock<HashMap<String, ProjectRuntime>>>,
    pub tasks: Arc<RwLock<HashMap<String, TaskInfo>>>,
    pub task_handles: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl AppState {
    /// Initialize application state
    pub async fn init(config: Config) -> Result<Self> {
        let started_at = Utc::now();
        let query_engine = QueryEngine::new(config.ranker_weights().clone());

        // Default project is current directory
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let default_id = ProjectId::from_path(&cwd).as_str().to_string();
        let default_name = cwd
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".to_string());
        let default_root = cwd.canonicalize().unwrap_or(cwd.clone());

        let default_pipeline = Arc::new(mccp_indexer::IndexingPipeline::new(
            Project::new(default_name.clone(), &default_root),
            config.indexer.clone(),
        ));

        // Try to load a persisted snapshot
        let existing = CodeIntelSnapshot::load(&default_id).ok().flatten();

        let (ws_tx, _) = broadcast::channel::<String>(1024);

        let state = Self {
            config: Arc::new(RwLock::new(config.clone())),
            query_engine,
            pipeline: default_pipeline.clone(),
            code_intel: Arc::new(RwLock::new(existing)),
            started_at,
            http_addr: Arc::new(RwLock::new(None)),
            ws_tx: ws_tx.clone(),
            logs: Arc::new(Mutex::new(VecDeque::with_capacity(2048))),
            sys: Arc::new(Mutex::new(System::new_all())),
            projects: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            task_handles: Arc::new(Mutex::new(HashMap::new())),
        };

        // Load persisted projects registry (best-effort)
        let mut projects = HashMap::new();
        projects.insert(
            default_id.clone(),
            ProjectRuntime {
                id: default_id.clone(),
                name: default_name,
                root_path: default_root.to_string_lossy().to_string(),
                watch: config.indexer.watch_enabled,
                status: "not_indexed".to_string(),
                last_indexed_at: None,
                pipeline: default_pipeline.clone(),
            },
        );

        if let Ok(extra) = load_projects_registry().await {
            for p in extra {
                if p.id == default_id {
                    continue;
                }
                let root = PathBuf::from(&p.root_path);
                let root = root.canonicalize().unwrap_or(root);
                let mut idx_cfg = config.indexer.clone();
                idx_cfg.watch_enabled = p.watch;

                let pipeline = Arc::new(mccp_indexer::IndexingPipeline::new(
                    Project::new(p.name.clone(), &root),
                    idx_cfg,
                ));

                projects.insert(
                    p.id.clone(),
                    ProjectRuntime {
                        id: p.id,
                        name: p.name,
                        root_path: root.to_string_lossy().to_string(),
                        watch: p.watch,
                        status: "not_indexed".to_string(),
                        last_indexed_at: None,
                        pipeline: pipeline.clone(),
                    },
                );

                spawn_progress_forwarder(pipeline.progress_rx.clone(), ws_tx.clone());
            }
        }

        *state.projects.write().await = projects;

        // Persist registry on first run (best-effort)
        let snapshot = state.projects.read().await.clone();
        let _ = save_projects_registry(&snapshot).await;

        // Forward default pipeline progress to WS
        spawn_progress_forwarder(state.pipeline.progress_rx.clone(), ws_tx);

        Ok(state)
    }
}

// ── Web Console (apis.md) types ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SystemStatus {
    pub version: String,
    pub started_at: String,
    pub uptime_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_addr: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemMetrics {
    pub cpu_percent: f32,
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_avg_1: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_indexed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub current: usize,
    pub total: usize,
    pub percentage: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub state: String,
    pub title: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<TaskProgress>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLine {
    pub id: String,
    pub ts: String,
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ConfigPutRequest {
    toml: String,
}

#[derive(Debug, Clone, Serialize)]
struct ConfigGetResponse {
    path: String,
    toml: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parsed: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogsResponse {
    items: Vec<LogLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedProject {
    id: String,
    name: String,
    root_path: String,
    #[serde(default)]
    watch: bool,
}

#[derive(Clone)]
pub struct ProjectRuntime {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub watch: bool,
    pub status: String,
    pub last_indexed_at: Option<String>,
    pub pipeline: Arc<mccp_indexer::IndexingPipeline>,
}

impl ProjectRuntime {
    pub fn to_info(&self) -> ProjectInfo {
        let st = self.pipeline.status();
        ProjectInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            root_path: self.root_path.clone(),
            status: self.status.clone(),
            last_indexed_at: self.last_indexed_at.clone(),
            file_count: Some(st.file_count),
            chunk_count: Some(0),
        }
    }

    pub fn to_persisted(&self) -> PersistedProject {
        PersistedProject {
            id: self.id.clone(),
            name: self.name.clone(),
            root_path: self.root_path.clone(),
            watch: self.watch,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct WsEnvelope<T: Serialize> {
    #[serde(rename = "type")]
    typ: String,
    ts: String,
    data: T,
}

/// Health check response
#[derive(Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    version: String,
}

async fn registry_path() -> Option<PathBuf> {
    mccp_core::Config::config_dir().ok().map(|d| d.join("projects.json"))
}

async fn load_projects_registry() -> anyhow::Result<Vec<PersistedProject>> {
    let Some(path) = registry_path().await else { return Ok(vec![]); };
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    if text.trim().is_empty() {
        return Ok(vec![]);
    }
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

async fn save_projects_registry(projects: &HashMap<String, ProjectRuntime>) -> anyhow::Result<()> {
    let Some(path) = registry_path().await else { return Ok(()); };
    let mut out: Vec<PersistedProject> = projects.values().map(|p| p.to_persisted()).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    let json = serde_json::to_string_pretty(&out)?;
    tokio::fs::write(&path, json).await?;
    Ok(())
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn spawn_progress_forwarder(
    mut rx: tokio::sync::watch::Receiver<Option<mccp_indexer::IndexProgress>>,
    ws_tx: broadcast::Sender<String>,
) {
    tokio::spawn(async move {
        loop {
            if rx.changed().await.is_err() {
                break;
            }
            let progress = rx.borrow_and_update().clone();
            if let Some(p) = progress {
                let env = WsEnvelope { typ: "index.progress".to_string(), ts: now_rfc3339(), data: p };
                if let Ok(text) = serde_json::to_string(&env) {
                    let _ = ws_tx.send(text);
                }
            }
        }
    });
}

async fn broadcast<T: Serialize>(state: &AppState, typ: &str, data: T) {
    let env = WsEnvelope { typ: typ.to_string(), ts: now_rfc3339(), data };
    if let Ok(text) = serde_json::to_string(&env) {
        let _ = state.ws_tx.send(text);
    }
}

async fn push_log(state: &AppState, level: &str, target: &str, message: impl ToString) {
    let line = LogLine {
        id: format!("log_{}", Uuid::new_v4()),
        ts: now_rfc3339(),
        level: level.to_string(),
        target: Some(target.to_string()),
        span: None,
        message: message.to_string(),
    };

    {
        let mut buf = state.logs.lock().await;
        buf.push_front(line.clone());
        while buf.len() > 2000 {
            buf.pop_back();
        }
    }

    broadcast(state, "logs.line", line).await;
}

/// Build the full HTTP + WS router (without binding).
/// Useful for embedding in custom servers or tests.
pub fn build_router(state: AppState) -> Router {
    // Spawn background cache warming
    tokio::spawn(warm_cache(state.clone()));

    Router::new()
        .route("/health", get(health))
        .route("/ws", get(ws_upgrade))

        // Web console admin APIs (apis.md)
        .route("/api/v1/system/status", get(api_system_status))
        .route("/api/v1/system/metrics", get(api_system_metrics))
        .route("/api/v1/config", get(api_get_config).put(api_put_config))
        .route("/api/v1/projects", get(api_list_projects).post(api_create_project))
        .route(
            "/api/v1/projects/:project_id",
            patch(api_patch_project).delete(api_delete_project),
        )
        .route(
            "/api/v1/projects/:project_id/reindex",
            post(api_reindex_project),
        )
        .route("/api/v1/tasks", get(api_list_tasks))
        .route("/api/v1/tasks/:task_id/cancel", post(api_cancel_task))
        .route("/api/v1/logs", get(api_list_logs))

        // Existing endpoints
        .route("/v1/query", post(query))
        .route("/v1/query/stream", post(query_stream_sse))
        .route("/v1/find_usages", post(find_usages))
        .route("/v1/get_flow", post(get_flow))
        .route("/v1/get_summary", post(get_summary))
        .route("/v1/get_related", post(get_related))
        .route("/v1/find_definition", post(find_definition))
        .route("/v1/get_symbol_map", post(get_symbol_map))
        .route("/v1/cross_project/query", post(cross_project_query))
        .route("/v1/write_file", post(write_file))
        .route("/index/progress", get(index_progress_sse))
        .route("/index/status", get(index_status))
        .route("/projects", get(list_projects))
        .route("/webhook/push", post(webhook_push))
        .route("/v1/code_intel/snapshot", get(code_intel_snapshot))
        .route("/v1/code_intel/symbol", get(code_intel_symbol))
        .route("/v1/code_intel/usages", get(code_intel_usages))
        .route("/v1/code_intel/callers", get(code_intel_callers))
        .route("/v1/code_intel/callees", get(code_intel_callees))
        .route("/v1/code_intel/cycles", get(code_intel_cycles))
        .route("/v1/code_intel/unused", get(code_intel_unused))
        .route("/v1/code_intel/refresh", post(code_intel_refresh))
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth_middleware))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Run HTTP server
pub async fn run_http(state: AppState, addr: &str) -> anyhow::Result<()> {
    *state.http_addr.write().await = Some(addr.to_string());

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("HTTP server listening on {}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

/// Run HTTP server with a graceful shutdown signal.
pub async fn run_http_with_shutdown(
    state: AppState,
    addr: &str,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    *state.http_addr.write().await = Some(addr.to_string());

    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("HTTP server listening on {}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async { let _ = shutdown.await; })
        .await?;
    Ok(())
}

/// Run MCP server over stdio
pub async fn run_stdio(_state: AppState) -> anyhow::Result<()> {
    // TODO: Implement MCP server over stdio
    tracing::info!("MCP server started on stdio");
    Ok(())
}

/// Health check endpoint
async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

// ── Web Console (apis.md) REST + WS ─────────────────────────────────────────

async fn api_system_status(State(state): State<AppState>) -> impl IntoResponse {
    let http_addr = state.http_addr.read().await.clone();
    let started_at = state.started_at;
    let uptime_ms = (Utc::now() - started_at)
        .num_milliseconds()
        .max(0) as u64;

    Json(SystemStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        started_at: started_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        uptime_ms,
        pid: Some(std::process::id()),
        http_addr,
    })
}

async fn api_system_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let mut sys = state.sys.lock().await;
    sys.refresh_cpu();
    sys.refresh_memory();

    let cpu_percent = if sys.cpus().is_empty() {
        0.0
    } else {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / (sys.cpus().len() as f32)
    };

    let load_avg_1 = Some(System::load_average().one);

    Json(SystemMetrics {
        cpu_percent,
        ram_used_bytes: sys.used_memory(),
        ram_total_bytes: sys.total_memory(),
        load_avg_1,
    })
}

async fn api_get_config(State(state): State<AppState>) -> axum::response::Response {
    let path = match mccp_core::Config::default_config_path() {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let toml = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(_) => {
            let cfg = state.config.read().await;
            toml::to_string_pretty(&*cfg).unwrap_or_default()
        }
    };

    let parsed = {
        let cfg = state.config.read().await;
        serde_json::to_value(&*cfg).ok()
    };

    Json(ConfigGetResponse {
        path: path.to_string_lossy().to_string(),
        toml,
        parsed,
    })
    .into_response()
}

async fn api_put_config(
    State(state): State<AppState>,
    Json(req): Json<ConfigPutRequest>,
) -> axum::response::Response {
    let path = match mccp_core::Config::default_config_path() {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let new_cfg: Config = match toml::from_str(&req.toml) {
        Ok(c) => c,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid toml: {e}")).into_response(),
    };

    if let Err(e) = mccp_core::ConfigValidator::validate(&new_cfg) {
        return (StatusCode::BAD_REQUEST, format!("invalid config: {e}"))
            .into_response();
    }

    let restart_required = {
        let old = state.config.read().await;
        old.daemon.http_port != new_cfg.daemon.http_port
    };

    if let Err(e) = tokio::fs::write(&path, &req.toml).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("write failed: {e}"))
            .into_response();
    }

    *state.config.write().await = new_cfg;
    push_log(&state, "INFO", "mccp_server", "config updated via API").await;

    Json(serde_json::json!({ "ok": true, "restart_required": restart_required }))
        .into_response()
}

#[derive(Debug, Deserialize)]
struct CreateProjectRequest {
    name: String,
    root_path: String,
    #[serde(default)]
    watch: Option<bool>,
    #[serde(default)]
    index_immediately: Option<bool>,
}

async fn api_list_projects(State(state): State<AppState>) -> impl IntoResponse {
    let projects = state.projects.read().await;
    let mut out: Vec<ProjectInfo> = projects.values().map(|p| p.to_info()).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Json(out)
}

async fn broadcast_projects_snapshot(state: &AppState) {
    let projects = state.projects.read().await;
    let mut out: Vec<ProjectInfo> = projects.values().map(|p| p.to_info()).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    broadcast(state, "projects.snapshot", out).await;
}

async fn broadcast_project_updated(state: &AppState, project_id: &str) {
    let projects = state.projects.read().await;
    if let Some(p) = projects.get(project_id) {
        broadcast(state, "projects.updated", p.to_info()).await;
    }
}

async fn api_create_project(
    State(state): State<AppState>,
    Json(req): Json<CreateProjectRequest>,
) -> impl IntoResponse {
    let root = PathBuf::from(&req.root_path);
    let root = root.canonicalize().unwrap_or(root);
    let id = ProjectId::from_path(&root).as_str().to_string();
    let watch = req.watch.unwrap_or(true);

    {
        let projects = state.projects.read().await;
        if projects.contains_key(&id) {
            return Json(serde_json::json!({ "id": id }));
        }
    }

    let idx_cfg = {
        let cfg = state.config.read().await;
        let mut idx = cfg.indexer.clone();
        idx.watch_enabled = watch;
        idx
    };

    let pipeline = Arc::new(mccp_indexer::IndexingPipeline::new(
        Project::new(req.name.clone(), &root),
        idx_cfg,
    ));

    spawn_progress_forwarder(pipeline.progress_rx.clone(), state.ws_tx.clone());

    {
        let mut projects = state.projects.write().await;
        projects.insert(
            id.clone(),
            ProjectRuntime {
                id: id.clone(),
                name: req.name,
                root_path: root.to_string_lossy().to_string(),
                watch,
                status: "not_indexed".to_string(),
                last_indexed_at: None,
                pipeline,
            },
        );
        let _ = save_projects_registry(&projects).await;
    }

    push_log(&state, "INFO", "mccp_server", format!("project created: {id}")).await;
    broadcast_projects_snapshot(&state).await;
    broadcast_project_updated(&state, &id).await;

    if req.index_immediately.unwrap_or(false) {
        let _ = spawn_reindex_task(state.clone(), id.clone()).await;
    }

    Json(serde_json::json!({ "id": id }))
}

#[derive(Debug, Deserialize)]
struct PatchProjectRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    watch: Option<bool>,
}

async fn api_patch_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(req): Json<PatchProjectRequest>,
) -> axum::response::Response {
    let mut projects = state.projects.write().await;
    let Some(p) = projects.get_mut(&project_id) else {
        return (StatusCode::NOT_FOUND, "project not found").into_response();
    };

    if let Some(name) = req.name {
        p.name = name;
    }
    if let Some(w) = req.watch {
        p.watch = w;
    }

    let _ = save_projects_registry(&projects).await;
    drop(projects);

    push_log(&state, "INFO", "mccp_server", format!("project updated: {project_id}")).await;
    broadcast_projects_snapshot(&state).await;
    broadcast_project_updated(&state, &project_id).await;

    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn api_delete_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> axum::response::Response {
    // Don't allow deleting the default (cwd) project.
    let default_id = ProjectId::from_path(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .as_str()
        .to_string();
    if project_id == default_id {
        return (StatusCode::BAD_REQUEST, "cannot delete default project").into_response();
    }

    let mut projects = state.projects.write().await;
    if projects.remove(&project_id).is_none() {
        return (StatusCode::NOT_FOUND, "project not found").into_response();
    }
    let _ = save_projects_registry(&projects).await;
    drop(projects);

    push_log(&state, "INFO", "mccp_server", format!("project deleted: {project_id}")).await;
    broadcast_projects_snapshot(&state).await;

    Json(serde_json::json!({ "ok": true })).into_response()
}

async fn spawn_reindex_task(state: AppState, project_id: String) -> anyhow::Result<String> {
    let task_id = format!("task_{}", Uuid::new_v4());
    let created_at = now_rfc3339();

    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(
            task_id.clone(),
            TaskInfo {
                id: task_id.clone(),
                kind: "reindex".to_string(),
                project_id: Some(project_id.clone()),
                state: "running".to_string(),
                title: format!("Reindex {project_id}"),
                created_at: created_at.clone(),
                started_at: Some(created_at.clone()),
                finished_at: None,
                progress: Some(TaskProgress { current: 0, total: 0, percentage: 0, phase: Some("chunking".to_string()) }),
                error: None,
            },
        );
    }

    broadcast(&state, "tasks.updated", state.tasks.read().await.get(&task_id).cloned().unwrap()).await;

    {
        let mut projects = state.projects.write().await;
        if let Some(p) = projects.get_mut(&project_id) {
            p.status = "indexing".to_string();
        }
    }
    broadcast_projects_snapshot(&state).await;
    broadcast_project_updated(&state, &project_id).await;

    let task_id_spawn = task_id.clone();
    let task_id_handle = task_id.clone();
    let handles_ref = state.task_handles.clone();
    let handle = tokio::spawn(async move {
        let task_id = task_id_spawn;
        let pipeline = {
            let projects = state.projects.read().await;
            projects.get(&project_id).map(|p| p.pipeline.clone())
        };

        let Some(pipeline) = pipeline else {
            return;
        };

        // Track progress -> tasks.updated
        let mut prx = pipeline.progress_rx.clone();
        let progress_task_id = task_id.clone();
        let progress_state = state.clone();
        tokio::spawn(async move {
            loop {
                if prx.changed().await.is_err() {
                    break;
                }
                let p = prx.borrow_and_update().clone();
                if let Some(p) = p {
                    let mut tasks = progress_state.tasks.write().await;
                    if let Some(t) = tasks.get_mut(&progress_task_id) {
                        t.progress = Some(TaskProgress {
                            current: p.current,
                            total: p.total,
                            percentage: p.percentage,
                            phase: Some(p.phase.clone()),
                        });
                        let updated = t.clone();
                        drop(tasks);
                        broadcast(&progress_state, "tasks.updated", updated).await;
                    }
                }
            }
        });

        let result = pipeline.force_reindex().await;
        let finished_at = now_rfc3339();

        // Remove our handle from task_handles
        state.task_handles.lock().await.remove(&task_id);

        match result {
            Ok(_) => {
                {
                    let mut projects = state.projects.write().await;
                    if let Some(p) = projects.get_mut(&project_id) {
                        p.status = "indexed".to_string();
                        p.last_indexed_at = Some(finished_at.clone());
                    }
                }
                broadcast_projects_snapshot(&state).await;
                broadcast_project_updated(&state, &project_id).await;

                let mut tasks = state.tasks.write().await;
                if let Some(t) = tasks.get_mut(&task_id) {
                    t.state = "finished".to_string();
                    t.finished_at = Some(finished_at);
                    let updated = t.clone();
                    drop(tasks);
                    broadcast(&state, "tasks.updated", updated).await;
                }

                push_log(&state, "INFO", "mccp_server", format!("reindex finished: {project_id}"))
                    .await;
            }
            Err(e) => {
                {
                    let mut projects = state.projects.write().await;
                    if let Some(p) = projects.get_mut(&project_id) {
                        p.status = "error".to_string();
                    }
                }
                broadcast_projects_snapshot(&state).await;
                broadcast_project_updated(&state, &project_id).await;

                let mut tasks = state.tasks.write().await;
                if let Some(t) = tasks.get_mut(&task_id) {
                    t.state = "failed".to_string();
                    t.finished_at = Some(finished_at);
                    t.error = Some(e.to_string());
                    let updated = t.clone();
                    drop(tasks);
                    broadcast(&state, "tasks.updated", updated).await;
                }

                push_log(&state, "ERROR", "mccp_server", format!("reindex failed: {project_id}: {e}"))
                    .await;
            }
        }
    });

    handles_ref.lock().await.insert(task_id_handle, handle);

    Ok(task_id)
}

async fn api_reindex_project(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
) -> axum::response::Response {
    {
        let projects = state.projects.read().await;
        if !projects.contains_key(&project_id) {
            return (StatusCode::NOT_FOUND, "project not found").into_response();
        }
    }

    match spawn_reindex_task(state.clone(), project_id).await {
        Ok(task_id) => Json(serde_json::json!({ "task_id": task_id })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct ListTasksQuery {
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

async fn api_list_tasks(
    State(state): State<AppState>,
    Query(q): Query<ListTasksQuery>,
) -> impl IntoResponse {
    let tasks = state.tasks.read().await;
    let mut out: Vec<TaskInfo> = tasks
        .values()
        .filter(|t| {
            if let Some(ref pid) = q.project_id {
                if t.project_id.as_deref() != Some(pid.as_str()) {
                    return false;
                }
            }
            match q.state.as_deref().unwrap_or("all") {
                "active" => t.state == "queued" || t.state == "running",
                "finished" => {
                    t.state == "finished" || t.state == "failed" || t.state == "canceled"
                }
                _ => true,
            }
        })
        .cloned()
        .collect();

    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(out)
}

async fn api_cancel_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> axum::response::Response {
    let mut tasks = state.tasks.write().await;
    let Some(t) = tasks.get_mut(&task_id) else {
        return (StatusCode::NOT_FOUND, "task not found").into_response();
    };

    if t.state == "finished" || t.state == "failed" || t.state == "canceled" {
        return Json(serde_json::json!({ "ok": true })).into_response();
    }

    t.state = "canceled".to_string();
    t.finished_at = Some(now_rfc3339());
    let project_id = t.project_id.clone();
    let updated = t.clone();
    drop(tasks);

    // Abort the spawned task if it's still running
    if let Some(handle) = state.task_handles.lock().await.remove(&task_id) {
        handle.abort();
    }

    // Reset project status back from "indexing" if applicable
    if let Some(pid) = project_id {
        let mut projects = state.projects.write().await;
        if let Some(p) = projects.get_mut(&pid) {
            if p.status == "indexing" {
                p.status = "not_indexed".to_string();
            }
        }
        drop(projects);
        broadcast_projects_snapshot(&state).await;
        broadcast_project_updated(&state, &pid).await;
    }

    broadcast(&state, "tasks.updated", updated).await;
    Json(serde_json::json!({ "ok": true })).into_response()
}

#[derive(Debug, Deserialize)]
struct LogsQuery {
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    cursor: Option<String>,
}

async fn api_list_logs(
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(500).min(5000);

    let since = q.since.as_deref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
    let until = q.until.as_deref().and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());

    let mut out = Vec::new();
    let buf = state.logs.lock().await;

    // Cursor-based pagination: skip entries until we pass the cursor id.
    let mut past_cursor = q.cursor.is_none();

    for line in buf.iter() {
        if !past_cursor {
            if Some(&line.id) == q.cursor.as_ref() {
                past_cursor = true;
            }
            continue;
        }

        if let Some(ref lvl) = q.level {
            if &line.level != lvl {
                continue;
            }
        }
        if let Some(ref t) = q.target {
            if let Some(ref target) = line.target {
                if !target.starts_with(t) {
                    continue;
                }
            } else {
                continue;
            }
        }
        if let Some(ref needle) = q.q {
            let blob = format!("{} {} {}", line.message, line.target.clone().unwrap_or_default(), line.span.clone().unwrap_or_default());
            if !blob.to_lowercase().contains(&needle.to_lowercase()) {
                continue;
            }
        }

        if since.is_some() || until.is_some() {
            if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&line.ts) {
                if let Some(ref s) = since {
                    if ts < *s {
                        continue;
                    }
                }
                if let Some(ref u) = until {
                    if ts > *u {
                        continue;
                    }
                }
            }
        }

        out.push(line.clone());
        if out.len() >= limit {
            break;
        }
    }

    let next_cursor = if out.len() >= limit {
        out.last().map(|l| l.id.clone())
    } else {
        None
    };

    Json(LogsResponse { items: out, next_cursor })
}

async fn ws_upgrade(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_handle(socket, state))
}

async fn ws_send<T: Serialize>(socket: &mut WebSocket, typ: &str, data: T) {
    let env = WsEnvelope { typ: typ.to_string(), ts: now_rfc3339(), data };
    if let Ok(text) = serde_json::to_string(&env) {
        let _ = socket.send(Message::Text(text)).await;
    }
}

async fn ws_handle(mut socket: WebSocket, state: AppState) {
    // Initial snapshots
    let _ = ws_send(
        &mut socket,
        "system.status",
        SystemStatus {
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: state.started_at.to_rfc3339_opts(SecondsFormat::Secs, true),
            uptime_ms: (Utc::now() - state.started_at).num_milliseconds().max(0) as u64,
            pid: Some(std::process::id()),
            http_addr: state.http_addr.read().await.clone(),
        },
    )
    .await;

    {
        let mut sys = state.sys.lock().await;
        sys.refresh_cpu();
        sys.refresh_memory();
        let cpu_percent = if sys.cpus().is_empty() {
            0.0
        } else {
            sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / (sys.cpus().len() as f32)
        };
        let m = SystemMetrics {
            cpu_percent,
            ram_used_bytes: sys.used_memory(),
            ram_total_bytes: sys.total_memory(),
            load_avg_1: Some(System::load_average().one),
        };
        ws_send(&mut socket, "system.metrics", m).await;
    }

    let tasks: Vec<TaskInfo> = state.tasks.read().await.values().cloned().collect();
    ws_send(&mut socket, "tasks.snapshot", tasks).await;

    let projects: Vec<ProjectInfo> = state.projects.read().await.values().map(|p| p.to_info()).collect();
    ws_send(&mut socket, "projects.snapshot", projects).await;

    let logs: Vec<LogLine> = state.logs.lock().await.iter().cloned().collect();
    ws_send(&mut socket, "logs.snapshot", logs).await;

    let mut rx = state.ws_tx.subscribe();
    let mut status_tick = tokio::time::interval(Duration::from_secs(5));
    let mut metrics_tick = tokio::time::interval(Duration::from_secs(2));

    loop {
        tokio::select! {
            _ = status_tick.tick() => {
                let env = SystemStatus {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    started_at: state.started_at.to_rfc3339_opts(SecondsFormat::Secs, true),
                    uptime_ms: (Utc::now() - state.started_at).num_milliseconds().max(0) as u64,
                    pid: Some(std::process::id()),
                    http_addr: state.http_addr.read().await.clone(),
                };
                ws_send(&mut socket, "system.status", env).await;
            }
            _ = metrics_tick.tick() => {
                // Recompute metrics per-connection.
                let mut sys = state.sys.lock().await;
                sys.refresh_cpu();
                sys.refresh_memory();
                let cpu_percent = if sys.cpus().is_empty() { 0.0 } else { sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / (sys.cpus().len() as f32) };
                let m = SystemMetrics {
                    cpu_percent,
                    ram_used_bytes: sys.used_memory(),
                    ram_total_bytes: sys.total_memory(),
                    load_avg_1: Some(System::load_average().one),
                };
                ws_send(&mut socket, "system.metrics", m).await;
            }
            msg = socket.recv() => {
                let Some(Ok(msg)) = msg else { break; };
                match msg {
                    Message::Text(t) => {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                            match v.get("type").and_then(|x| x.as_str()) {
                                Some("ping") => {
                                    ws_send(&mut socket, "pong", serde_json::json!({})).await;
                                }
                                Some("subscribe") => {
                                    let tasks: Vec<TaskInfo> = state.tasks.read().await.values().cloned().collect();
                                    ws_send(&mut socket, "tasks.snapshot", tasks).await;
                                    let projects: Vec<ProjectInfo> = state.projects.read().await.values().map(|p| p.to_info()).collect();
                                    ws_send(&mut socket, "projects.snapshot", projects).await;
                                }
                                _ => {}
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            evt = rx.recv() => {
                match evt {
                    Ok(text) => {
                        if socket.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        }
    }
}

// ── Existing server handlers ────────────────────────────────────────────────

/// Query endpoint
async fn query(
    State(state): State<AppState>,
    Json(req): Json<HttpQueryRequest>,
) -> impl IntoResponse {
    let engine_req = mccp_core::QueryRequest::new(req.project, req.query, req.top_k);
    match state.query_engine.query(engine_req).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Find usages endpoint
async fn find_usages(
    State(state): State<AppState>,
    Json(req): Json<HttpFindUsagesRequest>,
) -> impl IntoResponse {
    match state.query_engine.find_usages(
        &req.project, &req.symbol, req.symbol_kind, req.ref_kind, req.file_pattern.as_deref()
    ).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Get flow endpoint
async fn get_flow(
    State(state): State<AppState>,
    Json(req): Json<HttpGetFlowRequest>,
) -> impl IntoResponse {
    match state.query_engine.get_flow(&req.project, &req.entry, req.max_depth.unwrap_or(5)).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Get summary endpoint
async fn get_summary(
    State(state): State<AppState>,
    Json(req): Json<HttpGetSummaryRequest>,
) -> impl IntoResponse {
    match state.query_engine.get_summary(&req.project, &req.path, req.scope.as_deref()).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Get related endpoint
async fn get_related(
    State(state): State<AppState>,
    Json(req): Json<HttpGetRelatedRequest>,
) -> impl IntoResponse {
    match state.query_engine.get_related(&req.project, &req.path, req.depth.unwrap_or(2)).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Find definition endpoint
async fn find_definition(
    State(state): State<AppState>,
    Json(req): Json<HttpFindDefinitionRequest>,
) -> impl IntoResponse {
    match state.query_engine.find_definition(&req.project, &req.symbol, req.scope_hint.as_deref()).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Get symbol map endpoint
async fn get_symbol_map(
    State(state): State<AppState>,
    Json(req): Json<HttpGetSymbolMapRequest>,
) -> impl IntoResponse {
    match state.query_engine.get_symbol_map(&req.project, &req.path).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Index progress SSE endpoint
async fn index_progress_sse(
    State(state): State<AppState>,
) -> Sse<impl StreamExt<Item = std::result::Result<Event, Infallible>>> {
    let mut rx = state.pipeline.progress_rx.clone();
    let stream = async_stream::stream! {
        loop {
            rx.changed().await.ok();
            let progress = rx.borrow_and_update().clone();
            if let Some(p) = progress {
                let data = serde_json::to_string(&p).unwrap_or_default();
                yield Ok(Event::default().data(data));
                if p.percentage >= 100 { break; }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Index status endpoint
async fn index_status(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let status = state.pipeline.status();
    Json(status)
}

/// List projects endpoint
async fn list_projects(
    _state: State<AppState>,
) -> impl IntoResponse {
    Json(vec![serde_json::Value::Null; 0])
}

/// V3-3: Streaming query results SSE endpoint
async fn query_stream_sse(
    State(state): State<AppState>,
    Json(req): Json<HttpQueryRequest>,
) -> Sse<impl futures::Stream<Item = std::result::Result<Event, Infallible>>> {
    let engine_req = mccp_core::QueryRequest::new(req.project, req.query, req.top_k);
    let stream = async_stream::stream! {
        match state.query_engine.query(engine_req).await {
            Ok(results) => {
                for r in results {
                    let data = serde_json::to_string(&r).unwrap_or_default();
                    yield Ok(Event::default().event("chunk").data(data));
                }
                yield Ok(Event::default().event("done").data("{}"));
            }
            Err(e) => {
                let data = format!("{{\"error\":\"{}\"}}", e);
                yield Ok(Event::default().event("error").data(data));
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// V3-5: Cross-project query endpoint
async fn cross_project_query(
    State(state): State<AppState>,
    Json(req): Json<HttpCrossProjectQueryRequest>,
) -> impl IntoResponse {
    // Gather results from all allowed projects
    let allowed = req.allowed_projects.as_deref().unwrap_or(&[]);
    let mut all_results: Vec<mccp_core::QueryResult> = Vec::new();

    let primary_req = mccp_core::QueryRequest::new(req.project.clone(), req.query.clone(), req.top_k);
    if let Ok(mut results) = state.query_engine.query(primary_req).await {
        all_results.append(&mut results);
    }

    for project in allowed {
        if project == &req.project { continue; }
        let sub_req = mccp_core::QueryRequest::new(project.clone(), req.query.clone(), req.top_k);
        if let Ok(mut results) = state.query_engine.query(sub_req).await {
            all_results.append(&mut results);
        }
    }

    // Re-rank and truncate
    all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    all_results.truncate(req.top_k);

    (StatusCode::OK, Json(all_results)).into_response()
}

/// V3-4: Write file endpoint
async fn write_file(
    State(state): State<AppState>,
    Json(req): Json<HttpWriteFileRequest>,
) -> impl IntoResponse {
    let project_root = std::path::PathBuf::from(".");
    let abs_path = project_root.join(&req.path);

    // Security: reject path traversal
    if !abs_path.starts_with(&project_root) {
        return (StatusCode::BAD_REQUEST, "path escapes project root").into_response();
    }

    if let Some(parent) = abs_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("mkdir failed: {}", e)).into_response();
        }
    }

    match tokio::fs::write(&abs_path, &req.content).await {
        Ok(_) => {
            // Trigger file re-index (fire-and-forget)
            tracing::info!("write_file: wrote {} ({} bytes)", req.path, req.content.len());
            (StatusCode::OK, Json(serde_json::json!({ "path": req.path, "bytes": req.content.len() }))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("write failed: {}", e)).into_response(),
    }
}

/// V3-6: Webhook push handler
async fn webhook_push(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Verify HMAC-SHA256 if secret is configured
    let cfg = state.config.read().await;
    if let Some(secret) = cfg.webhook.as_ref().and_then(|w| w.secret.as_deref()) {
        if !secret.is_empty() {
            let sig_header = headers
                .get("X-Hub-Signature-256")
                .or_else(|| headers.get("X-Gitlab-Token"))
                .and_then(|v| v.to_str().ok());

            if let Some(sig) = sig_header {
                type HmacSha256 = Hmac<Sha256>;
                if let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) {
                    mac.update(&body);
                    let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
                    if sig != expected {
                        return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
                    }
                }
            } else {
                return (StatusCode::UNAUTHORIZED, "missing signature header").into_response();
            }
        }
    }

    // Extract changed files from GitHub push event or X-Changed-Files header
    let changed_files: Vec<String> = if let Some(files_hdr) = headers.get("X-Changed-Files") {
        files_hdr.to_str().unwrap_or("").split(',').map(|s| s.trim().to_string()).collect()
    } else if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) {
        payload["commits"].as_array().map(|commits| {
            commits.iter().flat_map(|c| {
                let added = c["added"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default();
                let modified = c["modified"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>()).unwrap_or_default();
                added.into_iter().chain(modified)
            }).collect()
        }).unwrap_or_default()
    } else {
        vec![]
    };

    if !changed_files.is_empty() {
        tracing::info!("webhook_push: {} changed files, triggering re-index", changed_files.len());
    }

    StatusCode::OK.into_response()
}

/// V3-7: Auth middleware — validates Bearer token; passes AgentContext in extensions
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> axum::response::Response {
    let cfg = state.config.read().await;

    // If no agents configured → single-user mode, allow all
    if cfg.agents.is_empty() {
        req.extensions_mut().insert(AgentContext::anonymous());
        return next.run(req).await;
    }

    let token = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(String::from);

    match token.as_deref().and_then(|t| cfg.find_agent(t)) {
        Some(agent_cfg) => {
            let ctx = AgentContext {
                name: agent_cfg.name.clone(),
                projects: agent_cfg.projects.clone(),
                can_write: agent_cfg.can_write,
            };
            req.extensions_mut().insert(ctx);
            next.run(req).await
        }
        None => (StatusCode::UNAUTHORIZED, "invalid agent token").into_response(),
    }
}

/// V3-8: Warm query cache on startup (background task)
async fn warm_cache(state: AppState) {
    // Load recent queries from storage and re-execute them to warm the LRU cache
    let recent = mccp_storage::load_recent_queries("default", 20).await.unwrap_or_default();
    for q in recent {
        let req = mccp_core::QueryRequest::new(q.project, q.query, 10);
        let _ = state.query_engine.query(req).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    tracing::debug!("cache warming complete");
}

/// Request types for REST API
#[derive(Deserialize)]
pub struct HttpQueryRequest {
    pub project: String,
    pub query: String,
    pub top_k: usize,
    pub filters: Option<Filters>,
}

#[derive(Deserialize)]
pub struct HttpFindUsagesRequest {
    pub project: String,
    pub symbol: String,
    pub symbol_kind: Option<SymbolKind>,
    pub ref_kind: Option<Vec<RefKind>>,
    pub file_pattern: Option<String>,
}

#[derive(Deserialize)]
pub struct HttpGetFlowRequest {
    pub project: String,
    pub entry: String,
    pub max_depth: Option<usize>,
}

#[derive(Deserialize)]
pub struct HttpGetSummaryRequest {
    pub project: String,
    pub path: String,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
pub struct HttpGetRelatedRequest {
    pub project: String,
    pub path: String,
    pub depth: Option<usize>,
}

#[derive(Deserialize)]
pub struct HttpFindDefinitionRequest {
    pub project: String,
    pub symbol: String,
    pub scope_hint: Option<String>,
}

#[derive(Deserialize)]
pub struct HttpGetSymbolMapRequest {
    pub project: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct HttpCrossProjectQueryRequest {
    pub project: String,
    pub query: String,
    pub top_k: usize,
    pub allowed_projects: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct HttpWriteFileRequest {
    pub project: String,
    pub path: String,
    pub content: String,
}

/// Response envelope for REST API
#[derive(Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub err: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Json<Self> {
        Json(Self { ok: true, data: Some(data), err: None })
    }
    
    pub fn err(msg: impl ToString) -> Json<Self> {
        Json(Self { ok: false, data: None, err: Some(msg.to_string()) })
    }
}

// ── V4-3 Code Intel query params ─────────────────────────────────────

#[derive(Deserialize)]
pub struct SymbolQuery {
    pub name: String,
}

#[derive(Deserialize)]
pub struct SymbolIdQuery {
    pub symbol: String,
}

// ── V4-3 Code Intel handlers ─────────────────────────────────────────

async fn code_intel_snapshot(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let guard = state.code_intel.read().await;
    match guard.as_ref() {
        Some(snap) => ApiResponse::ok(snap.clone()),
        None => ApiResponse::err("no snapshot available; POST /v1/code_intel/refresh first"),
    }
}

async fn code_intel_symbol(
    State(state): State<AppState>,
    Query(params): Query<SymbolQuery>,
) -> impl IntoResponse {
    let guard = state.code_intel.read().await;
    match guard.as_ref() {
        Some(snap) => {
            let symbols: Vec<_> = snap.find_symbols(&params.name).into_iter().cloned().collect();
            if symbols.is_empty() {
                ApiResponse::err(format!("symbol '{}' not found", params.name))
            } else {
                ApiResponse::ok(symbols)
            }
        }
        None => ApiResponse::err("no snapshot available"),
    }
}

async fn code_intel_usages(
    State(state): State<AppState>,
    Query(params): Query<SymbolIdQuery>,
) -> impl IntoResponse {
    let guard = state.code_intel.read().await;
    match guard.as_ref() {
        Some(snap) => {
            let usages: Vec<_> = snap.usages_of(&params.symbol).into_iter().cloned().collect();
            ApiResponse::ok(usages)
        }
        None => ApiResponse::err("no snapshot available"),
    }
}

async fn code_intel_callers(
    State(state): State<AppState>,
    Query(params): Query<SymbolIdQuery>,
) -> impl IntoResponse {
    let guard = state.code_intel.read().await;
    match guard.as_ref() {
        Some(snap) => {
            let callers: Vec<String> = snap.callers_of(&params.symbol).into_iter().map(String::from).collect();
            ApiResponse::ok(callers)
        }
        None => ApiResponse::err("no snapshot available"),
    }
}

async fn code_intel_callees(
    State(state): State<AppState>,
    Query(params): Query<SymbolIdQuery>,
) -> impl IntoResponse {
    let guard = state.code_intel.read().await;
    match guard.as_ref() {
        Some(snap) => {
            let callees: Vec<String> = snap.callees_of(&params.symbol).into_iter().map(String::from).collect();
            ApiResponse::ok(callees)
        }
        None => ApiResponse::err("no snapshot available"),
    }
}

async fn code_intel_cycles(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let guard = state.code_intel.read().await;
    match guard.as_ref() {
        Some(snap) => {
            let report = mccp_indexer::CycleDetector::detect_all(snap);
            ApiResponse::ok(report)
        }
        None => ApiResponse::err("no snapshot available"),
    }
}

async fn code_intel_unused(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let guard = state.code_intel.read().await;
    match guard.as_ref() {
        Some(snap) => {
            let unused: Vec<_> = snap.unused_symbols().into_iter().cloned().collect();
            ApiResponse::ok(unused)
        }
        None => ApiResponse::err("no snapshot available"),
    }
}

async fn code_intel_refresh(
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Use current directory as project root (matches AppState::init default)
    let project_root = std::path::PathBuf::from(".");
    let analyzer = mccp_indexer::TreeSitterAnalyzer::new();
    match analyzer.analyze(&project_root).await {
        Ok(mut snap) => {
            // Run cycle detection and annotate symbols
            let report = mccp_indexer::CycleDetector::detect_all(&snap);
            let cycled: std::collections::HashSet<String> = report.call_cycles.iter()
                .chain(report.import_cycles.iter())
                .flatten()
                .cloned()
                .collect();
            for sym in &mut snap.symbols {
                sym.in_cycle = cycled.contains(&sym.id);
            }
            // Log cycle warnings
            if !report.call_cycles.is_empty() || !report.import_cycles.is_empty() {
                tracing::warn!(
                    "detected {} call cycles and {} import cycles",
                    report.call_cycles.len(),
                    report.import_cycles.len()
                );
                for cycle in &report.call_cycles {
                    tracing::warn!("call cycle: {}", cycle.join(" → "));
                }
                for cycle in &report.import_cycles {
                    tracing::warn!("import cycle: {}", cycle.join(" → "));
                }
            }
            // Persist to disk
            if let Err(e) = snap.save() {
                tracing::warn!("failed to persist code_intel snapshot: {e}");
            }
            let mut guard = state.code_intel.write().await;
            *guard = Some(snap);
            ApiResponse::ok("refresh complete")
        }
        Err(e) => ApiResponse::err(format!("analysis failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;

    #[tokio::test]
    async fn test_health_endpoint() {
        let config = Config::default();
        let state = AppState::init(config).await.unwrap();
        let app = Router::new()
            .route("/health", get(health))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server.get("/health").await;
        
        assert_eq!(response.status_code(), 200);
        let body: HealthResponse = response.json();
        assert_eq!(body.status, "ok");
    }

    // ── V4-3 Code Intel API tests ────────────────────────────────────

    fn code_intel_app(state: AppState) -> Router {
        Router::new()
            .route("/v1/code_intel/snapshot", get(code_intel_snapshot))
            .route("/v1/code_intel/symbol", get(code_intel_symbol))
            .route("/v1/code_intel/usages", get(code_intel_usages))
            .route("/v1/code_intel/callers", get(code_intel_callers))
            .route("/v1/code_intel/callees", get(code_intel_callees))
            .route("/v1/code_intel/cycles", get(code_intel_cycles))
            .route("/v1/code_intel/unused", get(code_intel_unused))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_code_intel_no_snapshot() {
        let config = Config::default();
        let state = AppState::init(config).await.unwrap();
        let app = code_intel_app(state);
        let server = TestServer::new(app).unwrap();

        let resp = server.get("/v1/code_intel/snapshot").await;
        assert_eq!(resp.status_code(), 200);
        let body: ApiResponse<serde_json::Value> = resp.json();
        assert!(!body.ok);
        assert!(body.err.unwrap().contains("no snapshot"));
    }

    #[tokio::test]
    async fn test_code_intel_symbol_lookup() {
        let config = Config::default();
        let state = AppState::init(config).await.unwrap();

        // Pre-populate a snapshot
        {
            let mut snap = CodeIntelSnapshot::new("test".into());
            snap.symbols.push(SymbolDef::new(
                "Foo".into(), SymbolKind::Struct, "src/lib.rs".into(), 1, 10,
            ));
            snap.symbols.push(SymbolDef::new(
                "bar".into(), SymbolKind::Function, "src/lib.rs".into(), 12, 20,
            ));
            *state.code_intel.write().await = Some(snap);
        }

        let app = code_intel_app(state);
        let server = TestServer::new(app).unwrap();

        // Found
        let resp = server.get("/v1/code_intel/symbol").add_query_param("name", "Foo").await;
        let body: ApiResponse<Vec<SymbolDef>> = resp.json();
        assert!(body.ok);
        assert_eq!(body.data.unwrap().len(), 1);

        // Not found
        let resp = server.get("/v1/code_intel/symbol").add_query_param("name", "Baz").await;
        let body: ApiResponse<Vec<SymbolDef>> = resp.json();
        assert!(!body.ok);
    }

    #[tokio::test]
    async fn test_code_intel_callers_callees() {
        let config = Config::default();
        let state = AppState::init(config).await.unwrap();

        {
            let mut snap = CodeIntelSnapshot::new("test".into());
            snap.call_edges.push(CallEdge { caller: "a".into(), callee: "b".into() });
            snap.call_edges.push(CallEdge { caller: "c".into(), callee: "b".into() });
            *state.code_intel.write().await = Some(snap);
        }

        let app = code_intel_app(state);
        let server = TestServer::new(app).unwrap();

        let resp = server.get("/v1/code_intel/callers").add_query_param("symbol", "b").await;
        let body: ApiResponse<Vec<String>> = resp.json();
        assert!(body.ok);
        assert_eq!(body.data.unwrap().len(), 2);

        let resp = server.get("/v1/code_intel/callees").add_query_param("symbol", "a").await;
        let body: ApiResponse<Vec<String>> = resp.json();
        assert!(body.ok);
        assert_eq!(body.data.unwrap(), vec!["b"]);
    }

    #[tokio::test]
    async fn test_code_intel_cycles() {
        let config = Config::default();
        let state = AppState::init(config).await.unwrap();

        {
            let mut snap = CodeIntelSnapshot::new("test".into());
            snap.call_edges.push(CallEdge { caller: "a".into(), callee: "b".into() });
            snap.call_edges.push(CallEdge { caller: "b".into(), callee: "a".into() });
            *state.code_intel.write().await = Some(snap);
        }

        let app = code_intel_app(state);
        let server = TestServer::new(app).unwrap();

        let resp = server.get("/v1/code_intel/cycles").await;
        let body: ApiResponse<mccp_indexer::CycleReport> = resp.json();
        assert!(body.ok);
        let report = body.data.unwrap();
        assert!(!report.call_cycles.is_empty());
    }

    #[tokio::test]
    async fn test_code_intel_unused() {
        let config = Config::default();
        let state = AppState::init(config).await.unwrap();

        {
            let mut snap = CodeIntelSnapshot::new("test".into());
            let mut used = SymbolDef::new("used".into(), SymbolKind::Function, "a.rs".into(), 1, 5);
            used.references.push(SymbolRef { file: "b.rs".into(), line: 10, context: "used()".into() });
            snap.symbols.push(used);
            snap.symbols.push(SymbolDef::new("unused".into(), SymbolKind::Function, "a.rs".into(), 10, 15));
            *state.code_intel.write().await = Some(snap);
        }

        let app = code_intel_app(state);
        let server = TestServer::new(app).unwrap();

        let resp = server.get("/v1/code_intel/unused").await;
        let body: ApiResponse<Vec<SymbolDef>> = resp.json();
        assert!(body.ok);
        let unused = body.data.unwrap();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].name, "unused");
    }
}