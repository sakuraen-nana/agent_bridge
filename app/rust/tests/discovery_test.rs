//! 发现信标：编解码、自过滤、过期与定向广播（design D1）。

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use agent_bridge::discovery::{
    Beacon, DiscoveryTable, PROTOCOL, broadcast_targets, directed_broadcast, parse_beacon,
};

fn beacon(uuid: &str) -> Beacon {
    Beacon {
        proto: PROTOCOL.to_string(),
        uuid: uuid.to_string(),
        short_name: Some("dev-a".to_string()),
        hostname: "host-1".to_string(),
        port: 37777,
    }
}

#[test]
fn beacon_roundtrip_and_no_credentials() {
    let payload = serde_json::to_vec(&beacon("uuid-1")).unwrap();
    let text = String::from_utf8(payload.clone()).unwrap();
    assert!(!text.to_lowercase().contains("token"), "信标不得含凭据：{text}");
    let parsed = parse_beacon(&payload).unwrap();
    assert_eq!(parsed.proto, PROTOCOL);
    assert_eq!(parsed.uuid, "uuid-1");
    assert_eq!(parsed.short_name.as_deref(), Some("dev-a"));
    assert_eq!(parsed.port, 37777);
}

#[test]
fn parse_rejects_foreign_or_malformed() {
    assert!(parse_beacon(b"not json").is_none());
    let foreign = serde_json::to_vec(&Beacon {
        proto: "other/9".to_string(),
        uuid: "u".to_string(),
        short_name: None,
        hostname: String::new(),
        port: 1,
    })
    .unwrap();
    assert!(parse_beacon(&foreign).is_none());
}

#[test]
fn self_beacon_ignored_and_expiry_prunes() {
    let mut table = DiscoveryTable::new(Duration::from_secs(30));
    let now = Instant::now();

    table.observe("me", &beacon("me"), "10.0.0.1".to_string(), now);
    assert!(table.list(now).is_empty(), "本机自播应被忽略");

    table.observe("me", &beacon("peer-1"), "10.0.0.2".to_string(), now);
    assert_eq!(table.list(now).len(), 1);

    // 29 秒仍在，30 秒后过期
    let nearly = now + Duration::from_secs(29);
    assert_eq!(table.list(nearly).len(), 1);
    let expired = now + Duration::from_secs(31);
    assert!(table.list(expired).is_empty(), "30 秒未再见应过期");

    // 刷新后重新计时
    table.observe("me", &beacon("peer-1"), "10.0.0.2".to_string(), nearly);
    assert_eq!(table.list(expired).len(), 1, "新信标应刷新过期窗口");
}

#[test]
fn directed_broadcast_computation() {
    assert_eq!(
        directed_broadcast(Ipv4Addr::new(192, 168, 31, 213), 24),
        Some(Ipv4Addr::new(192, 168, 31, 255))
    );
    assert_eq!(
        directed_broadcast(Ipv4Addr::new(10, 1, 2, 3), 8),
        Some(Ipv4Addr::new(10, 255, 255, 255))
    );
    assert_eq!(directed_broadcast(Ipv4Addr::new(10, 1, 2, 3), 0), None);
    assert_eq!(
        directed_broadcast(Ipv4Addr::new(10, 1, 2, 3), 32),
        Some(Ipv4Addr::new(10, 1, 2, 3))
    );
}

#[test]
fn bind_fails_when_port_held_without_reuse() {
    // 先以「不复用」方式占住 37778 → 发现端口绑定应明确失败（主服务不受此测试影响）
    let squatter = match std::net::UdpSocket::bind(("0.0.0.0", 37778)) {
        Ok(sock) => sock,
        Err(_) => return, // 端口已被并发用例占用：跳过（真机覆盖在 §8.6）
    };
    let result = agent_bridge::discovery::bind_socket();
    assert!(result.is_err(), "被无复用占用时绑定应失败");
    drop(squatter);
}

#[test]
fn broadcast_targets_skip_virtual_interfaces() {
    let interfaces = vec![
        ("docker0".to_string(), "172.17.0.1".to_string(), 16u8),
        ("br-a1b2".to_string(), "172.18.0.1".to_string(), 16),
        ("ens33".to_string(), "192.168.1.213".to_string(), 24),
    ];
    let targets = broadcast_targets(&interfaces);
    assert!(targets.contains(&Ipv4Addr::BROADCAST));
    assert!(targets.contains(&Ipv4Addr::new(192, 168, 1, 255)));
    assert!(!targets.iter().any(|t| t.octets()[0] == 172), "虚拟接口不应产生定向广播");
}
