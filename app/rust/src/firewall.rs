//! 防火墙检测与幂等放行（design D3 / 变更 ④ D6：TCP 服务端口 + UDP 发现端口）。
//!
//! 命令执行经 [`CommandRunner`] 注入点，全分支可单测；生产实现强制 `LC_ALL=C`
//! 以保证输出解析不受区域影响。只增/查本工具的规则，MUST NOT 改动其它配置。

use std::process::Command;

/// 命令执行注入点：返回（退出码，stdout+stderr 合并文本）。
pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<(i32, String), String>;
}

/// 生产实现：真实执行。
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<(i32, String), String> {
        let output = Command::new(program)
            .args(args)
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .output()
            .map_err(|e| e.to_string())?;
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Ok((output.status.code().unwrap_or(-1), text))
    }
}

/// 探测与放行结果（进入快照 / 面板）。
#[derive(Debug, Clone)]
pub struct FirewallReport {
    /// 命中的管理器（"ufw" / "firewalld" / "Windows Defender"；未检出为 None）。
    pub manager: Option<String>,
    /// 管理器是否活跃。
    pub active: bool,
    /// 本次是否实际添加了放行规则。
    pub applied: bool,
    /// 面向人类的状态说明。
    pub detail: String,
}

/// 单个放行需求。
#[derive(Debug, Clone)]
struct PortNeed {
    proto: &'static str,
    port: u16,
}

fn needs(port: u16, udp_discovery_port: Option<u16>) -> Vec<PortNeed> {
    let mut list = vec![PortNeed {
        proto: "tcp",
        port,
    }];
    if let Some(udp) = udp_discovery_port {
        list.push(PortNeed {
            proto: "udp",
            port: udp,
        });
    }
    list
}

