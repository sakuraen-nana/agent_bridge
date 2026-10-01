//! 配对全环（design D3）：请求方（独立 ServerState + 注入发现条目）× 目标端
//! （进程内服务端）；同意后请求方配置写入 `[[peer]]`，并以 CLI 二进制连通目标。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_bridge::config;
use agent_bridge::discovery::{Beacon, PROTOCOL};
use agent_bridge::server::{self, ServerConfig, ServerState};

struct Target {
    _dir: tempfile::TempDir,
    handle: Option<server::ServerHandle>,
    dir: PathBuf,
    port: u16,
    uuid: String,
    token: String,
}

fn start_target(pair_timeout_secs: u64) -> Target {
    let dir = tempfile::tempdir().unwrap();
    let dir_path = dir.path().to_path_buf();
    let outcome = config::load_or_create(&dir_path).unwrap();
    let mut server_config = ServerConfig::new(
        0,
        dir_path.clone(),
        "s".repeat(64),
        Some(dir_path.to_string_lossy().into_owned()),
    );
    server_config.pair_timeout_secs = pair_timeout_secs;
    let handle = server::start(server_config).unwrap();
    let port = handle.port();
    Target {
        _dir: dir,
        handle: Some(handle),
        dir: dir_path,
        port,
        uuid: outcome.device.uuid,
        token: outcome.device.long_term_token,
    }
}

impl Target {
    fn state(&self) -> Arc<ServerState> {
        self.handle.as_ref().unwrap().state()
    }

    fn stop(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
    }
}

fn wait_pending(state: &Arc<ServerState>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while agent_bridge::pairing::pending_info(state).is_none() {
        assert!(Instant::now() < deadline, "配对请求应在 5 秒内进入待决表");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// 构造请求方现场：独立数据目录 + 独立 ServerState + 注入「发现到目标」的条目。
fn requester_fixture(target: &Target) -> (tempfile::TempDir, PathBuf, Arc<ServerState>) {
    let home = tempfile::tempdir().unwrap();
    let data_dir = home.path().join("agent-bridge");
    let _ = config::load_or_create(&data_dir).unwrap();
    let state = Arc::new(
        ServerState::new(
            &ServerConfig::new(0, data_dir.clone(), "s".repeat(64), None),
            40000,
        )
        .unwrap(),
    );
    state.discovery_table.lock().unwrap().observe(
        "self-uuid",
        &Beacon {
            proto: PROTOCOL.to_string(),
            uuid: target.uuid.clone(),
            short_name: Some("target".to_string()),
            hostname: "target-host".to_string(),
            port: target.port,
        },
        "127.0.0.1".to_string(),
        Instant::now(),
    );
    (home, data_dir, state)
}

#[test]
fn full_pairing_loop_writes_peer_and_cli_connects() {
    let target = start_target(120);
    let (home, data_dir, requester_state) = requester_fixture(&target);

    let state_for_req = requester_state.clone();
    let dir_for_req = data_dir.clone();
    let target_uuid = target.uuid.clone();
    let requester = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(agent_bridge::api::pair::request_pairing_core(
            &state_for_req,
            &dir_for_req,
            &target_uuid,
        ))
    });

    wait_pending(&target.state());
    agent_bridge::pairing::decide(&target.state(), true).unwrap();

    let outcome = requester.join().unwrap().unwrap();
    assert_eq!(outcome.status, "paired", "{}", outcome.detail);

    let peers = config::load_or_create(&data_dir).unwrap().peers;
    assert_eq!(peers.len(), 1, "应自动写入一条 [[peer]]");
    assert_eq!(peers[0].uuid, target.uuid);
    assert_eq!(peers[0].token, target.token, "应拿到对端长期 token");
    assert_eq!(peers[0].address, "127.0.0.1", "地址应取发现来源 IP");
    assert_eq!(peers[0].port, target.port);

    // 以该配置跑真实 CLI：应能连通目标
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_agent-bridge"))
        .args(["hello", &target.uuid])
        .env("XDG_CONFIG_HOME", home.path())
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "CLI 应连通：stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains(&target.uuid));
    target.stop();
}

#[test]
fn pairing_rejected_writes_nothing() {
    let target = start_target(120);
    let (_home, data_dir, requester_state) = requester_fixture(&target);

    let state_for_req = requester_state.clone();
    let dir_for_req = data_dir.clone();
    let target_uuid = target.uuid.clone();
    let requester = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(agent_bridge::api::pair::request_pairing_core(
            &state_for_req,
            &dir_for_req,
            &target_uuid,
        ))
    });

    wait_pending(&target.state());
    agent_bridge::pairing::decide(&target.state(), false).unwrap();

    let outcome = requester.join().unwrap().unwrap();
    assert_eq!(outcome.status, "rejected");
    assert!(
        config::load_or_create(&data_dir).unwrap().peers.is_empty(),
        "拒绝时不应写入配置"
    );
    target.stop();
}

#[test]
fn pairing_timeout_clears_and_writes_nothing() {
    let target = start_target(1);
    let (_home, data_dir, requester_state) = requester_fixture(&target);

    let state_for_req = requester_state.clone();
    let dir_for_req = data_dir.clone();
    let target_uuid = target.uuid.clone();
    let requester = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(agent_bridge::api::pair::request_pairing_core(
            &state_for_req,
            &dir_for_req,
            &target_uuid,
        ))
    });

    let outcome = requester.join().unwrap().unwrap();
    assert_eq!(outcome.status, "timeout", "{}", outcome.detail);
    assert!(
        config::load_or_create(&data_dir).unwrap().peers.is_empty(),
        "超时不应写入配置"
    );
    assert!(
        agent_bridge::pairing::pending_info(&target.state()).is_none(),
        "超时后目标端应清表"
    );
    target.stop();
}
