//! 设备身份：UUID 生成与短名规则（design D7 口径，变更 ② 的冲突判定共用）。

use crate::error::AppError;

/// 短名长度上限（按 Unicode 标量计数，去首尾空白后）。
pub const SHORT_NAME_MAX_CHARS: usize = 32;

/// 生成设备 UUID（v4 随机）。
pub fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 校验并规范化短名：去首尾空白后 1–32 字符、不含空白或控制字符。
///
/// 通过后返回规范化（trim 后）的短名；失败返回带可读原因的 `AppError`。
pub fn validate_short_name(input: &str) -> Result<String, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidShortName("不能为空".to_string()));
    }
    let count = trimmed.chars().count();
    if count > SHORT_NAME_MAX_CHARS {
        return Err(AppError::InvalidShortName(format!(
            "最长 {SHORT_NAME_MAX_CHARS} 个字符（当前 {count} 个）"
        )));
    }
    if let Some(c) = trimmed.chars().find(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::InvalidShortName(format!(
            "不能包含空白或控制字符（发现 {c:?}）"
        )));
    }
    Ok(trimmed.to_string())
}

/// 短名判重比较键：去首尾空白后 Unicode 小写折叠（不区分大小写）。
pub fn short_name_compare_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// 生成 token：32 字节密码学随机源 → 64 位十六进制（≥24 字节熵达标）。
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("系统随机源不可用（getrandom 失败）");
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}
