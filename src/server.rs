use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    error::EmberlaneError,
    model::{ApiResponse, ChatRequest, RouteRequest},
    router::RuntimeRouter,
};

#[derive(Clone)]
pub struct AppState {
    pub router: RuntimeRouter,
    api_key: Option<String>,
}

impl AppState {
    pub fn new(router: RuntimeRouter) -> Self {
        let api_key = router.cfg.api_key();
        Self { router, api_key }
    }
}

pub async fn serve(router: RuntimeRouter) -> Result<(), EmberlaneError> {
    let addr: SocketAddr = format!("{}:{}", router.cfg.server.host, router.cfg.server.port)
        .parse()
        .map_err(|err| EmberlaneError::InvalidRequest(format!("invalid server address: {err}")))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("emberlane listening on http://{addr}");
    axum::serve(listener, app(AppState::new(router))).await?;
    Ok(())
}

pub fn app(state: AppState) -> Router {
    let protected = Router::new()
        .route("/v1/runtimes", get(list_runtimes))
        .route("/v1/runtimes/:runtime_id/status", get(runtime_status))
        .route("/v1/runtimes/:runtime_id/wake", post(wake_runtime))
        .route("/v1/runtimes/:runtime_id/sleep", post(sleep_runtime))
        .route("/v1/route/:runtime_id", post(route_runtime))
        .route("/v1/chat/:runtime_id", post(chat_runtime))
        .route("/v1/files", post(upload_file))
        .route("/v1/files/:file_id", get(get_file))
        .route("/v1/files/:file_id/presign", post(presign_file))
        .route("/v1/files/:file_id/route/:runtime_id", post(route_file))
        .route("/v1/chat-file/:runtime_id/:file_id", post(chat_file))
        .route("/v1/chat/completions", post(openai_chat_default))
        .route(
            "/v1/openai/:runtime_id/chat/completions",
            post(openai_chat_runtime),
        )
        .layer(middleware::from_fn_with_state(state.clone(), auth));

    Router::new()
        .route("/healthz", get(healthz))
        .merge(protected)
        .with_state(state)
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, EmberlaneError> {
    if let Some(api_key) = &state.api_key {
        let expected = format!("Bearer {api_key}");
        let got = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if got != expected {
            return Err(EmberlaneError::AuthRequired);
        }
    }
    Ok(next.run(request).await)
}

async fn healthz() -> Json<Value> {
    Json(json!({"ok": true}))
}

async fn list_runtimes(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, EmberlaneError> {
    Ok(ok(json!(state.router.list_runtimes()?)))
}

async fn runtime_status(
    State(state): State<AppState>,
    Path(runtime_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, EmberlaneError> {
    Ok(ok(json!(state.router.status(&runtime_id).await?)))
}

async fn wake_runtime(
    State(state): State<AppState>,
    Path(runtime_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, EmberlaneError> {
    state.router.wake(&runtime_id).await?;
    Ok(ok(json!({"runtime_id": runtime_id, "state": "ready"})))
}

async fn sleep_runtime(
    State(state): State<AppState>,
    Path(runtime_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, EmberlaneError> {
    state.router.sleep(&runtime_id).await?;
    Ok(ok(json!({"runtime_id": runtime_id, "state": "cold"})))
}

async fn route_runtime(
    State(state): State<AppState>,
    Path(runtime_id): Path<String>,
    Json(request): Json<RouteRequest>,
) -> Response {
    route_result(
        &state,
        &runtime_id,
        state.router.route(&runtime_id, request).await,
    )
}

async fn chat_runtime(
    State(state): State<AppState>,
    Path(runtime_id): Path<String>,
    Json(request): Json<ChatRequest>,
) -> Response {
    route_result(
        &state,
        &runtime_id,
        state.router.chat(&runtime_id, request).await,
    )
}

async fn upload_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<Value>>, EmberlaneError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| EmberlaneError::InvalidRequest(err.to_string()))?
    {
        if field.name() == Some("file") {
            let name = field.file_name().unwrap_or("upload.bin").to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|err| EmberlaneError::InvalidRequest(err.to_string()))?;
            let record = state.router.upload_bytes(&name, &bytes).await?;
            return Ok(ok(json!(record)));
        }
    }
    Err(EmberlaneError::InvalidRequest(
        "multipart field 'file' is required".to_string(),
    ))
}

async fn get_file(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, EmberlaneError> {
    Ok(ok(json!(state.router.storage.get_file(&file_id)?)))
}

#[derive(Deserialize)]
struct PresignBody {
    expires_secs: Option<u64>,
}

async fn presign_file(
    State(state): State<AppState>,
    Path(file_id): Path<String>,
    Json(body): Json<PresignBody>,
) -> Result<Json<ApiResponse<Value>>, EmberlaneError> {
    let expires_secs = body.expires_secs.unwrap_or(900);
    let url = state.router.presign_file(&file_id, expires_secs).await?;
    Ok(ok(
        json!({"file_id": file_id, "url": url, "expires_secs": expires_secs}),
    ))
}

#[derive(Deserialize)]
struct RouteFileBody {
    path: String,
    #[serde(default)]
    include_presigned_url: bool,
    expires_secs: Option<u64>,
    #[serde(default)]
    body: Value,
}

async fn route_file(
    State(state): State<AppState>,
    Path((file_id, runtime_id)): Path<(String, String)>,
    Json(body): Json<RouteFileBody>,
) -> Response {
    route_result(
        &state,
        &runtime_id,
        state
            .router
            .route_file(
                &file_id,
                &runtime_id,
                &body.path,
                body.include_presigned_url,
                body.expires_secs.unwrap_or(900),
                body.body,
            )
            .await,
    )
}

#[derive(Deserialize)]
struct ChatFileBody {
    message: String,
}

async fn chat_file(
    State(state): State<AppState>,
    Path((runtime_id, file_id)): Path<(String, String)>,
    Json(body): Json<ChatFileBody>,
) -> Response {
    route_result(
        &state,
        &runtime_id,
        state
            .router
            .chat_file(&runtime_id, &file_id, &body.message)
            .await,
    )
}

async fn openai_chat_default(State(state): State<AppState>, Json(body): Json<Value>) -> Response {
    let runtime_id = body
        .get("model")
        .and_then(Value::as_str)
        .map(|model| {
            if state
                .router
                .storage
                .load_runtime(model)
                .ok()
                .flatten()
                .is_some()
            {
                model.to_string()
            } else if model == "ollama" || model == state.router.cfg.server.default_ollama_model {
                "ollama".to_string()
            } else {
                "echo".to_string()
            }
        })
        .unwrap_or_else(|| state.router.cfg.server.default_runtime_id.clone());
    route_result(
        &state,
        &runtime_id,
        state.router.openai_chat(None, body).await,
    )
}

async fn openai_chat_runtime(
    State(state): State<AppState>,
    Path(runtime_id): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    route_result(
        &state,
        &runtime_id,
        state.router.openai_chat(Some(&runtime_id), body).await,
    )
}

fn route_result(
    state: &AppState,
    runtime_id: &str,
    result: Result<crate::model::RouteResponse, EmberlaneError>,
) -> Response {
    match result {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::OK);
            (status, Json(json!({"ok": true, "data": resp.body}))).into_response()
        }
        Err(EmberlaneError::RuntimeWarming(_)) => (
            StatusCode::ACCEPTED,
            Json(json!({"ok": true, "data": state.router.warming_body(runtime_id).unwrap()})),
        )
            .into_response(),
        Err(err) => err.into_response(),
    }
}

fn ok(data: Value) -> Json<ApiResponse<Value>> {
    Json(ApiResponse { ok: true, data })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    fn test_app() -> Router {
        let mut cfg = crate::config::EmberlaneConfig::default();
        cfg.server.api_key = Some("secret".to_string());
        cfg.runtimes[0].provider = crate::model::ProviderKind::Mock;
        cfg.runtimes[0].config = json!({});
        cfg.runtimes.truncate(1);
        let storage = crate::storage::Storage::open_memory().unwrap();
        let router = RuntimeRouter::new(cfg, storage);
        router.seed_config_runtimes().unwrap();
        app(AppState::new(router))
    }

    #[tokio::test]
    async fn healthz_no_auth() {
        let resp = test_app()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn protected_endpoint_rejects_missing_auth_and_accepts_valid_auth() {
        let app = test_app();
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/runtimes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/runtimes")
                    .header("authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn echo_openai_compatible_endpoint_works() {
        let resp = test_app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("authorization", "Bearer secret")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"model":"echo","messages":[{"role":"user","content":"hello"}],"stream":false}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(
            body["data"]["choices"][0]["message"]["content"],
            "Echo: hello"
        );
    }

    #[tokio::test]
    async fn http_chat_echo_works() {
        let resp = test_app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/echo")
                    .header("authorization", "Bearer secret")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"messages":[{"role":"user","content":"hello"}],"files":[]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body["data"]["reply"], "Echo: hello");
    }
}
