//! Exec 命令执行 API：NDJSON 流式事件与进程树终止（design D6）。

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use super::state::ServerState;

/// 超时缺省值（与 Python 版一致）。
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 1800;

/// `POST /exec` 请求体。
#[derive(Debug, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

/// 校验通过后的执行参数。
pub struct Prepared {
    pub cwd: PathBuf,
    pub timeout_seconds: u64,
}

enum ExecEvent {
    Output(String),
    Exit {
        code: i64,
        duration_ms: u64,
        timed_out: bool,
    },
}

/// 校验并解析执行参数（路由层在留痕后调用）。
pub fn prepare(workdir: &Path, req: &ExecRequest) -> Result<Prepared, String> {
    let cwd = match req.cwd.as_deref() {
        Some(raw) => {
            let p = PathBuf::from(raw);
            if p.is_absolute() {
                p
            } else {
                workdir.join(p)
            }
        }
        None => workdir.to_path_buf(),
    };
    if !cwd.is_dir() {
        return Err(format!(
            "cwd 不存在或不是目录：{}",
            req.cwd.as_deref().unwrap_or_default()
        ));
    }
    Ok(Prepared {
        cwd,
        timeout_seconds: req.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS),
    })
}

/// 启动执行并以 NDJSON 流式响应返回。
pub fn spawn_stream(
    state: Arc<ServerState>,
    req: ExecRequest,
    prepared: Prepared,
    request_id: u64,
) -> Response {
    let started = Instant::now();
    let (tx, rx) = mpsc::channel::<ExecEvent>(256);
    let pid_slot = Arc::new(AtomicU32::new(0));
    tokio::spawn(run(
        state,
        req,
        prepared,
        request_id,
        started,
        tx,
        pid_slot.clone(),
    ));

    let stream = ExecStream {
        rx,
        pid_slot,
        finished: false,
    };
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/x-ndjson")
        .body(Body::from_stream(stream))
        .expect("构造流式响应")
}

/// NDJSON 事件流：客户端断开（Body 被 drop）时终止进程树（design D6）。
struct ExecStream {
    rx: mpsc::Receiver<ExecEvent>,
    /// 子进程 pid（0 = 尚未 spawn 或已正常收尾）。
    pid_slot: Arc<AtomicU32>,
    finished: bool,
}

impl futures_util::Stream for ExecStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.as_mut().get_mut();
        match this.rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(event)) => {
                let line = match event {
                    ExecEvent::Output(data) => {
                        json!({"type": "output", "data": data}).to_string()
                    }
                    ExecEvent::Exit {
                        code,
                        duration_ms,
                        timed_out,
                    } => {
                        this.finished = true;
                        if timed_out {
                            json!({"type": "exit", "code": code, "duration_ms": duration_ms, "timed_out": true})
                                .to_string()
                        } else {
                            json!({"type": "exit", "code": code, "duration_ms": duration_ms})
                                .to_string()
                        }
                    }
                };
                std::task::Poll::Ready(Some(Ok(Bytes::from(format!("{line}\n")))))
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl Drop for ExecStream {
    fn drop(&mut self) {
        if !self.finished {
            let pid = self.pid_slot.load(Ordering::Acquire);
            if pid != 0 {
                kill_tree_blocking(Some(pid));
            }
        }
    }
}

/// 同步版进程树终止（用于 Drop 等无法 await 的场合）。
pub fn kill_tree_blocking(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

async fn run(
    state: Arc<ServerState>,
    req: ExecRequest,
    prepared: Prepared,
    request_id: u64,
    started: Instant,
    tx: mpsc::Sender<ExecEvent>,
    pid_slot: Arc<AtomicU32>,
) {
    #[cfg(windows)]
    let mut command = {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(&req.command);
        c
    };
    #[cfg(not(windows))]
    let mut command = {
        let mut c = tokio::process::Command::new("/bin/sh");
        c.arg("-c").arg(&req.command);
        c
    };
    command
        .current_dir(&prepared.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = tx
                .send(ExecEvent::Output(format!("（启动命令失败：{e}）\n")))
                .await;
            let _ = tx
                .send(ExecEvent::Exit {
                    code: -1,
                    duration_ms: started.elapsed().as_millis() as u64,
                    timed_out: false,
                })
                .await;
            log_result(&state, request_id, -1, started, false, 0);
            return;
        }
    };
    let pid = child.id();
    if let Some(p) = pid {
        pid_slot.store(p, Ordering::Release);
    }
    let output_bytes = Arc::new(AtomicU64::new(0));

    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(pipe_output(
            stdout,
            tx.clone(),
            pid,
            output_bytes.clone(),
        ));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(pipe_output(
            stderr,
            tx.clone(),
            pid,
            output_bytes.clone(),
        ));
    }

    // 等待结束或超时（超时终止进程树）
    let timeout = tokio::time::sleep(Duration::from_secs(prepared.timeout_seconds));
    tokio::pin!(timeout);
    let (status, timed_out) = tokio::select! {
        status = child.wait() => (status.ok(), false),
        _ = &mut timeout => {
            kill_tree(pid).await;
            (child.wait().await.ok(), true)
        }
    };
    let code = status.as_ref().and_then(exit_code).unwrap_or(-1);
    let duration_ms = started.elapsed().as_millis() as u64;
    let bytes = output_bytes.load(Ordering::Relaxed);
    let _ = tx
        .send(ExecEvent::Exit {
            code,
            duration_ms,
            timed_out,
        })
        .await;
    log_result(&state, request_id, code, started, timed_out, bytes);
}

