//! 桥接面：发现、配对与在线状态（design D5）。

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use futures_util::future::join_all;
use serde_json::json;

use super::init::app_runtime_ref;
use crate::config::{self, Peer};
use crate::identity::short_name_compare_key;
use crate::pairing;
use crate::peers::is_conflicted_key;

/// 发现功能状态（进入启动快照）。
#[derive(Debug, Clone)]
pub struct DiscoverySnapshot {
    /// UDP 信标监听是否可用。
    pub available: bool,
    /// 可用说明 / 不可用原因。
    pub detail: String,
}

/// 发现到的设备（界面列表用）。
#[derive(Debug, Clone)]
pub struct DiscoveredDeviceInfo {
    pub uuid: String,
    pub short_name: Option<String>,
    pub hostname: String,
    pub port: u16,
    /// 信标来源 IP（配对写入 address 时采用）。
    pub source_ip: String,
    /// 是否已配对（配置中已有同 uuid 条目）。
    pub paired: bool,
    /// 短名在发现集合内是否冲突（冲突则短名无效）。
    pub conflicted: bool,
}

/// 待决配对请求（界面弹窗用）。
#[derive(Debug, Clone)]
pub struct PendingPairingInfo {
    pub id: u64,
    pub uuid: String,
    pub short_name: Option<String>,
    pub port: u16,
    pub source_ip: String,
}

/// 配对结果。
#[derive(Debug, Clone)]
pub struct PairOutcomeInfo {
    /// "paired" | "rejected" | "timeout" | "busy" | "not_found" | "unreachable"
    pub status: String,
    pub detail: String,
}

/// 已配对设备状态。
#[derive(Debug, Clone)]
pub struct PeerStatusInfo {
    pub uuid: String,
    pub short_name: Option<String>,
    pub address: String,
    pub port: u16,
    /// "online" | "unauthorized" | "offline"
    pub status: String,
    pub note: String,
    pub conflicted: bool,
}

fn current_state() -> Option<std::sync::Arc<crate::server::ServerState>> {
    let runtime = app_runtime_ref();
    let guard = runtime.server.lock().unwrap();
    guard.as_ref().map(|handle| handle.state())
}

/// 发现到的设备列表（含已配对与冲突标记）。
pub async fn discovered_devices() -> anyhow::Result<Vec<DiscoveredDeviceInfo>> {
    let data_dir = config::resolve_data_dir()?;
    let peers = config::load_or_create(&data_dir)?.peers;
    let paired: HashSet<String> = peers.iter().map(|peer| peer.uuid.clone()).collect();
    let Some(state) = current_state() else {
        return Ok(Vec::new());
    };
    let list = state.discovery_table.lock().unwrap().list(Instant::now());

    let mut key_counts: HashMap<String, usize> = HashMap::new();
    for discovered in &list {
        if let Some(name) = discovered.short_name.as_deref() {
            *key_counts
                .entry(short_name_compare_key(name))
                .or_default() += 1;
        }
    }
    Ok(list
        .into_iter()
        .map(|discovered| {
            let conflicted = discovered
                .short_name
                .as_deref()
                .map(|name| {
                    key_counts
                        .get(&short_name_compare_key(name))
                        .copied()
                        .unwrap_or(0)
                        > 1
                })
                .unwrap_or(false);
            DiscoveredDeviceInfo {
                paired: paired.contains(&discovered.uuid),
                uuid: discovered.uuid,
                short_name: discovered.short_name,
                hostname: discovered.hostname,
                port: discovered.port,
                source_ip: discovered.source_ip,
                conflicted,
            }
        })
        .collect())
}

/// 当前待决配对请求（界面 1 秒轮询）。
pub async fn pairing_pending() -> Option<PendingPairingInfo> {
    let state = current_state()?;
    pairing::pending_info(&state).map(|pending| PendingPairingInfo {
        id: pending.id,
        uuid: pending.uuid,
        short_name: pending.short_name,
        port: pending.port,
        source_ip: pending.source_ip,
    })
}

/// 对当前待决请求做出决定（同意 / 拒绝）。
pub async fn respond_pairing(approve: bool) -> anyhow::Result<()> {
    let state = current_state().ok_or_else(|| anyhow::anyhow!("服务端未运行"))?;
    pairing::decide(&state, approve).map_err(|e| anyhow::anyhow!(e))
}

/// 向发现到的设备发起配对；同意后自动写入 `[[peer]]`。
pub async fn request_pairing(uuid: String) -> anyhow::Result<PairOutcomeInfo> {
    let data_dir = config::resolve_data_dir()?;
    let state = current_state().ok_or_else(|| anyhow::anyhow!("服务端未运行"))?;
    request_pairing_core(&state, &data_dir, &uuid).await
}

