//! 防火墙探测与放行全分支（design D3；命令执行注入回放）。

use std::collections::HashMap;

use agent_bridge::firewall::{CommandRunner, ensure, ensure_windows};

struct MockRunner {
    responses: HashMap<String, Result<(i32, String), String>>,
}

impl MockRunner {
    fn new(pairs: Vec<(&str, Result<(i32, String), String>)>) -> Self {
        let responses = pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        Self { responses }
    }
}

impl CommandRunner for MockRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<(i32, String), String> {
        let key = format!("{program} {}", args.join(" "));
        self.responses
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Err(format!("command not found: {key}")))
    }
}

fn ok(text: &str) -> Result<(i32, String), String> {
    Ok((0, text.to_string()))
}

#[test]
fn ufw_active_adds_rule() {
    let runner = MockRunner::new(vec![
        ("ufw status", ok("Status: active\nTo Action From\n")),
        ("ufw allow 37777/tcp", ok("Rule added")),
    ]);
    let report = ensure(37777, &runner);
    assert_eq!(report.manager.as_deref(), Some("ufw"));
    assert!(report.active && report.applied);
    assert!(report.detail.contains("已放行 37777/tcp"));
}

#[test]
fn ufw_active_skips_when_already_allowed() {
    let runner = MockRunner::new(vec![(
        "ufw status",
        ok("Status: active\n37777/tcp ALLOW Anywhere\n"),
    )]);
    let report = ensure(37777, &runner);
    assert!(report.active && !report.applied);
    assert!(report.detail.contains("跳过重复添加"));
}

#[test]
fn ufw_inactive_is_skipped() {
    let runner = MockRunner::new(vec![("ufw status", ok("Status: inactive"))]);
    let report = ensure(37777, &runner);
    assert_eq!(report.manager.as_deref(), Some("ufw"));
    assert!(!report.active && !report.applied);
    assert!(report.detail.contains("未激活"));
}

#[test]
fn ufw_permission_denied_reported() {
    let runner = MockRunner::new(vec![(
        "ufw status",
        Ok((1, "ERROR: You need to be root to run this script".to_string())),
    )]);
    let report = ensure(37777, &runner);
    assert!(!report.applied);
    assert!(
        report.detail.contains("手动放行") || report.detail.contains("受限"),
        "{}",
        report.detail
    );
}

#[test]
fn firewalld_active_adds_and_reloads() {
    let runner = MockRunner::new(vec![
        ("ufw status", Err("not found".to_string())),
        ("firewall-cmd --state", ok("running")),
        ("firewall-cmd --query-port=37777/tcp", Err("not found".to_string())),
        ("firewall-cmd --permanent --add-port=37777/tcp", ok("success")),
        ("firewall-cmd --reload", ok("success")),
    ]);
    let report = ensure(37777, &runner);
    assert_eq!(report.manager.as_deref(), Some("firewalld"));
    assert!(report.active && report.applied);
}

#[test]
fn firewalld_query_yes_skips() {
    let runner = MockRunner::new(vec![
        ("ufw status", Err("not found".to_string())),
        ("firewall-cmd --state", ok("running")),
        ("firewall-cmd --query-port=37777/tcp", ok("yes")),
    ]);
    let report = ensure(37777, &runner);
    assert!(report.active && !report.applied);
    assert!(report.detail.contains("跳过重复添加"));
}

#[test]
fn nothing_supported_reports_manual_hint() {
    let runner = MockRunner::new(vec![]);
    let report = ensure(37777, &runner);
    assert!(report.manager.is_none() && !report.active && !report.applied);
    assert!(report.detail.contains("手动放行 37777/tcp"), "{}", report.detail);
}

#[test]
fn windows_defender_active_adds_rule() {
    let runner = MockRunner::new(vec![
        (
            "netsh advfirewall show allprofiles state",
            ok("Domain Profile Settings:\nState ON\n"),
        ),
        (
            "netsh advfirewall firewall show rule name=agent-bridge",
            Err("not found".to_string()),
        ),
        (
            "netsh advfirewall firewall add rule name=agent-bridge dir=in action=allow protocol=TCP localport=37777",
            ok("Ok."),
        ),
    ]);
    let report = ensure_windows(37777, &runner);
    assert_eq!(report.manager.as_deref(), Some("Windows Defender"));
    assert!(report.active && report.applied);
}

#[test]
fn windows_defender_rule_exists_skips() {
    let runner = MockRunner::new(vec![
        ("netsh advfirewall show allprofiles state", ok("State ON")),
        (
            "netsh advfirewall firewall show rule name=agent-bridge",
            ok("Rule name: agent-bridge"),
        ),
    ]);
    let report = ensure_windows(37777, &runner);
    assert!(report.active && !report.applied);
    assert!(report.detail.contains("已存在"));
}

#[test]
fn tray_host_probe_prefers_gdbus_then_dbus_send_then_false() {
    use agent_bridge::firewall::tray_host_available_with;

    // gdbus 可用且宿主存在
    let gdbus_yes = MockRunner::new(vec![(
        "gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner org.kde.StatusNotifierWatcher",
        ok("(true,)"),
    )]);
    assert!(tray_host_available_with(&gdbus_yes));

    // gdbus 报无宿主
    let gdbus_no = MockRunner::new(vec![(
        "gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner org.kde.StatusNotifierWatcher",
        ok("(false,)"),
    )]);
    assert!(!tray_host_available_with(&gdbus_no));

    // gdbus 缺失 → 回退 dbus-send
    let dbus_send = MockRunner::new(vec![(
        "dbus-send --session --dest=org.freedesktop.DBus --type=method_call --print-reply /org/freedesktop/DBus org.freedesktop.DBus.NameHasOwner string:org.kde.StatusNotifierWatcher",
        ok("boolean true"),
    )]);
    assert!(tray_host_available_with(&dbus_send));

    // 两者皆缺 → 按不可用处理（宁降级不产生幽灵进程）
    let none = MockRunner::new(vec![]);
    assert!(!tray_host_available_with(&none));
}

#[test]
fn windows_defender_off_skips() {
    let runner = MockRunner::new(vec![(
        "netsh advfirewall show allprofiles state",
        ok("State OFF"),
    )]);
    let report = ensure_windows(37777, &runner);
    assert!(!report.active && !report.applied);
    assert!(report.detail.contains("未开启"));
}
