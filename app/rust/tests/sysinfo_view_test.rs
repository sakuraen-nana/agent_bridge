//! 系统信息采集的基本健壮性（与系统工具的一致性由 Linux 实跑抽查覆盖）。

#[test]
fn collect_returns_sane_values() {
    let s = agent_bridge::sysinfo_view::collect();
    assert!(!s.platform.is_empty(), "平台信息不应为空");
    assert!(!s.cpu.is_empty(), "CPU 信息不应为空");
    assert!(s.memory.contains("总计"), "内存应为格式化文本");
    assert!(s.memory.contains("可用"));
    assert!(!s.local_time.is_empty(), "本地时间不应为空");
    for ip in &s.ip_addresses {
        assert!(!ip.contains("127.0.0.1"), "不应包含环回地址");
    }
}
