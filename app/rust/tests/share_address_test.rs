//! 配置片段地址选择（design D6）。

use agent_bridge::api::init::select_share_address;

fn pair(iface: &str, addr: &str) -> (String, String) {
    (iface.to_string(), addr.to_string())
}

#[test]
fn skips_virtual_interfaces_and_prefers_private() {
    let candidates = vec![
        pair("lo", "127.0.0.1"),
        pair("docker0", "172.17.0.1"),
        pair("br-a1b2", "172.18.0.1"),
        pair("ens33", "8.8.8.8"),
        pair("ens33", "192.168.1.213"),
    ];
    assert_eq!(
        select_share_address(&candidates).as_deref(),
        Some("192.168.1.213")
    );
}

#[test]
fn private_ranges_recognized() {
    assert_eq!(
        select_share_address(&[pair("eth0", "10.1.2.3")]).as_deref(),
        Some("10.1.2.3")
    );
    assert_eq!(
        select_share_address(&[pair("eth0", "172.16.0.9")]).as_deref(),
        Some("172.16.0.9")
    );
    assert_eq!(
        select_share_address(&[pair("eth0", "172.32.0.9")]).as_deref(),
        Some("172.32.0.9") // 172.32 不在 16–31，非私网，但仍是唯一候选
    );
}

#[test]
fn falls_back_to_public_when_no_private() {
    let candidates = vec![pair("ens33", "203.0.113.7"), pair("ens34", "8.8.4.4")];
    assert_eq!(
        select_share_address(&candidates).as_deref(),
        Some("203.0.113.7")
    );
}

#[test]
fn none_when_all_virtual() {
    let candidates = vec![pair("docker0", "172.17.0.1"), pair("veth1", "10.0.0.2")];
    assert_eq!(select_share_address(&candidates), None);
}
