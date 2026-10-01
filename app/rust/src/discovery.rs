//! 局域网发现信标（design D1）。
//!
//! 固定 UDP 端口 37778：每 3 秒广播 JSON 信标（无任何凭据）并同时监听；
//! 收到的对端记入「最近活跃设备」表（30 秒过期，含来源 IP）；本机自播忽略。
//! UDP 绑定失败不阻断主服务（状态经 [`crate::server::ServerState`] 呈现）。

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use crate::error::AppError;
use crate::server::ServerState;

/// 发现信标端口（固定）。
pub const DISCOVERY_PORT: u16 = 37778;
/// 协议标识（信标首字段）。
pub const PROTOCOL: &str = "agent-bridge/1";
/// 广播周期。
pub const BEACON_INTERVAL: Duration = Duration::from_secs(3);
/// 条目过期时长。
pub const EXPIRY: Duration = Duration::from_secs(30);

/// 信标载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Beacon {
    pub proto: String,
    pub uuid: String,
    #[serde(default)]
    pub short_name: Option<String>,
    #[serde(default)]
    pub hostname: String,
    pub port: u16,
}

/// 最近活跃设备条目。
#[derive(Debug, Clone)]
pub struct Discovered {
    pub uuid: String,
    pub short_name: Option<String>,
    pub hostname: String,
    pub port: u16,
    /// 信标来源 IP（配对写入 [[peer]] 时用作 address）。
    pub source_ip: String,
    pub last_seen: Instant,
}

/// 最近活跃设备表（时钟注入便于单测）。
pub struct DiscoveryTable {
    entries: HashMap<String, Discovered>,
    expiry: Duration,
}

impl Default for DiscoveryTable {
    fn default() -> Self {
        Self::new(EXPIRY)
    }
}

impl DiscoveryTable {
    pub fn new(expiry: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            expiry,
        }
    }

    /// 记录一条信标（self_uuid 的自播忽略）。
    pub fn observe(&mut self, self_uuid: &str, beacon: &Beacon, source_ip: String, now: Instant) {
        if beacon.uuid == self_uuid {
            return;
        }
        self.entries.insert(
            beacon.uuid.clone(),
            Discovered {
                uuid: beacon.uuid.clone(),
                short_name: beacon.short_name.clone(),
                hostname: beacon.hostname.clone(),
                port: beacon.port,
                source_ip,
                last_seen: now,
            },
        );
    }

    /// 当前列表（按 UUID 排序，稳定输出）。
    pub fn list(&self, now: Instant) -> Vec<Discovered> {
        let mut out: Vec<Discovered> = self
            .entries
            .values()
            .filter(|entry| now.duration_since(entry.last_seen) < self.expiry)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.uuid.cmp(&b.uuid));
        out
    }

    /// 指定 uuid 的条目（含过期清理）。
    pub fn get(&self, uuid: &str, now: Instant) -> Option<Discovered> {
        self.entries
            .get(uuid)
            .filter(|entry| now.duration_since(entry.last_seen) < self.expiry)
            .cloned()
    }
}

/// 构造本机信标（无凭据）。
pub fn local_beacon(uuid: &str, short_name: Option<String>, port: u16) -> Beacon {
    Beacon {
        proto: PROTOCOL.to_string(),
        uuid: uuid.to_string(),
        short_name,
        hostname: sysinfo::System::host_name().unwrap_or_default(),
        port,
    }
}

/// 解析对端信标（异协议/非法 JSON 返回 None）。
pub fn parse_beacon(bytes: &[u8]) -> Option<Beacon> {
    let beacon: Beacon = serde_json::from_slice(bytes).ok()?;
    if beacon.proto != PROTOCOL || beacon.uuid.is_empty() {
        return None;
    }
    Some(beacon)
}

