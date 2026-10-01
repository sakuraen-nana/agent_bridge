//! 应用内部错误类型。
//!
//! 模块层用 thiserror 定义枚举；桥接层经 `anyhow` 以可读消息送达 UI
//! （design D4：Dart 侧只做展示）。

/// agent-bridge 核心错误。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 无法从环境确定用户数据目录（如缺 HOME / APPDATA）。
    #[error("无法确定用户数据目录：{0}")]
    DataDirUnavailable(String),

    /// 数据目录创建或访问失败。
    #[error("数据目录不可用：{0}")]
    DataDirIo(String),

    /// 读取配置文件失败（IO 层；解析失败不属于此列，走备份重建路径）。
    #[error("读取配置文件失败：{0}")]
    ConfigRead(String),

    /// 写入配置文件失败。
    #[error("写入配置文件失败：{0}")]
    ConfigWrite(String),

    /// 短名不满足规格（去首尾空白后 1–32 字符、不含空白或控制字符）。
    #[error("短名无效：{0}")]
    InvalidShortName(String),
}
