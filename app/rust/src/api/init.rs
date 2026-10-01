//! 桥接面：初始化快照与系统信息刷新。

use crate::config;
use crate::version;

pub use crate::sysinfo_view::SystemSnapshot;

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
}

/// 初始化：解析数据目录、读取/创建配置，返回完整快照。
pub async fn app_init() -> anyhow::Result<AppSnapshot> {
    let data_dir = config::resolve_data_dir()?;
    let outcome = config::load_or_create(&data_dir)?;
    Ok(AppSnapshot {
        version: version::APP_VERSION.to_string(),
        uuid: outcome.device.uuid,
        short_name: outcome.device.short_name,
        notice: outcome.notice,
        system: crate::sysinfo_view::collect(),
    })
}

/// 仅刷新系统信息（面板的「刷新」动作；身份与配置不变）。
pub async fn refresh_system() -> anyhow::Result<SystemSnapshot> {
    Ok(crate::sysinfo_view::collect())
}
