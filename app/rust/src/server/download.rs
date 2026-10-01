//! Download 文件下载 API：octet-stream 流式与明确的错误 JSON。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use tokio_util::io::ReaderStream;

/// `POST /download` 请求体。
#[derive(Debug, Deserialize)]
pub struct DownloadRequest {
    pub path: String,
}

/// 校验通过后的下载上下文。
pub struct DownloadTarget {
    pub raw_path: String,
    pub resolved: PathBuf,
    pub size: u64,
    pub file: tokio::fs::File,
}

/// 解析并打开目标（相对路径基于默认工作目录）；失败返回既成的 JSON 错误响应。
pub async fn prepare(workdir: &Path, req: &DownloadRequest) -> Result<DownloadTarget, Response> {
    let raw = req.path.clone();
    let mut resolved = PathBuf::from(&raw);
    if !resolved.is_absolute() {
        resolved = workdir.join(resolved);
    }
    let meta = match tokio::fs::metadata(&resolved).await {
        Ok(meta) => meta,
        Err(_) => {
            return Err(error_json(
                StatusCode::NOT_FOUND,
                format!("路径不存在：{raw}"),
            ));
        }
    };
    if meta.is_dir() {
        return Err(error_json(
            StatusCode::BAD_REQUEST,
            format!("路径是目录：{raw}"),
        ));
    }
    let file = match tokio::fs::File::open(&resolved).await {
        Ok(file) => file,
        Err(e) => {
            return Err(error_json(
                StatusCode::NOT_FOUND,
                format!("无法打开文件：{raw}（{e}）"),
            ));
        }
    };
    Ok(DownloadTarget {
        raw_path: raw,
        resolved,
        size: meta.len(),
        file,
    })
}

/// 以流式响应发送文件内容。
pub fn stream_response(prepared: DownloadTarget) -> Response {
    let stream = ReaderStream::new(prepared.file);
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .header("content-length", prepared.size.to_string())
        .body(Body::from_stream(stream))
        .expect("构造下载响应")
}

/// 统一的 JSON 错误响应。
pub fn error_json(status: StatusCode, message: String) -> Response {
    (status, axum::Json(json!({ "error": message }))).into_response()
}
