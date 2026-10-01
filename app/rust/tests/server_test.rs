//! 服务端集成测试：认证、API 语义、终止语义、留痕与生命周期（进程内起服务）。
//!
//! 注意：测试一律为**同步** `#[test]`——服务端持有独立 Runtime，若在异步上下文
//! （`#[tokio::test]`）中 drop 句柄会在 tokio 内触发 "Cannot drop a runtime in a
//! context where blocking is not allowed" panic。网络调用统一经显式 `block_on`。

use std::path::PathBuf;
use std::time::Duration;

use agent_bridge::config::{self, CONFIG_FILE_NAME};
use agent_bridge::server::{self, ServerConfig};
use futures_util::StreamExt;
use serde_json::json;

struct Harness {
    _dir: tempfile::TempDir,
    dir: PathBuf,
    handle: Option<server::ServerHandle>,
    port: u16,
    session_token: String,
    long_term_token: String,
}

impl Harness {
    fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let dir_path = dir.path().to_path_buf();
        let outcome = config::load_or_create(&dir_path).unwrap();
        let session_token = "s".repeat(64);
        let handle = server::start(ServerConfig::new(
            0,
            dir_path.clone(),
            session_token.clone(),
            Some(dir_path.to_string_lossy().into_owned()),
        ))
        .unwrap();
        let port = handle.port();
        Self {
            _dir: dir,
            dir: dir_path,
            handle: Some(handle),
            port,
            session_token,
            long_term_token: outcome.device.long_term_token,
        }
    }

    fn url(&self, path: &str, token: &str) -> String {
        format!("http://127.0.0.1:{}{}?token={}", self.port, path, token)
    }

    fn stop(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
    }
}

fn client_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().unwrap()
}

#[test]
fn auth_accepts_session_and_long_term_and_rejects_else() {
    let h = Harness::start();
    let rt = client_runtime();
    rt.block_on(async {
        let client = reqwest::Client::new();
        // 缺失
        let r = client
            .post(format!("http://127.0.0.1:{}/hello", h.port))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404);
        let r = client
            .post(format!("http://127.0.0.1:{}/hello?token=wrong", h.port))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404);
        // 会话 token
        let r = client
            .post(h.url("/hello", &h.session_token))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        // 长期 token
        let r = client
            .post(h.url("/hello", &h.long_term_token))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
    });
    h.stop();
}

#[test]
fn hello_fields_match_config() {
    let h = Harness::start();
    let rt = client_runtime();
    rt.block_on(async {
        let value: serde_json::Value = reqwest::Client::new()
            .post(h.url("/hello", &h.long_term_token))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(value["version"], agent_bridge::version::APP_VERSION);
        assert!(value["uuid"].as_str().unwrap().len() >= 32);
        assert!(value["short_name"].is_null(), "未设置短名时应为 null");
        assert!(value["cwd"].as_str().unwrap().contains("tmp"));
        assert!(value["started_at"].as_str().unwrap().len() > 10);
    });
    h.stop();
}

