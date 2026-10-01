//! CLI 端到端：真实二进制 × 进程内服务端（寻址、退出码、流式与 token 管理）。

use std::path::PathBuf;
use std::process::{Command, Output};

use agent_bridge::config;
use agent_bridge::server::{self, ServerConfig};

const CLI_DEVICE_UUID: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const PEER_UUID: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const PEER_UUID_2: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

struct Fixture {
    _server_dir: tempfile::TempDir,
    home: tempfile::TempDir,
    handle: Option<server::ServerHandle>,
    port: u16,
    server_uuid: String,
    server_token: String,
}

impl Fixture {
    fn start() -> Self {
        let server_dir = tempfile::tempdir().unwrap();
        let outcome = config::load_or_create(server_dir.path()).unwrap();
        let handle = server::start(ServerConfig::new(
            0,
            server_dir.path().to_path_buf(),
            "s".repeat(64),
            Some(server_dir.path().to_string_lossy().into_owned()),
        ))
        .unwrap();
        let port = handle.port();
        Self {
            _server_dir: server_dir,
            home: tempfile::tempdir().unwrap(),
            handle: Some(handle),
            port,
            server_uuid: outcome.device.uuid,
            server_token: outcome.device.long_term_token,
        }
    }

    fn server_dir(&self) -> PathBuf {
        self._server_dir.path().to_path_buf()
    }

    /// 写 CLI 侧配置：本机 device 段 + 给定 peers 段。
    fn write_cli_config(&self, peers_toml: &str) {
        let dir = self.home.path().join("agent-bridge");
        std::fs::create_dir_all(&dir).unwrap();
        let content = format!(
            "[device]\nuuid = \"{CLI_DEVICE_UUID}\"\nlong_term_token = \"{}\"\n\n{peers_toml}",
            "c".repeat(64)
        );
        std::fs::write(dir.join(config::CONFIG_FILE_NAME), content).unwrap();
    }

    /// 指向进程内服务端的标准 peers 段。
    fn default_peers(&self, token: &str) -> String {
        format!(
            "[[peer]]\nuuid = \"{}\"\nshort_name = \"dev-a\"\naddress = \"127.0.0.1\"\nport = {}\ntoken = \"{token}\"\n",
            self.server_uuid, self.port
        )
    }

    fn cli(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_agent-bridge"))
            .args(args)
            .env("XDG_CONFIG_HOME", self.home.path())
            .env("HOME", self.home.path())
            .current_dir(self.home.path())
            .output()
            .unwrap()
    }

