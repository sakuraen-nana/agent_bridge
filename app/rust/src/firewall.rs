//! 防火墙检测与幂等放行（design D3）。
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

/// 生产入口：以系统执行器检测并放行（桥接面的调用点）。
pub fn ensure_system(port: u16) -> FirewallReport {
    ensure(port, &SystemRunner)
}

/// 检测并（必要时）幂等放行实际服务端口。
pub fn ensure(port: u16, runner: &dyn CommandRunner) -> FirewallReport {
    #[cfg(windows)]
    {
        ensure_windows(port, runner)
    }
    #[cfg(not(windows))]
    {
        ensure_unix(port, runner)
    }
}

fn ensure_unix(port: u16, runner: &dyn CommandRunner) -> FirewallReport {
    let needle = format!("{port}/tcp");

    // 1) ufw
    if let Ok((_, out)) = runner.run("ufw", &["status"]) {
        if out.contains("Status: active") {
            if out.contains(&needle) {
                return FirewallReport {
                    manager: Some("ufw".into()),
                    active: true,
                    applied: false,
                    detail: format!("已放行 {needle}（跳过重复添加）"),
                };
            }
            return match runner.run("ufw", &["allow", &needle]) {
                Ok((0, _)) => FirewallReport {
                    manager: Some("ufw".into()),
                    active: true,
                    applied: true,
                    detail: format!("已放行 {needle}"),
                },
                Ok((code, out)) => FirewallReport {
                    manager: Some("ufw".into()),
                    active: true,
                    applied: false,
                    detail: format!("放行失败（退出码 {code}）：{}", first_line(&out)),
                },
                Err(e) => FirewallReport {
                    manager: Some("ufw".into()),
                    active: true,
                    applied: false,
                    detail: format!("放行失败：{e}"),
                },
            };
        }
        if out.contains("Status: inactive") {
            return FirewallReport {
                manager: Some("ufw".into()),
                active: false,
                applied: false,
                detail: format!("ufw 未激活，无需放行（如需手动：ufw allow {needle}）"),
            };
        }
        // 权限不足等：如实报告并继续探测下一个管理器
        if out.to_ascii_lowercase().contains("root") {
            return FirewallReport {
                manager: Some("ufw".into()),
                active: false,
                applied: false,
                detail: format!("无权限执行 ufw（受限模式下请手动放行 {needle}）"),
            };
        }
    }

    // 2) firewalld
    if let Ok((_, out)) = runner.run("firewall-cmd", &["--state"]) {
        if out.contains("running") {
            let query = format!("--query-port={needle}");
            if let Ok((0, _)) = runner.run("firewall-cmd", &[&query]) {
                return FirewallReport {
                    manager: Some("firewalld".into()),
                    active: true,
                    applied: false,
                    detail: format!("已放行 {needle}（跳过重复添加）"),
                };
            }
            let add = format!("--add-port={needle}");
            return match runner.run("firewall-cmd", &["--permanent", &add]) {
                Ok((0, _)) => {
                    let _ = runner.run("firewall-cmd", &["--reload"]);
                    FirewallReport {
                        manager: Some("firewalld".into()),
                        active: true,
                        applied: true,
                        detail: format!("已放行 {needle}"),
                    }
                }
                Ok((code, out)) => FirewallReport {
                    manager: Some("firewalld".into()),
                    active: true,
                    applied: false,
                    detail: format!("放行失败（退出码 {code}）：{}", first_line(&out)),
                },
                Err(e) => FirewallReport {
                    manager: Some("firewalld".into()),
                    active: true,
                    applied: false,
                    detail: format!("放行失败：{e}"),
                },
            };
        }
        return FirewallReport {
            manager: Some("firewalld".into()),
            active: false,
            applied: false,
            detail: format!("firewalld 未运行，无需放行（如需手动：firewall-cmd --add-port={needle}）"),
        };
    }

    FirewallReport {
        manager: None,
        active: false,
        applied: false,
        detail: format!("未检测到受支持的活跃防火墙（ufw/firewalld）；如有自管规则请手动放行 {needle}"),
    }
}

#[allow(dead_code)] // 仅 Windows 分支调用；保留以便单测注入
pub fn ensure_windows(port: u16, runner: &dyn CommandRunner) -> FirewallReport {
    let manager = || Some("Windows Defender".to_string());
    match runner.run("netsh", &["advfirewall", "show", "allprofiles", "state"]) {
        Ok((_, out)) => {
            if !out.contains("State") || !out.contains("ON") {
                return FirewallReport {
                    manager: manager(),
                    active: false,
                    applied: false,
                    detail: format!("Windows 防火墙未开启，无需放行（端口 {port}/tcp）"),
                };
            }
            if let Ok((0, _)) =
                runner.run("netsh", &["advfirewall", "firewall", "show", "rule", "name=agent-bridge"])
            {
                return FirewallReport {
                    manager: manager(),
                    active: true,
                    applied: false,
                    detail: format!("已放行 {port}/tcp（规则 agent-bridge 已存在）"),
                };
            }
            let port_arg = format!("localport={port}");
            match runner.run(
                "netsh",
                &[
                    "advfirewall",
                    "firewall",
                    "add",
                    "rule",
                    "name=agent-bridge",
                    "dir=in",
                    "action=allow",
                    "protocol=TCP",
                    &port_arg,
                ],
            ) {
                Ok((0, _)) => FirewallReport {
                    manager: manager(),
                    active: true,
                    applied: true,
                    detail: format!("已放行 {port}/tcp（规则 agent-bridge）"),
                },
                Ok((code, out)) => FirewallReport {
                    manager: manager(),
                    active: true,
                    applied: false,
                    detail: format!("放行失败（退出码 {code}）：{}", first_line(&out)),
                },
                Err(e) => FirewallReport {
                    manager: manager(),
                    active: true,
                    applied: false,
                    detail: format!("放行失败：{e}"),
                },
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
pub fn skipped(reason: &str, port: u16) -> FirewallReport {
    FirewallReport {
        manager: None,
        active: false,
        applied: false,
        detail: format!("未执行（{reason}）；如需手动放行 {port}/tcp"),
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}