#[test]
fn exec_streams_merged_output_and_exit_code() {
    let h = Harness::start();
    let rt = client_runtime();
    rt.block_on(async {
        let resp = reqwest::Client::new()
            .post(h.url("/exec", &h.long_term_token))
            .json(&json!({"command": "printf 'out-中文\\n'; printf 'err-line\\n' 1>&2; exit 3"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.unwrap();
        assert!(body.contains("out-中文"), "stdout 应包含中文输出: {body}");
        assert!(body.contains("err-line"), "stderr 应被合并: {body}");
        let exit = body
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .find(|v| v["type"] == "exit")
            .expect("应有 exit 事件");
        assert_eq!(exit["code"], 3);
        assert!(exit["duration_ms"].as_u64().is_some());
        assert!(exit.get("timed_out").is_none(), "非超时不应带 timed_out");
    });
    h.stop();
}

#[test]
fn exec_timeout_kills_process_tree() {
    let h = Harness::start();
    let rt = client_runtime();
    let marker = h.dir.join("marker-timeout");
    rt.block_on(async {
        let command = format!("sleep 2; touch {}", marker.display());
        let body = reqwest::Client::new()
            .post(h.url("/exec", &h.long_term_token))
            .json(&json!({"command": command, "timeout_seconds": 1}))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let exit = body
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .find(|v| v["type"] == "exit")
            .expect("应有 exit 事件");
        assert_eq!(exit["timed_out"], true);
        // 若进程树未被终止，sleep 2 后 touch 会创建 marker
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(!marker.exists(), "超时后派生进程不应继续执行");
    });
    h.stop();
}

#[test]
fn exec_client_disconnect_kills_process_tree() {
    let h = Harness::start();
    let rt = client_runtime();
    let marker = h.dir.join("marker-disconnect");
    rt.block_on(async {
        let command = format!("sleep 3; touch {}", marker.display());
        let resp = reqwest::Client::new()
            .post(h.url("/exec", &h.long_term_token))
            .json(&json!({"command": command}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let mut stream = resp.bytes_stream();
        // 静默命令无任何事件：等 500ms 表示流已建立，随后断开（不能 await 到命令结束）
        let _ = tokio::time::timeout(Duration::from_millis(500), stream.next()).await;
        drop(stream); // 断开
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(!marker.exists(), "断开后进程树应被终止");
    });
    h.stop();
}

#[test]
fn download_streams_bytes_and_reports_clear_errors() {
    let h = Harness::start();
    let rt = client_runtime();
    let payload = b"hello-download-\xe4\xb8\xad\xe6\x96\x87".to_vec();
    std::fs::write(h.dir.join("data.bin"), &payload).unwrap();
    rt.block_on(async {
        let client = reqwest::Client::new();
        // 成功
        let resp = client
            .post(h.url("/download", &h.long_term_token))
            .json(&json!({"path": "data.bin"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.content_length(), Some(payload.len() as u64));
        assert_eq!(resp.bytes().await.unwrap().to_vec(), payload);

        // 不存在
        let resp = client
            .post(h.url("/download", &h.long_term_token))
            .json(&json!({"path": "nope.bin"}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);
        let value: serde_json::Value = resp.json().await.unwrap();
        assert!(value["error"].as_str().unwrap().contains("不存在"));

        // 是目录
        let resp = client
            .post(h.url("/download", &h.long_term_token))
            .json(&json!({"path": "."}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
        let value: serde_json::Value = resp.json().await.unwrap();
        assert!(value["error"].as_str().unwrap().contains("目录"));
    });
    h.stop();
}

#[test]
fn log_records_requests_without_token_values_or_output() {
    let h = Harness::start();
    let rt = client_runtime();
    rt.block_on(async {
        let client = reqwest::Client::new();
        let _ = client
            .post(h.url("/exec", &h.long_term_token))
            .json(&json!({"command": "echo UNIQUE-OUTPUT-MARKER-XYZ"}))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let _ = client
            .post(format!("http://127.0.0.1:{}/hello", h.port))
            .send()
            .await
            .unwrap();
    });

    let log = std::fs::read_to_string(h.dir.join("server.log")).unwrap();
    assert!(log.contains("POST /exec"), "应记录 exec 请求: {log}");
    assert!(log.contains("command = echo UNIQUE-OUTPUT-MARKER-XYZ"));
    // 命令行出现一次；若输出内容也落日志则会出现两次
    assert_eq!(log.matches("UNIQUE-OUTPUT-MARKER-XYZ").count(), 1, "{log}");
    assert!(log.contains("token=缺失"), "被拒请求应留痕: {log}");
    assert!(!log.contains(&h.session_token), "会话 token 不得入日志");
    assert!(!log.contains(&h.long_term_token), "长期 token 不得入日志");
    h.stop();
}

#[test]
fn log_rotates_at_size_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = config::load_or_create(dir.path()).unwrap();
    let mut server_config = ServerConfig::new(
        0,
        dir.path().to_path_buf(),
        "s".repeat(64),
        Some(dir.path().to_string_lossy().into_owned()),
    );
    server_config.log_max_bytes = 256; // 小阈值促发轮转
    let handle = server::start(server_config).unwrap();
    let port = handle.port();
    let rt = client_runtime();
    rt.block_on(async {
        let client = reqwest::Client::new();
        for _ in 0..12 {
            let _ = client
                .post(format!(
                    "http://127.0.0.1:{port}/hello?token={}",
                    outcome.device.long_term_token
                ))
                .send()
                .await
                .unwrap()
                .text()
                .await;
        }
    });
    assert!(dir.path().join("server.log.1").exists(), "应发生轮转");
    handle.stop();
}

#[test]
fn port_in_use_fails_fast_and_stop_releases_port() {
    let dir = tempfile::tempdir().unwrap();
    let _ = config::load_or_create(dir.path()).unwrap();

    // 已占用 → 明确失败、不换端口
    let squatter = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
    let occupied = squatter.local_addr().unwrap().port();
    let result = server::start(ServerConfig::new(
        occupied,
        dir.path().to_path_buf(),
        "s".repeat(64),
        None,
    ));
    assert!(matches!(
        result,
        Err(agent_bridge::error::AppError::PortInUse(..))
    ));

    // stop 后端口释放
    let handle = server::start(ServerConfig::new(
        0,
        dir.path().to_path_buf(),
        "s".repeat(64),
        None,
    ))
    .unwrap();
    let port = handle.port();
    handle.stop();
    let rebind = std::net::TcpListener::bind(("0.0.0.0", port));
    assert!(rebind.is_ok(), "stop 后端口应可重新绑定");
}

#[test]
fn long_term_token_reset_takes_effect_without_restart() {
    let h = Harness::start();
    let old = h.long_term_token.clone();
    let new_token = config::reset_long_term_token(&h.dir).unwrap();
    let rt = client_runtime();
    rt.block_on(async {
        let client = reqwest::Client::new();
        let r = client.post(h.url("/hello", &old)).send().await.unwrap();
        assert_eq!(r.status(), 404, "旧长期 token 应立即失效");
        let r = client
            .post(h.url("/hello", &new_token))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "新长期 token 应立即可用");
        let r = client
            .post(h.url("/hello", &h.session_token))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200, "会话 token 不受影响");
    });
    h.stop();
}

#[test]
fn session_token_rotates_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let outcome = config::load_or_create(dir.path()).unwrap();
    let long_term = outcome.device.long_term_token.clone();
    let dir_path = dir.path().to_path_buf();
    let rt = client_runtime();

    // 第一次「启动」：会话 token s1
    let first = server::start(ServerConfig::new(
        0,
        dir_path.clone(),
        "1".repeat(64),
        Some(dir_path.to_string_lossy().into_owned()),
    ))
    .unwrap();
    let port = first.port();
    rt.block_on(async {
        let client = reqwest::Client::new();
        let ok = client
            .post(format!(
                "http://127.0.0.1:{port}/hello?token={}",
                "1".repeat(64)
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), 200);
    });
    first.stop();

    // 第二次「启动」（同数据目录、新会话 token s2）：旧会话 token 失效、新会话与长期均可用
    let second = server::start(ServerConfig::new(
        0,
        dir_path.clone(),
        "2".repeat(64),
        Some(dir_path.to_string_lossy().into_owned()),
    ))
    .unwrap();
    let port2 = second.port();
    rt.block_on(async {
        let client = reqwest::Client::new();
        let old = client
            .post(format!(
                "http://127.0.0.1:{port2}/hello?token={}",
                "1".repeat(64)
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(old.status(), 404, "旧会话 token 应随重启失效");
        let new = client
            .post(format!(
                "http://127.0.0.1:{port2}/hello?token={}",
                "2".repeat(64)
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(new.status(), 200);
        let long = client
            .post(format!("http://127.0.0.1:{port2}/hello?token={long_term}"))
            .send()
            .await
            .unwrap();
        assert_eq!(long.status(), 200, "长期 token 应跨重启有效");
    });
    second.stop();
}

#[test]
fn config_file_kept_intact_by_reads() {
    let dir = tempfile::tempdir().unwrap();
    let _ = config::load_or_create(dir.path()).unwrap();
    let path = dir.path().join(CONFIG_FILE_NAME);
    let before = std::fs::read_to_string(&path).unwrap();
    let _ = config::load_or_create(dir.path()).unwrap();
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(before, after, "合法配置的读取不应改写文件");
}
