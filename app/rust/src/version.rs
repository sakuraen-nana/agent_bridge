//! 新应用版本源（全仓唯一）：源头为 `Cargo.toml` 的 `package.version`。
//!
//! 按 AGENTS.md「归档即 bump」规则推进；Python 版的 `BRIDGE_VERSION` 已冻结，
//! 两者互不联动（详见本变更 design D8/D11）。

/// 应用版本（编译期取自 `Cargo.toml`，经桥接面在信息面板展示）。
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
