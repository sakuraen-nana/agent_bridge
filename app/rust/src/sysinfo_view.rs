//! 启动信息面板的取值采集（design D8 口径：可核对性优先，不做美化换算）。

use std::net::IpAddr;

use sysinfo::{Networks, System};

/// 面板所需的系统信息快照（全部为面向展示的字符串，取值来自当前系统实际状态）。
#[derive(Debug, Clone)]
pub struct SystemSnapshot {
    /// 操作系统名称与版本（如 `Ubuntu 24.04（内核 6.8.0-xx）`）。
    pub platform: String,
    /// 区域与语言（系统区域设置的 BCP-47 标签，如 `zh-CN`）。
    pub locale: String,
    /// 当前本地日期时间（含时区偏移）。
    pub local_time: String,
    /// CPU 型号与物理核心数。
    pub cpu: String,
    /// 内存总量与可用量。
    pub memory: String,
    /// 局域网 IP 地址列表（地址 + 接口名；无地址时为空表，由界面展示占位）。
    pub ip_addresses: Vec<String>,
}

/// 采集当前系统信息。
pub fn collect() -> SystemSnapshot {
    let sys = System::new_all();
    SystemSnapshot {
        platform: platform_string(),
        locale: sys_locale::get_locale().unwrap_or_else(|| "未知".to_string()),
        local_time: chrono::Local::now().format("%Y-%m-%d %H:%M:%S %:z").to_string(),
        cpu: cpu_string(&sys),
        memory: memory_string(&sys),
        ip_addresses: ip_addresses(),
    }
}

/// 系统名 + 版本（+ 内核）。
fn platform_string() -> String {
    let name = System::name().unwrap_or_else(|| std::env::consts::OS.to_string());
    let version = System::os_version().unwrap_or_default();
    let kernel = System::kernel_version().unwrap_or_default();
    match (version.trim().is_empty(), kernel.trim().is_empty()) {
        (false, false) => format!("{name} {version}（内核 {kernel}）"),
        (false, true) => format!("{name} {version}"),
        _ => name,
    }
}

/// CPU 型号 + 物理核心数。
fn cpu_string(sys: &System) -> String {
    let brand = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_default();
    match (brand.is_empty(), System::physical_core_count()) {
        (false, Some(cores)) => format!("{brand}（{cores} 物理核）"),
        (false, None) => brand,
        (true, Some(cores)) => format!("{cores} 物理核"),
        (true, None) => "未知".to_string(),
    }
}

/// 内存总量与可用量。
fn memory_string(sys: &System) -> String {
    format!(
        "总计 {} · 可用 {}",
        human_bytes(sys.total_memory()),
        human_bytes(sys.available_memory())
    )
}

/// 字节数转人性化单位（GiB 一位小数；不足 1 GiB 用 MiB）。
fn human_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{:.1} GiB", b / GIB)
    } else {
        format!("{:.0} MiB", b / MIB)
    }
}

/// 非环回、非 link-local、非未指定、非广播的 IPv4 地址（附接口名）。
fn ip_addresses() -> Vec<String> {
    let networks = Networks::new_with_refreshed_list();
    let mut out = Vec::new();
    for (name, data) in &networks {
        for ip_net in data.ip_networks() {
            if let IpAddr::V4(v4) = ip_net.addr {
                if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() && !v4.is_broadcast() {
                    out.push(format!("{v4} ({name})"));
                }
            }
        }
    }
    out
}

/// 局域网 IP 地址列表（纯地址、无接口名；供 hello 响应使用）。
pub fn lan_ip_list() -> Vec<String> {
    let networks = Networks::new_with_refreshed_list();
    let mut out = Vec::new();
    for (_, data) in &networks {
        for ip_net in data.ip_networks() {
            if let IpAddr::V4(v4) = ip_net.addr {
                if !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified() && !v4.is_broadcast() {
                    out.push(v4.to_string());
                }
            }
        }
    }
    out
}
