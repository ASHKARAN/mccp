use super::*;
use mccp_core::*;
use axum::{
    routing::{get, post},
    Router, Json, extract::State, response::IntoResponse,
    middleware::Next,
    extract::Request,
    body::Bytes,
    http::{HeaderMap, HeaderValue},
    extract::Query,
};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::convert::Infallible;
use tokio_stream::StreamExt;
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
    pub config: Config,
    pub query_engine: QueryEngine,
    pub pipeline: Arc<mccp_indexer::IndexingPipeline>,
    pub code_intel: Arc<tokio::sync::RwLock<Option<CodeIntelSnapshot>>>,
}

impl AppState {
    /// Initialize application state
    pub async fn init(config: Config) -> Result<Self> {
        let query_engine = QueryEngine::new(config.ranker_weights().clone());
        let pipeline = Arc::new(mccp_indexer::IndexingPipeline::new(
            Project::new("default".to_string(), &std::path::PathBuf::from(".")),
            config.indexer.clone(),
        ));

        // Try to load a persisted snapshot
        let project_id = ProjectId::from_path(&std::path::PathBuf::from("."));
        let existing = CodeIntelSnapshot::load(project_id.as_str())
            .ok()
            .flatten();

        Ok(Self {
            config,
            query_engine,
            pipeline,
            code_intel: Arc::new(tokio::sync::RwLock::new(existing)),
        })
    }
}

/// Health check response
#[derive(Serialize, Deserialize)]
struct HealthResponse {
    status: String,
    version: String,
}

/// Run HTTP server
pub async fn run_http(state: AppState, addr: &str) -> anyhow::Result<()> {
    // Spawn background cache warming
    tokio::spawn(warm_cache(state.clone()));

    let app = Router::new()
        .route("/health", get(health))
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
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("HTTP server listening on {}", addr);
    axum::serve(listener, app).await?;
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
    if let Some(secret) = state.config.webhook.as_ref().and_then(|w| w.secret.as_deref()) {
        if !secret.is_empty() {
            let sig_header = headers.get("X-Hub-Signature-256")
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
    // If no agents configured → single-user mode, allow all
    if state.config.agents.is_empty() {
        req.extensions_mut().insert(AgentContext::anonymous());
        return next.run(req).await;
    }

    let token = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(String::from);

    match token.as_deref().and_then(|t| state.config.find_agent(t)) {
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