/// 转发一端输出到事件通道；发送失败（客户端断开）→ 终止进程树。
async fn pipe_output<R>(mut stream: R, tx: mpsc::Sender<ExecEvent>, pid: Option<u32>, counter: Arc<AtomicU64>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut decoder = Utf8Streamer::default();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                counter.fetch_add(n as u64, Ordering::Relaxed);
                let text = decoder.push(&buf[..n]);
                if !text.is_empty() && tx.send(ExecEvent::Output(text)).await.is_err() {
                    kill_tree(pid).await;
                    return;
                }
            }
        }
    }
    let tail = decoder.flush();
    if !tail.is_empty() {
        let _ = tx.send(ExecEvent::Output(tail)).await;
    }
}

/// 终止进程树：Unix 杀进程组（子进程为组长）、Windows 用 taskkill /T。
pub async fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = tokio::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
}

fn exit_code(status: &std::process::ExitStatus) -> Option<i64> {
    if let Some(code) = status.code() {
        return Some(code as i64);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        return status.signal().map(|s| -(s as i64));
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn log_result(
    state: &ServerState,
    request_id: u64,
    code: i64,
    started: Instant,
    timed_out: bool,
    output_bytes: u64,
) {
    let mut log = state.log();
    let mut text = format!(
        "#{request_id} 结束：退出码 {code} · 耗时 {}ms · 输出 {output_bytes} 字节",
        started.elapsed().as_millis()
    );
    if timed_out {
        text.push_str(" · 已超时（进程树已终止）");
    }
    log.detail(&text);
}

/// 流式 UTF-8 解码（跨块边界安全；非法字节 Windows 上按 GBK 回退、其余平台替换字符）。
#[derive(Default)]
struct Utf8Streamer {
    pending: Vec<u8>,
}

impl Utf8Streamer {
    fn push(&mut self, chunk: &[u8]) -> String {
        self.pending.extend_from_slice(chunk);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    if valid > 0 {
                        if let Ok(text) = std::str::from_utf8(&self.pending[..valid]) {
                            out.push_str(text);
                        }
                    }
                    match e.error_len() {
                        Some(len) => {
                            let bad_end = (valid + len).min(self.pending.len());
                            out.push_str(&decode_invalid(&self.pending[valid..bad_end]));
                            self.pending.drain(..bad_end);
                        }
                        None => {
                            // 尾部不完整序列：保留，等待后续字节
                            self.pending.drain(..valid);
                            break;
                        }
                    }
                }
            }
        }
        out
    }

    fn flush(&mut self) -> String {
        if self.pending.is_empty() {
            return String::new();
        }
        let text = decode_invalid(&self.pending);
        self.pending.clear();
        text
    }
}

fn decode_invalid(bytes: &[u8]) -> String {
    #[cfg(windows)]
    {
        encoding_rs::GBK
            .decode_without_bom_handling(bytes)
            .0
            .into_owned()
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}
