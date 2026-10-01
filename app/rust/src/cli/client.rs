//! CLI 的 HTTP 客户端（reqwest 流式；仅局域网明文 HTTP，不引入 TLS）。

use std::io::Write as _;
use std::path::PathBuf;

use futures_util::StreamExt;
use serde_json::{json, Value};

use super::CliFailure;
use crate::config::Peer;

/// 构建 CLI 用的单线程 tokio runtime。
pub fn runtime() -> Result<tokio::runtime::Runtime, CliFailure> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| CliFailure::Network(format!("无法初始化网络运行时：{e}")))
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        // 仅限制建连；流式请求（exec）不得设整体超时
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("构建 HTTP 客户端")
}

/// 带 token 的请求 URL（token 恒为本应用生成的十六进制串，无需再转义）。
fn url(peer: &Peer, path: &str) -> String {
    format!(
        "http://{}:{}{}?token={}",
        peer.address, peer.port, path, peer.token
    )
}

fn network_error(e: reqwest::Error) -> CliFailure {
    CliFailure::Network(format!("连接失败：{}", sanitize(&e.to_string())))
}

/// 错误文案脱敏：抹掉 URL 中的 token 取值（reqwest 的 Display 会带上完整 URL）。
fn sanitize(text: &str) -> String {
    let Some(start) = text.find("?token=") else {
        return text.to_string();
    };
    let rest = &text[start + "?token=".len()..];
    let end = rest
        .find(|c: char| c == ')' || c == ' ' || c == '\n' || c == ',' || c == '"')
        .map(|p| start + "?token=".len() + p)
        .unwrap_or(text.len());
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str("?token=***");
    out.push_str(&text[end..]);
    out
}

/// 状态处理：成功直通；404 无 JSON 体 → token 被拒；其余错误 → 业务失败（JSON 错误优先）。
async fn ensure_ok(resp: reqwest::Response) -> Result<reqwest::Response, CliFailure> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    let json_error = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.as_str())
                .map(str::to_string)
        });
    if let Some(message) = json_error {
        return Err(CliFailure::Business(message));
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(CliFailure::TokenRejected(
            "token 可能已失效（被重置或对端轮换）——请到对端设备重新获取 token 后更新配置"
                .to_string(),
        ));
    }
    Err(CliFailure::Business(format!("服务端返回 {status}")))
}

/// `hello`：打印目标设备信息。
pub async fn hello(peer: &Peer) -> Result<i32, CliFailure> {
    let resp = client()
        .post(url(peer, "/hello"))
        .send()
        .await
        .map_err(network_error)?;
    let resp = ensure_ok(resp).await?;
    let value: Value = resp
        .json()
        .await
        .map_err(|e| CliFailure::Network(format!("解析响应失败：{e}")))?;
    println!("设备信息：");
    let fields = [
        ("uuid", "UUID"),
        ("short_name", "短名"),
        ("version", "版本"),
        ("hostname", "主机名"),
        ("user", "用户"),
        ("system", "系统"),
        ("release", "内核"),
        ("platform", "系统版本"),
        ("cwd", "默认工作目录"),
        ("started_at", "服务启动时刻"),
        ("lan_ips", "局域网 IP"),
    ];
    for (key, label) in fields {
        let text = match value.get(key) {
            None | Some(Value::Null) => "（未设置）".to_string(),
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        };
        println!("  {label:<12} {text}");
    }
    Ok(0)
}

/// `exec`：流式转发输出；返回远端退出码（负值按 128+信号 归一）。
pub async fn exec(peer: &Peer, command: &str, timeout: Option<u64>) -> Result<i32, CliFailure> {
    let mut body = json!({ "command": command });
    if let Some(timeout) = timeout {
        body["timeout_seconds"] = json!(timeout);
    }
    let resp = client()
        .post(url(peer, "/exec"))
        .json(&body)
        .send()
        .await
        .map_err(network_error)?;
    let resp = ensure_ok(resp).await?;

    let mut stream = resp.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    let mut exit: Option<(i64, bool)> = None;
    let stdout = std::io::stdout();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| CliFailure::Network(format!("流式读取中断：{}", sanitize(&e.to_string()))))?;
        buffer.extend_from_slice(&chunk);
        while let Some(position) = buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=position).collect();
            let line = String::from_utf8_lossy(&line[..line.len() - 1]);
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                match value.get("type").and_then(|t| t.as_str()) {
                    Some("output") => {
                        let data = value
                            .get("data")
                            .and_then(|d| d.as_str())
                            .unwrap_or_default();
                        print!("{data}");
                        let _ = stdout.lock().flush();
                    }
                    Some("exit") => {
                        let code = value.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
                        let timed_out = value
                            .get("timed_out")
                            .and_then(|t| t.as_bool())
                            .unwrap_or(false);
                        exit = Some((code, timed_out));
                    }
                    _ => {}
                }
            }
        }
    }
    match exit {
        Some((code, timed_out)) => {
            if timed_out {
                eprintln!("（远端命令超时，进程树已被终止）");
            }
            Ok(normalize_exit_code(code))
        }
        None => Err(CliFailure::Network(
            "流式响应中断：未收到退出事件".to_string(),
        )),
    }
}

/// `download`：下载到本地（缺省当前目录同名文件），校验完整性。
pub async fn download(peer: &Peer, path: &str, out: Option<PathBuf>) -> Result<i32, CliFailure> {
    let resp = client()
        .post(url(peer, "/download"))
        .json(&json!({ "path": path }))
        .send()
        .await
        .map_err(network_error)?;
    let resp = ensure_ok(resp).await?;
    let total = resp.content_length();

    let dest = out.unwrap_or_else(|| {
        let base = path.rsplit(['/', '\\']).next().unwrap_or_default();
        let base = if base.is_empty() { "download.bin" } else { base };
        PathBuf::from(base)
    });
    let mut file = std::fs::File::create(&dest).map_err(|e| {
        CliFailure::Business(format!("无法创建本地文件 {}：{e}", dest.display()))
    })?;

    let mut stream = resp.bytes_stream();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| CliFailure::Network(format!("下载中断：{e}")))?;
        file.write_all(&chunk)
            .map_err(|e| CliFailure::Business(format!("写入失败：{e}")))?;
        written += chunk.len() as u64;
    }
    if let Some(total) = total {
        if written != total {
            return Err(CliFailure::Business(format!(
                "下载不完整：期望 {total} 字节，实收 {written} 字节"
            )));
        }
    }
    println!("已保存到 {}", dest.display());
    Ok(0)
}

/// 远端退出码归一：负值（信号终止）→ 128+信号；收敛到 0–255。
fn normalize_exit_code(code: i64) -> i32 {
    let code = if code < 0 { 128 + (-code) } else { code };
    code.clamp(0, 255) as i32
}
