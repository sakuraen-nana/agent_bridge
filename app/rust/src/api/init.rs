//! 桥接面：初始化快照与系统信息刷新（含提权、服务端、防火墙；design D1/D3/D11）。

use std::sync::{Mutex, OnceLock};

use crate::autostart::{self, AutostartStatus};
use crate::config;
use crate::elevation::{self, ElevationOutcome};
use crate::firewall::{self, FirewallReport};
use crate::identity;
use crate::server::{self, ServerConfig, ServerHandle};
use crate::version;

pub use crate::sysinfo_view::SystemSnapshot;

/// 提权状态（面板「管理员权限」行）。
#[derive(Debug, Clone)]
pub struct ElevationSnapshot {
    /// 是否以管理员/root 运行。
    pub admin: bool,
    /// 权限说明或受限模式原因。
    pub detail: String,
}

/// 防火墙状态（面板「防火墙」行）。
#[derive(Debug, Clone)]
pub struct FirewallSnapshot {
    /// 命中的管理器名（未检出为 null）。
    pub manager: Option<String>,
    /// 管理器是否活跃。
    pub active: bool,
    /// 本次是否实际添加了放行规则。
    pub applied: bool,
    /// 面向人类的状态说明。
    pub detail: String,
}

/// 服务端状态（信息面板 / 横幅展示用）。
#[derive(Debug, Clone)]
pub struct ServerSnapshot {
    /// 是否已启动。
    pub running: bool,
    /// 监听端口。
    pub port: u16,
    /// 启动失败时的可读原因。
    pub error: Option<String>,
}

/// 开机自启状态（桥接面结构）。
#[derive(Debug, Clone)]
pub struct AutostartInfo {
    /// 是否已启用。
    pub enabled: bool,
    /// 机制名（如 autostart .desktop / HKCU Run）。
    pub mechanism: String,
    /// 机制详情（路径或键址）。
    pub detail: String,
}

impl From<AutostartStatus> for AutostartInfo {
    fn from(status: AutostartStatus) -> Self {
        Self {
            enabled: status.enabled,
            mechanism: status.mechanism,
            detail: status.detail,
        }
    }
}

/// 应用启动快照（信息面板的数据面）。
#[derive(Debug, Clone)]
pub struct AppSnapshot {
    /// 应用版本（新应用版本源）。
    pub version: String,
    /// 设备 UUID（持久）。
    pub uuid: String,
    /// 本机默认短名；`None` 表示未设置。
    pub short_name: Option<String>,
    /// 一次性界面提示（如配置损坏重建）；正常时为 `None`。
    pub notice: Option<String>,
    /// 系统信息。
    pub system: SystemSnapshot,
    /// 服务端状态。
    pub server: ServerSnapshot,
    /// 管理员权限状态。
    pub elevation: ElevationSnapshot,
    /// 防火墙状态。
    pub firewall: FirewallSnapshot,
}

/// 进程级运行上下文（会话 token 与服务端句柄）。
struct AppRuntime {
    session_token: String,
    server: Mutex<Option<ServerHandle>>,
}

static APP_RUNTIME: OnceLock<AppRuntime> = OnceLock::new();

fn app_runtime() -> &'static AppRuntime {
    APP_RUNTIME.get_or_init(|| AppRuntime {
        session_token: identity::new_token(),
        server: Mutex::new(None),
    })
}

