//! 桥接面：本机默认短名设置/清空与启动补全。

use crate::api::init::AppSnapshot;
use crate::config;

/// 设置（`Some`）或清空（`None`）本机短名；返回刷新后的完整快照。
///
/// 短名不合法时返回可读错误（界面展示），原值保持不变。
pub async fn set_short_name(name: Option<String>) -> anyhow::Result<AppSnapshot> {
    let data_dir = config::resolve_data_dir()?;
    config::set_short_name(&data_dir, name.as_deref())?;
    crate::api::init::app_init().await
}

/// 启动补全：本机短名为空时以本机设备名（主机名）初始化；返回补全后生效的短名。
///
/// 仅由应用启动路径（Dart `main()`）调用一次——MUST NOT 并入 `app_init`：清空按钮为
/// 刷新快照也调用 `app_init`，并入会使清空被当场填回（design D1）。
pub async fn ensure_default_short_name() -> anyhow::Result<Option<String>> {
    let data_dir = config::resolve_data_dir()?;
    let device_name = sysinfo::System::host_name().unwrap_or_default();
    Ok(config::ensure_default_short_name(&data_dir, &device_name)?)
}
