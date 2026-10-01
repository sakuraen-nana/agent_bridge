//! 服务端（design D1/D2）：axum + 独立 tokio runtime；生命周期由 [`ServerHandle`] 管理。

pub mod auth;
pub mod download;
pub mod exec;
pub mod hello;
pub mod log;
pub mod state;

pub use state::{ServerConfig, ServerState};

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{rejection::JsonRejection, ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::error::AppError;

/// 服务端句柄：持有 runtime 与后台任务；`stop()` 后端口释放。
pub struct ServerHandle {
    runtime: Option<tokio::runtime::Runtime>,
    join: Option<tokio::task::JoinHandle<()>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    state: Arc<ServerState>,
}

impl ServerHandle {
    /// 实际绑定的端口（端口传 0 时为系统分配的端口）。
    pub fn port(&self) -> u16 {
        self.state.port
    }

    /// 本次会话 token（仅存内存；供剪贴板/界面使用）。
    pub fn session_token(&self) -> &str {
        &self.state.session_token
    }

    /// 运行期状态（发现表、配对待决等）。
    pub fn state(&self) -> Arc<ServerState> {
        self.state.clone()
    }

    /// 停止服务并释放端口。
    pub fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let runtime = self.runtime.take();
        if let (Some(rt), Some(join)) = (runtime.as_ref(), self.join.take()) {
            let _ = rt.block_on(async {
                tokio::time::timeout(std::time::Duration::from_secs(2), join).await
            });
        }
        if let Some(rt) = runtime {
            rt.shutdown_timeout(std::time::Duration::from_secs(2));
        }
    }
}

/// 启动服务端：同步完成端口绑定（占用即失败、不换端口），随后在后台 runtime 提供 HTTP。
pub fn start(config: ServerConfig) -> Result<ServerHandle, AppError> {
    let std_listener = std::net::TcpListener::bind(("0.0.0.0", config.port))
        .map_err(|e| AppError::PortInUse(config.port, e.to_string()))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| AppError::Server(e.to_string()))?;
    let bound_port = std_listener
        .local_addr()
        .map_err(|e| AppError::Server(e.to_string()))?
        .port();
    let state = Arc::new(ServerState::new(&config, bound_port)?);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| AppError::Server(e.to_string()))?;
    let listener = {
        let _guard = runtime.enter();
        tokio::net::TcpListener::from_std(std_listener)
            .map_err(|e| AppError::Server(e.to_string()))?
    };
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let join = runtime.spawn(serve(listener, state.clone(), shutdown_rx));

    // 发现信标（UDP 绑定失败不阻断主服务；仅记为状态）
    match crate::discovery::bind_socket() {
        Ok(socket) => {
            let discovery_state = state.clone();
            runtime.spawn(crate::discovery::run(discovery_state, socket));
        }
        Err(e) => {
            *state.discovery_error.lock().unwrap() = Some(e.to_string());
        }
    }

    Ok(ServerHandle {
        runtime: Some(runtime),
        join: Some(join),
        shutdown: Some(shutdown_tx),
        state,
    })
}

async fn serve(
    listener: tokio::net::TcpListener,
    state: Arc<ServerState>,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) {
    let app = router(state);
    let _ = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        let _ = shutdown.await;
    })
    .await;
}

fn router(state: Arc<ServerState>) -> Router {
    Router::new()
        .route("/hello", post(hello_handler))
        .route("/exec", post(exec_handler))
        .route("/download", post(download_handler))
        .route("/pair/request", post(pair_request_handler))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(state)
}

