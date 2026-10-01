//! 桥接面：本机默认短名设置/清空。

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