/// 配对请求核心（显式状态与数据目录，便于 Rust 级集成测试复用）。
///
/// `#[frb(ignore)]`：仅供 Rust 内部与集成测试调用，不暴露到桥接面
/// （参数含服务端内部类型，GUI 无用途）。
#[flutter_rust_bridge::frb(ignore)]
pub async fn request_pairing_core(
    state: &crate::server::ServerState,
    data_dir: &std::path::Path,
    uuid: &str,
) -> anyhow::Result<PairOutcomeInfo> {
    let uuid = uuid.to_string();
    let discovered = {
        let table = state.discovery_table.lock().unwrap();
        table.get(&uuid, Instant::now())
    };
    let Some(discovered) = discovered else {
        return Ok(PairOutcomeInfo {
            status: "not_found".to_string(),
            detail: "未发现该设备（信标已过期或不在同一网段）".to_string(),
        });
    };

    let (self_uuid, self_short_name) = state.device_snapshot();
    let body = json!({
        "uuid": self_uuid,
        "short_name": self_short_name,
        "port": state.port,
    });
    let url = format!(
        "http://{}:{}/pair/request",
        discovered.source_ip, discovered.port
    );
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    let send = client.post(&url).json(&body).send();
    let response = match tokio::time::timeout(
        Duration::from_secs(state.pair_timeout_secs + 5),
        send,
    )
    .await
    {
        Err(_) => {
            return Ok(PairOutcomeInfo {
                status: "timeout".to_string(),
                detail: "等待对端决定超时".to_string(),
            });
        }
        Ok(Err(e)) => {
            return Ok(PairOutcomeInfo {
                status: "unreachable".to_string(),
                detail: format!(
                    "无法连接对端：{}",
                    crate::cli::client::sanitize(&e.to_string())
                ),
            });
        }
        Ok(Ok(response)) => response,
    };

    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Ok(PairOutcomeInfo {
            status: "busy".to_string(),
            detail: "对端已有待处理的配对请求".to_string(),
        });
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("解析对端响应失败：{e}"))?;
    if value.get("approved").and_then(|a| a.as_bool()) != Some(true) {
        let reason = value
            .get("reason")
            .and_then(|r| r.as_str())
            .unwrap_or("被拒绝")
            .to_string();
        let status = if reason.contains("超时") {
            "timeout"
        } else {
            "rejected"
        };
        return Ok(PairOutcomeInfo {
            status: status.to_string(),
            detail: reason,
        });
    }

    let config_obj = value
        .get("config")
        .ok_or_else(|| anyhow::anyhow!("对端响应缺少 config"))?;
    let peer = Peer {
        uuid: config_obj
            .get("uuid")
            .and_then(|v| v.as_str())
            .unwrap_or(&uuid)
            .to_string(),
        short_name: config_obj
            .get("short_name")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        // 地址采用发现来源 IP（防止多网卡下对端自报地址不可达）
        address: discovered.source_ip.clone(),
        port: config_obj
            .get("port")
            .and_then(|v| v.as_u64())
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(discovered.port),
        token: config_obj
            .get("token")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    };
    if peer.token.is_empty() {
        return Ok(PairOutcomeInfo {
            status: "rejected".to_string(),
            detail: "对端未返回 token".to_string(),
        });
    }
    config::add_peer(&data_dir, &peer)?;
    Ok(PairOutcomeInfo {
        status: "paired".to_string(),
        detail: format!(
            "已配对：{}",
            peer.short_name
                .clone()
                .unwrap_or_else(|| peer.uuid.clone())
        ),
    })
}

/// 已配对设备的在线状态（界面 5 秒轮询）。
pub async fn peers_status() -> anyhow::Result<Vec<PeerStatusInfo>> {
    let data_dir = config::resolve_data_dir()?;
    let peers = config::load_or_create(&data_dir)?.peers;
    let results = join_all(peers.iter().map(|peer| probe_peer(peer, &peers))).await;
    Ok(results)
}

async fn probe_peer(peer: &Peer, all: &[Peer]) -> PeerStatusInfo {
    let conflicted = peer
        .short_name
        .as_deref()
        .map(|name| is_conflicted_key(all, &short_name_compare_key(name)))
        .unwrap_or(false);
    let mut info = PeerStatusInfo {
        uuid: peer.uuid.clone(),
        short_name: peer.short_name.clone(),
        address: peer.address.clone(),
        port: peer.port,
        status: "offline".to_string(),
        note: String::new(),
        conflicted,
    };
    if peer.token.trim().is_empty() {
        info.note = "未配置 token（建议重新配对）".to_string();
        return info;
    }
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            info.note = e.to_string();
            return info;
        }
    };
    let url = format!(
        "http://{}:{}/hello?token={}",
        peer.address, peer.port, peer.token
    );
    match client.post(url).send().await {
        Ok(response) if response.status().is_success() => {
            info.status = "online".to_string();
        }
        Ok(response) if response.status() == reqwest::StatusCode::NOT_FOUND => {
            info.status = "unauthorized".to_string();
            info.note = "在线但凭据失效（token 被重置或轮换），请重新配对".to_string();
        }
        Ok(response) => {
            info.note = format!("对端返回 {}", response.status());
        }
        Err(e) => {
            info.note = if e.is_timeout() {
                "连接超时".to_string()
            } else {
                format!(
                    "连接失败：{}",
                    crate::cli::client::sanitize(&e.to_string())
                )
            };
        }
    }
    info
}
