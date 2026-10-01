//! Hello 问候 API（design：返回设备与系统信息；双 token 认证由路由层统一处理）。

use serde_json::{json, Value};

use super::state::ServerState;
use crate::sysinfo_view;
use crate::version;

/// 组装 hello 响应。
pub fn payload(state: &ServerState) -> Value {
    let (uuid, short_name) = state.device_snapshot();
    let hostname = sysinfo::System::host_name().unwrap_or_default();
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    json!({
        "version": version::APP_VERSION,
        "uuid": uuid,
        "short_name": short_name,
        "hostname": hostname,
        "user": user,
        "system": sysinfo::System::name().unwrap_or_default(),
        "release": sysinfo::System::kernel_version().unwrap_or_default(),
        "platform": sysinfo::System::os_version().unwrap_or_default(),
        "cwd": state.workdir.to_string_lossy(),
        "lan_ips": sysinfo_view::lan_ip_list(),
        "started_at": state.started_at,
    })
}