/// 绑定发现端口（SO_REUSEADDR/SO_REUSEPORT，便于同机多实例与自测）。
pub fn bind_socket() -> Result<std::net::UdpSocket, AppError> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
        .map_err(|e| AppError::Server(format!("创建 UDP socket 失败：{e}")))?;
    socket
        .set_reuse_address(true)
        .map_err(|e| AppError::Server(e.to_string()))?;
    #[cfg(unix)]
    socket
        .set_reuse_port(true)
        .map_err(|e| AppError::Server(e.to_string()))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| AppError::Server(e.to_string()))?;
    socket
        .bind(&SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DISCOVERY_PORT).into())
        .map_err(|e| {
            AppError::Server(format!(
                "UDP 端口 {DISCOVERY_PORT} 绑定失败（发现功能不可用）：{e}"
            ))
        })?;
    socket
        .set_broadcast(true)
        .map_err(|e| AppError::Server(e.to_string()))?;
    Ok(socket.into())
}

/// 广播目标集合：全局广播 + 各非虚拟私网接口的定向广播地址。
pub fn broadcast_targets(interfaces: &[(String, String, u8)]) -> Vec<Ipv4Addr> {
    const VIRTUAL_PREFIXES: [&str; 10] = [
        "lo", "docker", "br-", "veth", "virbr", "tailscale", "tun", "wg", "zt", "vmnet",
    ];
    let mut targets = vec![Ipv4Addr::BROADCAST];
    for (iface, addr, prefix) in interfaces {
        if VIRTUAL_PREFIXES.iter().any(|p| iface.starts_with(p)) {
            continue;
        }
        if let Ok(ip) = addr.parse::<Ipv4Addr>() {
            if let Some(broadcast) = directed_broadcast(ip, *prefix) {
                if !targets.contains(&broadcast) {
                    targets.push(broadcast);
                }
            }
        }
    }
    targets
}

/// 由地址与前缀计算定向广播地址（prefix 0 或 ≥32 时返回 None 或自身）。
pub fn directed_broadcast(ip: Ipv4Addr, prefix: u8) -> Option<Ipv4Addr> {
    if prefix == 0 || prefix > 32 {
        return None;
    }
    if prefix == 32 {
        return Some(ip);
    }
    let mask = u32::MAX << (32 - prefix);
    let network = u32::from(ip) & mask;
    Some(Ipv4Addr::from(network | !mask))
}

/// 在服务端 runtime 中运行广播 + 监听循环（socket 已绑定）。
pub async fn run(state: Arc<ServerState>, std_socket: std::net::UdpSocket) {
    let socket = match UdpSocket::from_std(std_socket) {
        Ok(socket) => Arc::new(socket),
        Err(_) => return,
    };

    // 接收循环（自播按 uuid 过滤）
    let recv_socket = socket.clone();
    let recv_state = state.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            match recv_socket.recv_from(&mut buf).await {
                Ok((n, from)) => {
                    if let Some(beacon) = parse_beacon(&buf[..n]) {
                        let (self_uuid, _) = recv_state.device_snapshot();
                        let mut table = recv_state.discovery_table.lock().unwrap();
                        table.observe(&self_uuid, &beacon, from.ip().to_string(), Instant::now());
                    }
                }
                Err(_) => break,
            }
        }
    });

    // 广播循环（全局广播 + 各网段定向广播）
    let send_state = state.clone();
    tokio::spawn(async move {
        loop {
            let (uuid, short_name) = send_state.device_snapshot();
            let beacon = local_beacon(&uuid, short_name, send_state.port);
            if let Ok(payload) = serde_json::to_vec(&beacon) {
                let interfaces = crate::sysinfo_view::interface_ipv4s_with_prefix();
                for target in broadcast_targets(&interfaces) {
                    let _ = socket
                        .send_to(&payload, SocketAddr::new(IpAddr::V4(target), DISCOVERY_PORT))
                        .await;
                }
            }
            tokio::time::sleep(BEACON_INTERVAL).await;
        }
    });
}