    fn stop(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
    }
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn hello_by_short_name_and_uuid() {
    let f = Fixture::start();
    f.write_cli_config(&f.default_peers(&f.server_token));

    let out = f.cli(&["hello", "dev-a"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(stdout(&out).contains(&f.server_uuid), "{}", stdout(&out));

    let out = f.cli(&["hello", &f.server_uuid]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    f.stop();
}

#[test]
fn exec_exit_code_passthrough_and_streaming() {
    let f = Fixture::start();
    f.write_cli_config(&f.default_peers(&f.server_token));

    let out = f.cli(&["exec", "dev-a", "echo 流式输出OK; exit 7"]);
    assert_eq!(code(&out), 7, "远端退出码应透传; stderr: {}", stderr(&out));
    assert!(stdout(&out).contains("流式输出OK"));
    f.stop();
}

#[test]
fn download_default_name_and_out() {
    let f = Fixture::start();
    f.write_cli_config(&f.default_peers(&f.server_token));
    let content = "来自服务端的字节-δ";
    std::fs::write(f.server_dir().join("hello.txt"), content).unwrap();

    let out = f.cli(&["download", "dev-a", "hello.txt"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let local = f.home.path().join("hello.txt");
    assert_eq!(std::fs::read_to_string(&local).unwrap(), content);

    let out = f.cli(&["download", "dev-a", "hello.txt", "--out", "copy.txt"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(f.home.path().join("copy.txt")).unwrap(),
        content
    );
    f.stop();
}

#[test]
fn short_name_conflict_invalidates_all_and_uuid_bypasses() {
    let f = Fixture::start();
    let peers = format!(
        "[[peer]]\nuuid = \"{PEER_UUID}\"\nshort_name = \"dev\"\naddress = \"10.0.0.1\"\ntoken = \"x\"\n\n[[peer]]\nuuid = \"{PEER_UUID_2}\"\nshort_name = \"DEV\"\naddress = \"10.0.0.2\"\ntoken = \"x\"\n\n{}",
        f.default_peers(&f.server_token)
    );
    f.write_cli_config(&peers);

    // peers 列表标记冲突
    let out = f.cli(&["peers"]);
    assert_eq!(code(&out), 0);
    let text = stdout(&out);
    assert_eq!(text.matches("短名冲突").count(), 2, "{text}");

    // 短名寻址被拒（退出码 2，提示冲突与解除方式）
    let out = f.cli(&["hello", "dev"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("冲突"), "{}", stderr(&out));
    assert!(stderr(&out).contains(PEER_UUID), "应给出涉及的 UUID: {}", stderr(&out));

    // UUID 寻址绕过冲突：以服务端 UUID 走通 hello
    let out = f.cli(&["hello", &f.server_uuid]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    f.stop();
}

#[test]
fn token_rejected_exit_code_and_hint() {
    let f = Fixture::start();
    f.write_cli_config(&f.default_peers("deadbeef"));
    let out = f.cli(&["hello", "dev-a"]);
    assert_eq!(code(&out), 4, "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("token"), "{}", stderr(&out));
    f.stop();
}

#[test]
fn unknown_device_and_missing_token_are_config_errors() {
    let f = Fixture::start();
    f.write_cli_config(&f.default_peers(&f.server_token));
    let out = f.cli(&["hello", "nope"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("找不到设备"), "{}", stderr(&out));

    // token 为空的设备
    let peers = format!(
        "[[peer]]\nuuid = \"{PEER_UUID}\"\nshort_name = \"empty\"\naddress = \"10.0.0.9\"\ntoken = \"\"\n"
    );
    f.write_cli_config(&peers);
    let out = f.cli(&["hello", "empty"]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("未配置 token"), "{}", stderr(&out));
    f.stop();
}

#[test]
fn business_error_maps_to_exit_1() {
    let f = Fixture::start();
    f.write_cli_config(&f.default_peers(&f.server_token));
    let out = f.cli(&["download", "dev-a", "nope.bin"]);
    assert_eq!(code(&out), 1, "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("路径不存在"), "{}", stderr(&out));
    f.stop();
}

#[test]
fn network_error_maps_to_exit_3() {
    let f = Fixture::start();
    // 指向未监听的端口
    let peers = format!(
        "[[peer]]\nuuid = \"{PEER_UUID}\"\nshort_name = \"dead\"\naddress = \"127.0.0.1\"\nport = 1\ntoken = \"x\"\n"
    );
    f.write_cli_config(&peers);
    let out = f.cli(&["hello", "dead"]);
    assert_eq!(code(&out), 3, "stderr: {}", stderr(&out));
    f.stop();
}

#[test]
fn token_show_and_reset() {
    let f = Fixture::start();
    f.write_cli_config(&f.default_peers(&f.server_token));

    let out = f.cli(&["token", "show"]);
    assert_eq!(code(&out), 0);
    assert_eq!(stdout(&out).trim(), "c".repeat(64));

    let out = f.cli(&["token", "reset"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let new_token = stdout(&out).trim().to_string();
    assert_eq!(new_token.len(), 64);
    assert_ne!(new_token, "c".repeat(64));

    let out = f.cli(&["token", "show"]);
    assert_eq!(stdout(&out).trim(), new_token);

    // 重置不影响对端连通性（以服务端真实 token 继续可用）
    let peers = f.default_peers(&f.server_token);
    f.write_cli_config(&peers);
    let out = f.cli(&["hello", "dev-a"]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    f.stop();
}
