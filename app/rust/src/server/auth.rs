//! 双 token 认证（design D3）：会话 token 与长期 token 任一有效即通过；
//! 恒定时间比较；失败统一 404（由路由层处理）。

use subtle::ConstantTimeEq;

use super::log::mask_token;
use super::state::ServerState;

/// 认证判定结果（同时用于留痕的脱敏描述）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenStatus {
    Missing,
    Invalid {
        /// 脱敏片段（前 4 位）。
        masked: String,
    },
    Session,
    LongTerm,
}

impl TokenStatus {
    pub fn is_valid(&self) -> bool {
        matches!(self, TokenStatus::Session | TokenStatus::LongTerm)
    }

    /// 留痕用描述（脱敏；不含 token 取值）。
    pub fn describe(&self) -> String {
        match self {
            TokenStatus::Missing => "缺失".to_string(),
            TokenStatus::Invalid { masked } => format!("不匹配（{masked}）"),
            TokenStatus::Session => "有效（会话）".to_string(),
            TokenStatus::LongTerm => "有效（长期）".to_string(),
        }
    }
}

/// 校验请求 token：接受会话 token 或长期 token 任一。
pub fn check(state: &ServerState, token: Option<&str>) -> TokenStatus {
    let Some(token) = token.filter(|t| !t.is_empty()) else {
        return TokenStatus::Missing;
    };
    // 两类 token 均做恒时比较后再判定，避免以比较顺序泄露信息
    let session_match = constant_time_eq(token.as_bytes(), state.session_token.as_bytes());
    let long_term_match = constant_time_eq(token.as_bytes(), state.long_term_token().as_bytes());
    if session_match {
        TokenStatus::Session
    } else if long_term_match {
        TokenStatus::LongTerm
    } else {
        TokenStatus::Invalid {
            masked: mask_token(token),
        }
    }
}

/// 恒定时间字节比较。
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    bool::from(a.ct_eq(b))
}