/// 确保服务端已启动（幂等；同一进程只启动一个实例）。
fn ensure_server(data_dir: &std::path::Path, workdir: Option<String>) -> ServerSnapshot {
    let runtime = app_runtime();
    let mut guard = runtime.server.lock().unwrap();
    if let Some(handle) = guard.as_ref() {
        return ServerSnapshot {
            running: true,
            port: handle.port(),
            error: None,
        };
    }
    // AGENT_BRIDGE_PORT：仅作命令行实跑的临时覆盖口（不面向用户文档）
    let port = std::env::var("AGENT_BRIDGE_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(config::DEFAULT_PORT);
    let server_config = ServerConfig::new(
        port,
        data_dir.to_path_buf(),
        runtime.session_token.clone(),
        workdir,
    );
    match server::start(server_config) {
        Ok(handle) => {
            let snapshot = ServerSnapshot {
                running: true,
                port: handle.port(),
                error: None,
            };
            *guard = Some(handle);
            snapshot
        }
        Err(e) => ServerSnapshot {
            running: false,
            port,
            error: Some(e.to_string()),
        },
    }
}

/// 初始化：提权检测（最前）→ 配置 → 服务端 → 防火墙 → 快照。
///
/// 提权重启路径（Linux pkexec）会直接结束本实例（新实例已在运行）。
pub async fn app_init() -> anyhow::Result<AppSnapshot> {
    let elevation_status = match elevation::ensure() {
        ElevationOutcome::Continue(status) => status,
        ElevationOutcome::Relaunching => std::process::exit(0),
    };

    let data_dir = config::resolve_data_dir()?;
    let outcome = config::load_or_create(&data_dir)?;
    let server = ensure_server(&data_dir, outcome.device.workdir.clone());

    let firewall_report = decide_firewall(&server, elevation_status.admin).await;

    Ok(AppSnapshot {
        version: version::APP_VERSION.to_string(),
        uuid: outcome.device.uuid,
        short_name: outcome.device.short_name,
        notice: outcome.notice,
        system: crate::sysinfo_view::collect(),
        server,
        elevation: ElevationSnapshot {
            admin: elevation_status.admin,
            detail: elevation_status.detail,
        },
        firewall: FirewallSnapshot {
            manager: firewall_report.manager,
            active: firewall_report.active,
            applied: firewall_report.applied,
            detail: firewall_report.detail,
        },
    })
}

/// 防火墙步骤决策：服务端未运行或受限模式 → 如实跳过；否则探测并放行。
async fn decide_firewall(server: &ServerSnapshot, admin: bool) -> FirewallReport {
    if !server.running {
        return firewall::skipped("服务端未运行", server.port);
    }
    if !admin {
        return firewall::skipped("受限模式（未获得管理员权限）", server.port);
    }
    let port = server.port;
    tokio::task::spawn_blocking(move || firewall::ensure_system(port))
        .await
        .unwrap_or_else(|e| FirewallReport {
            manager: None,
            active: false,
            applied: false,
            detail: format!("防火墙检测异常：{e}"),
        })
}

/// 仅刷新系统信息（面板的「刷新」动作；身份、配置与状态不变）。
pub async fn refresh_system() -> anyhow::Result<SystemSnapshot> {
    Ok(crate::sysinfo_view::collect())
}

/// 生成「本机配置」TOML 片段（含本次会话短期 token；供复制到剪贴板）。
pub async fn share_payload() -> anyhow::Result<String> {
    let data_dir = config::resolve_data_dir()?;
    let outcome = config::load_or_create(&data_dir)?;
    let runtime = app_runtime();
    let (port, token) = {
        let guard = runtime.server.lock().unwrap();
        let Some(handle) = guard.as_ref() else {
            anyhow::bail!("服务端未运行：请先确保应用面板显示「服务端：运行中」");
        };
        (handle.port(), handle.session_token().to_string())
    };

    let address = select_share_address(&crate::sysinfo_view::interface_ipv4s())
        .ok_or_else(|| anyhow::anyhow!("没有可用的局域网地址（已排除环回与虚拟网桥接口）"))?;
    let short_line = outcome
        .device
        .short_name
        .as_deref()
        .map(|name| format!("\nshort_name = \"{name}\""))
        .unwrap_or_default();
    Ok(format!(
        "# agent-bridge 本机配置（token 为本次会话短期 token，应用重启后失效）\n[[peer]]\nuuid = \"{}\"{short_line}\naddress = \"{address}\"\nport = {port}\ntoken = \"{token}\"\n",
        outcome.device.uuid
    ))
}

/// 地址选择（纯函数，可单测）：排除虚拟/容器接口，私网优先，取第一个。
pub fn select_share_address(candidates: &[(String, String)]) -> Option<String> {
    const VIRTUAL_PREFIXES: [&str; 10] = [
        "lo",
        "docker",
        "br-",
        "veth",
        "virbr",
        "tailscale",
        "tun",
        "wg",
        "zt",
        "vmnet",
    ];
    let usable: Vec<&(String, String)> = candidates
        .iter()
        .filter(|(iface, _)| !VIRTUAL_PREFIXES.iter().any(|p| iface.starts_with(p)))
        .collect();
    let is_rfc1918 = |addr: &str| {
        if addr.starts_with("10.") || addr.starts_with("192.168.") {
            return true;
        }
        if let Some(rest) = addr.strip_prefix("172.") {
            let second: u32 = rest
                .split('.')
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            return (16..=31).contains(&second);
        }
        false
    };
    usable
        .iter()
        .find(|(_, addr)| is_rfc1918(addr))
        .map(|(_, addr)| addr.clone())
        .or_else(|| usable.first().map(|(_, addr)| addr.clone()))
}

/// 托盘宿主是否可用（design D4 降级判据）。
///
/// Linux 上查询会话 D-Bus 是否存在 StatusNotifierWatcher（gdbus/dbus-send 任一可用）；
/// 检测工具缺失时按不可用处理——宁可降级为关窗即退出，也不产生没有可见入口的幽灵进程。
/// Windows 恒返回 true（系统托盘始终存在）。
pub async fn tray_host_available() -> bool {
    #[cfg(unix)]
    {
        firewall::tray_host_available_with(&firewall::SystemRunner)
    }
    #[cfg(windows)]
    {
        true
    }
}

/// 查询开机自启状态。
pub async fn autostart_status() -> anyhow::Result<AutostartInfo> {
    Ok(autostart::status()?.into())
}

/// 设置开机自启（幂等：重复开/关不产生重复项）。
pub async fn set_autostart(enabled: bool) -> anyhow::Result<AutostartInfo> {
    let status = if enabled {
        autostart::enable()?
    } else {
        autostart::disable()?
    };
    Ok(status.into())
}
