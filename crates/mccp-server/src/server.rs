use super::*;
use mccp_core::*;
use axum::{
    routing::{get, post},
    Router, Json, extract::State, response::IntoResponse,
};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_stream::StreamExt;

/// Application state shared across the server
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub query_engine: QueryEngine,
    pub pipeline: Arc<IndexingPipeline>,
}

impl AppState {
    /// Initialize application state
    pub async fn init(config: Config) -> Result<Self> {
        let query_engine = QueryEngine::new(config.clone()).await?;
        let pipeline = Arc::new(IndexingPipeline::new(
            Project::new("default".to_string(), &std::path::PathBuf::from(".")),
            config.indexer.clone(),
        ));

        Ok(Self {
            config,
            query_engine,
            pipeline,
        })
    }
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

/// Run HTTP server
pub async fn run_http(state: AppState, addr: &str) -> anyhow::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/query", post(query))
        .route("/v1/find_usages", post(find_usages))
        .route("/v1/get_flow", post(get_flow))
        .route("/v1/get_summary", post(get_summary))
        .route("/v1/get_related", post(get_related))
        .route("/v1/find_definition", post(find_definition))
        .route("/v1/get_symbol_map", post(get_symbol_map))
        .route("/index/progress", get(index_progress_sse))
        .route("/index/status", get(index_status))
        .route("/projects", get(list_projects))
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
    Json(req): Json<QueryRequest>,
) -> impl IntoResponse {
    match state.query_engine.query(req).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Find usages endpoint
async fn find_usages(
    State(state): State<AppState>,
    Json(req): Json<FindUsagesRequest>,
) -> impl IntoResponse {
    match state.query_engine.find_usages(req).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Get flow endpoint
async fn get_flow(
    State(state): State<AppState>,
    Json(req): Json<GetFlowRequest>,
) -> impl IntoResponse {
    match state.query_engine.get_flow(req).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Get summary endpoint
async fn get_summary(
    State(state): State<AppState>,
    Json(req): Json<GetSummaryRequest>,
) -> impl IntoResponse {
    match state.query_engine.get_summary(req).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Get related endpoint
async fn get_related(
    State(state): State<AppState>,
    Json(req): Json<GetRelatedRequest>,
) -> impl IntoResponse {
    match state.query_engine.get_related(req).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Find definition endpoint
async fn find_definition(
    State(state): State<AppState>,
    Json(req): Json<FindDefinitionRequest>,
) -> impl IntoResponse {
    match state.query_engine.find_definition(req).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Get symbol map endpoint
async fn get_symbol_map(
    State(state): State<AppState>,
    Json(req): Json<GetSymbolMapRequest>,
) -> impl IntoResponse {
    match state.query_engine.get_symbol_map(req).await {
        Ok(results) => (StatusCode::OK, Json(results)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(format!("Error: {}", e))).into_response(),
    }
}

/// Index progress SSE endpoint
async fn index_progress_sse(
    State(state): State<AppState>,
) -> Sse<impl StreamExt<Item = Result<Event, Infallible>>> {
    let mut rx = state.pipeline.progress_rx.clone();
    let stream = async_stream::stream! {
        loop {
            rx.changed().await.ok();
            if let Some(p) = rx.borrow().clone() {
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
    State(state): State<AppState>,
) -> impl IntoResponse {
    // TODO: Implement project listing
    Json(vec![])
}

/// Request types for REST API
#[derive(Deserialize)]
pub struct QueryRequest {
    pub project: String,
    pub query: String,
    pub top_k: usize,
    pub filters: Option<Filters>,
}

#[derive(Deserialize)]
pub struct FindUsagesRequest {
    pub project: String,
    pub symbol: String,
    pub symbol_kind: Option<SymbolKind>,
    pub ref_kind: Option<Vec<RefKind>>,
    pub file_pattern: Option<String>,
}

#[derive(Deserialize)]
pub struct GetFlowRequest {
    pub project: String,
    pub entry: String,
    pub max_depth: Option<usize>,
}

#[derive(Deserialize)]
pub struct GetSummaryRequest {
    pub project: String,
    pub path: String,
    pub scope: Option<String>,
}

#[derive(Deserialize)]
pub struct GetRelatedRequest {
    pub project: String,
    pub path: String,
    pub depth: Option<usize>,
}

#[derive(Deserialize)]
pub struct FindDefinitionRequest {
    pub project: String,
    pub symbol: String,
    pub scope_hint: Option<String>,
}

#[derive(Deserialize)]
pub struct GetSymbolMapRequest {
    pub project: String,
    pub path: String,
}

/// Response envelope for REST API
#[derive(Serialize)]
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
}