fn needs_text(list: &[PortNeed]) -> String {
    list.iter()
        .map(|need| format!("{} {}", need.proto.to_uppercase(), need.port))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// 生产入口：以系统执行器检测并放行（桥接面的调用点）。
pub fn ensure_system(port: u16, udp_discovery_port: Option<u16>) -> FirewallReport {
    ensure(port, udp_discovery_port, &SystemRunner)
}

/// 检测并（必要时）幂等放行所需端口（TCP 服务端口 + 可选 UDP 发现端口）。
pub fn ensure(port: u16, udp_discovery_port: Option<u16>, runner: &dyn CommandRunner) -> FirewallReport {
    let list = needs(port, udp_discovery_port);
    #[cfg(windows)]
    {
        ensure_windows(&list, runner)
    }
    #[cfg(not(windows))]
    {
        ensure_unix(&list, runner)
    }
}

fn ensure_unix(list: &[PortNeed], runner: &dyn CommandRunner) -> FirewallReport {
    // 1) ufw
    if let Ok((_, out)) = runner.run("ufw", &["status"]) {
        if out.contains("Status: active") {
            let mut applied = Vec::new();
            let mut skipped = Vec::new();
            let mut failure: Option<String> = None;
            for need in list {
                let needle = format!("{}/{}", need.port, need.proto);
                if out.contains(&needle) {
                    skipped.push(needle);
                    continue;
                }
                match runner.run("ufw", &["allow", &needle]) {
                    Ok((0, _)) => applied.push(needle),
                    Ok((code, out)) => {
                        failure = Some(format!("放行 {needle} 失败（退出码 {code}）：{}", first_line(&out)));
                        break;
                    }
                    Err(e) => {
                        failure = Some(format!("放行 {needle} 失败：{e}"));
                        break;
                    }
                }
            }
            let detail = if let Some(failure) = failure {
                failure
            } else if applied.is_empty() {
                format!("已放行（跳过重复添加）：{}", skipped.join("、"))
            } else if skipped.is_empty() {
                format!("已放行：{}", applied.join("、"))
            } else {
                format!("已放行：{}；跳过重复：{}", applied.join("、"), skipped.join("、"))
            };
            return FirewallReport {
                manager: Some("ufw".into()),
                active: true,
                applied: !applied.is_empty(),
                detail,
            };
        }
        if out.contains("Status: inactive") {
            return FirewallReport {
                manager: Some("ufw".into()),
                active: false,
                applied: false,
                detail: format!("ufw 未激活，无需放行（如需手动：ufw allow …；所需端口 {}）", needs_text(&list)),
            };
        }
        if out.to_ascii_lowercase().contains("root") {
            return FirewallReport {
                manager: Some("ufw".into()),
                active: false,
                applied: false,
                detail: format!("无权限执行 ufw（受限模式下请手动放行所需端口 {}）", needs_text(&list)),
            };
        }
    }

    // 2) firewalld
    if let Ok((_, out)) = runner.run("firewall-cmd", &["--state"]) {
        if out.contains("running") {
            let mut applied = Vec::new();
            let mut skipped = Vec::new();
            let mut failure: Option<String> = None;
            for need in list {
                let needle = format!("{}/{}", need.port, need.proto);
                let query = format!("--query-port={needle}");
                if let Ok((0, _)) = runner.run("firewall-cmd", &[&query]) {
                    skipped.push(needle);
                    continue;
                }
                let add = format!("--add-port={needle}");
                match runner.run("firewall-cmd", &["--permanent", &add]) {
                    Ok((0, _)) => {
                        let _ = runner.run("firewall-cmd", &["--reload"]);
                        applied.push(needle);
                    }
                    Ok((code, out)) => {
                        failure = Some(format!("放行 {needle} 失败（退出码 {code}）：{}", first_line(&out)));
                        break;
                    }
                    Err(e) => {
                        failure = Some(format!("放行 {needle} 失败：{e}"));
                        break;
                    }
                }
            }
            let detail = if let Some(failure) = failure {
                failure
            } else if applied.is_empty() {
                format!("已放行（跳过重复添加）：{}", skipped.join("、"))
            } else {
                format!("已放行：{}", applied.join("、"))
            };
            return FirewallReport {
                manager: Some("firewalld".into()),
                active: true,
                applied: !applied.is_empty(),
                detail,
            };
        }
        return FirewallReport {
            manager: Some("firewalld".into()),
            active: false,
            applied: false,
            detail: format!("firewalld 未运行，无需放行（所需端口 {}）", needs_text(&list)),
        };
    }

    FirewallReport {
        manager: None,
        active: false,
        applied: false,
        detail: format!(
            "未检测到受支持的活跃防火墙（ufw/firewalld）；如有自管规则请手动放行所需端口（{}）",
            needs_text(list)
        ),
    }
}

/// Windows Defender 分支（经注入执行器，可单测）。
pub fn ensure_windows(port: u16, udp_discovery_port: Option<u16>, runner: &dyn CommandRunner) -> FirewallReport {
    let list = needs(port, udp_discovery_port);
    let manager = || Some("Windows Defender".to_string());
    match runner.run("netsh", &["advfirewall", "show", "allprofiles", "state"]) {
        Ok((_, out)) => {
            if !out.contains("State") || !out.contains("ON") {
                return FirewallReport {
                    manager: manager(),
                    active: false,
                    applied: false,
                    detail: format!("Windows 防火墙未开启，无需放行（所需端口 {}）", needs_text(&list)),
                };
            }
            let mut applied = Vec::new();
            let mut skipped = Vec::new();
            let mut failure: Option<String> = None;
            for need in list {
                let rule_name = if need.proto == "udp" {
                    "agent-bridge-udp".to_string()
                } else {
                    "agent-bridge".to_string()
                };
                let show_arg = format!("name={rule_name}");
                if let Ok((0, _)) = runner.run(
                    "netsh",
                    &["advfirewall", "firewall", "show", "rule", &show_arg],
                ) {
                    skipped.push(rule_name);
                    continue;
                }
                let port_arg = format!("localport={}", need.port);
                match runner.run(
                    "netsh",
                    &[
                        "advfirewall",
                        "firewall",
                        "add",
                        "rule",
                        &show_arg,
                        "dir=in",
                        "action=allow",
                        &format!("protocol={}", need.proto.to_uppercase()),
                        &port_arg,
                    ],
                ) {
                    Ok((0, _)) => applied.push(rule_name),
                    Ok((code, out)) => {
                        failure = Some(format!("放行 {rule_name} 失败（退出码 {code}）：{}", first_line(&out)));
                        break;
                    }
                    Err(e) => {
                        failure = Some(format!("放行 {rule_name} 失败：{e}"));
                        break;
                    }
                }
            }
            let detail = if let Some(failure) = failure {
                failure
            } else if applied.is_empty() {
                format!("已放行（规则已存在）：{}", skipped.join("、"))
            } else {
                format!("已放行：{}", applied.join("、"))
            };
            FirewallReport {
                manager: manager(),
                active: true,
                applied: !applied.is_empty(),
                detail,
            }
        }
        Err(e) => FirewallReport {
            manager: None,
            active: false,
            applied: false,
            detail: format!("无法探测 Windows 防火墙：{e}"),
        },
    }
}

/// 托盘宿主探测（经注入执行器，可单测）：会话 D-Bus 是否存在
/// org.kde.StatusNotifierWatcher（gdbus 优先、dbus-send 回退）。
/// 检测工具缺失 → 返回 false（按不可用处理，宁降级不产生幽灵进程）。
pub fn tray_host_available_with(runner: &dyn CommandRunner) -> bool {
    let probe = |out: &str| out.contains("true");
    if let Ok((0, out)) = runner.run(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            "org.freedesktop.DBus",
            "--object-path",
            "/org/freedesktop/DBus",
            "--method",
            "org.freedesktop.DBus.NameHasOwner",
            "org.kde.StatusNotifierWatcher",
        ],
    ) {
        return probe(&out);
    }
    if let Ok((0, out)) = runner.run(
        "dbus-send",
        &[
            "--session",
            "--dest=org.freedesktop.DBus",
            "--type=method_call",
            "--print-reply",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus.NameHasOwner",
            "string:org.kde.StatusNotifierWatcher",
        ],
    ) {
        return probe(&out);
    }
    false
}

/// 受限模式（无管理员权限）下的占位报告。
pub fn skipped(reason: &str, port: u16, udp_discovery_port: Option<u16>) -> FirewallReport {
    let list = needs(port, udp_discovery_port);
    FirewallReport {
        manager: None,
        active: false,
        applied: false,
        detail: format!(
            "未执行（{reason}）；如需手动放行所需端口（{}）",
            needs_text(&list)
        ),
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}
