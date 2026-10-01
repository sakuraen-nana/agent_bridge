//! 桥接面（flutter_rust_bridge）：只暴露快照结构体与少量动作函数，
//! 业务真相全部落在 `crate::config` 等内部模块（design D4）。

pub mod device;
pub mod init;

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // frb 默认工具初始化（日志等）
    flutter_rust_bridge::setup_default_user_utils();
}