#[derive(Deserialize)]
struct TokenQuery {
    #[serde(default)]
    token: Option<String>,
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

fn source_of(addr: &SocketAddr) -> String {
    addr.to_string()
}

/// 认证前置：返回 Some(拒绝响应) 表示应直接返回（已留痕）。
fn gate(
    state: &ServerState,
    token: &auth::TokenStatus,
    id: u64,
    method: &str,
    path: &str,
    source: &str,
) -> Option<Response> {
    if token.is_valid() {
        return None;
    }
    let mut log = state.log();
    log.request_line(id, method, path, source, &token.describe());
    log.detail(&format!("#{id} 已拒绝 · 404"));
    Some(not_found())
}

async fn hello_handler(
    State(state): State<Arc<ServerState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<TokenQuery>,
) -> Response {
    let id = state.next_request_id();
    let token = auth::check(&state, query.token.as_deref());
    if let Some(rejected) = gate(&state, &token, id, "POST", "/hello", &source_of(&addr)) {
        return rejected;
    }
    let payload = hello::payload(&state);
    {
        let mut log = state.log();
        log.request_line(id, "POST", "/hello", &source_of(&addr), &token.describe());
        log.detail(&format!("响应: {payload}"));
    }
    Json(payload).into_response()
}

async fn exec_handler(
    State(state): State<Arc<ServerState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<TokenQuery>,
    body: Result<Json<exec::ExecRequest>, JsonRejection>,
) -> Response {
    let id = state.next_request_id();
    let token = auth::check(&state, query.token.as_deref());
    if let Some(rejected) = gate(&state, &token, id, "POST", "/exec", &source_of(&addr)) {
        return rejected;
    }
    let Ok(Json(request)) = body else {
        let mut log = state.log();
        log.request_line(id, "POST", "/exec", &source_of(&addr), &token.describe());
        log.detail(&format!("#{id} 已拒绝 · 400 · 请求体解析失败"));
        return download::error_json(StatusCode::BAD_REQUEST, "请求体解析失败".to_string());
    };
    {
        let mut log = state.log();
        log.request_line(id, "POST", "/exec", &source_of(&addr), &token.describe());
        log.detail(&format!("command = {}", request.command));
        match request.cwd.as_deref() {
            Some(cwd) => log.detail(&format!("cwd = {cwd}（相对时基于 {}）", state.workdir.display())),
            None => log.detail(&format!("cwd = {}（未提供 → 默认工作目录）", state.workdir.display())),
        }
        let timeout = request
            .timeout_seconds
            .unwrap_or(exec::DEFAULT_TIMEOUT_SECONDS);
        let source = if request.timeout_seconds.is_some() {
            "显式"
        } else {
            "缺省"
        };
        log.detail(&format!("timeout_seconds = {timeout}（{source}）"));
    }
    match exec::prepare(&state.workdir, &request) {
        Ok(prepared) => exec::spawn_stream(state.clone(), request, prepared, id),
        Err(message) => {
            state
                .log()
                .detail(&format!("#{id} 已拒绝 · 400 · {message}"));
            download::error_json(StatusCode::BAD_REQUEST, message)
        }
    }
}

/// `POST /pair/request`（全应用唯一免 token 端点；design D2）：
/// 入待决表 → 长轮询等待使用者决定（上限 120 秒）→ 同意时返回含长期 token 的配置。
#[derive(Deserialize)]
struct PairRequestBody {
    uuid: String,
    #[serde(default)]
    short_name: Option<String>,
    port: u16,
}

async fn pair_request_handler(
    State(state): State<Arc<ServerState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    body: Result<Json<PairRequestBody>, JsonRejection>,
) -> Response {
    let id = state.next_request_id();
    let source_ip = addr.ip().to_string();
    {
        let mut log = state.log();
        log.request_line(
            id,
            "POST",
            "/pair/request",
            &source_of(&addr),
            "免认证端点（配对请求）",
        );
    }
    let Ok(Json(request)) = body else {
        state
            .log()
            .detail(&format!("#{id} 已拒绝 · 400 · 请求体解析失败"));
        return download::error_json(StatusCode::BAD_REQUEST, "请求体解析失败".to_string());
    };
    if uuid::Uuid::parse_str(&request.uuid).is_err() {
        state
            .log()
            .detail(&format!("#{id} 已拒绝 · 400 · uuid 非法"));
        return download::error_json(StatusCode::BAD_REQUEST, "uuid 非法".to_string());
    }
    let requester = crate::pairing::PairRequester {
        uuid: request.uuid.clone(),
        short_name: request
            .short_name
            .clone()
            .filter(|name| !name.trim().is_empty()),
        port: request.port,
        source_ip,
    };
    let receiver = match crate::pairing::begin(&state, requester) {
        Ok(receiver) => receiver,
        Err(()) => {
            state
                .log()
                .detail(&format!("#{id} 已拒绝 · 429 · 已有待处理的配对请求"));
            return download::error_json(
                StatusCode::TOO_MANY_REQUESTS,
                "已有待处理的配对请求".to_string(),
            );
        }
    };
    state.log().detail(&format!(
        "配对请求：{}（{}），等待使用者决定（上限 {} 秒）",
        request
            .short_name
            .clone()
            .unwrap_or_else(|| "未命名".to_string()),
        request.uuid,
        state.pair_timeout_secs
    ));
    match tokio::time::timeout(
        std::time::Duration::from_secs(state.pair_timeout_secs),
        receiver,
    )
    .await
    {
        Ok(Ok(true)) => {
            let (uuid, short_name) = state.device_snapshot();
            let token = state.long_term_token();
            state
                .log()
                .detail(&format!("#{id} 已同意 · 已返回本机配置（含长期 token，值不落日志）"));
            Json(json!({
                "approved": true,
                "config": {
                    "uuid": uuid,
                    "short_name": short_name,
                    "port": state.port,
                    "token": token,
                }
            }))
            .into_response()
        }
        Ok(Ok(false)) => {
            state.log().detail(&format!("#{id} 已拒绝"));
            Json(json!({ "approved": false, "reason": "被拒绝" })).into_response()
        }
        Ok(Err(_)) => {
            state.log().detail(&format!("#{id} 服务端关闭，请求中断"));
            Json(json!({ "approved": false, "reason": "服务端关闭" })).into_response()
        }
        Err(_) => {
            crate::pairing::expire(&state, id);
            state.log().detail(&format!("#{id} 超时（无人决定）"));
            Json(json!({ "approved": false, "reason": "超时（无人决定）" })).into_response()
        }
    }
}

async fn download_handler(
    State(state): State<Arc<ServerState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(query): Query<TokenQuery>,
    body: Result<Json<download::DownloadRequest>, JsonRejection>,
) -> Response {
    let id = state.next_request_id();
    let token = auth::check(&state, query.token.as_deref());
    if let Some(rejected) = gate(&state, &token, id, "POST", "/download", &source_of(&addr)) {
        return rejected;
    }
    let Ok(Json(request)) = body else {
        let mut log = state.log();
        log.request_line(id, "POST", "/download", &source_of(&addr), &token.describe());
        log.detail(&format!("#{id} 已拒绝 · 400 · 请求体解析失败"));
        return download::error_json(StatusCode::BAD_REQUEST, "请求体解析失败".to_string());
    };
    match download::prepare(&state.workdir, &request).await {
        Ok(prepared) => {
            {
                let mut log = state.log();
                log.request_line(id, "POST", "/download", &source_of(&addr), &token.describe());
                log.detail(&format!("path = {}", prepared.raw_path));
                log.detail(&format!("解析后路径 = {}", prepared.resolved.display()));
                log.detail(&format!("#{id} 发送 {} 字节", prepared.size));
            }
            download::stream_response(prepared)
        }
        Err(response) => {
            let mut log = state.log();
            log.request_line(id, "POST", "/download", &source_of(&addr), &token.describe());
            log.detail(&format!("path = {}", request.path));
            log.detail(&format!(
                "#{id} 已拒绝 · {}",
                response.status().as_u16()
            ));
            response
        }
    }
}
