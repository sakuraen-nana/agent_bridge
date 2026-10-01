//! 图形化配对（design D2）：单待决请求 + 一次性决定通道。
//!
//! 免 token 端点 `/pair/request` 的实现支撑：请求入表后长轮询等待使用者在
//! 界面做出决定；同时仅一条待决（其余「忙」）；同意时由调用方组装含长期
//! token 的配置响应。

use std::time::Instant;

use tokio::sync::oneshot;

use crate::server::ServerState;

/// 配对请求等待上限（秒）。
pub const PAIRING_TIMEOUT_SECS: u64 = 120;

/// 请求方身份（来源 IP 由连接取得）。
#[derive(Debug, Clone)]
pub struct PairRequester {
    pub uuid: String,
    pub short_name: Option<String>,
    pub port: u16,
    pub source_ip: String,
}

/// 待决请求（决定通道 + 元数据）。
pub struct Pending {
    pub id: u64,
    pub requester: PairRequester,
    pub created_at: Instant,
    decided: oneshot::Sender<bool>,
}

/// 待决摘要（供界面轮询展示）。
#[derive(Debug, Clone)]
pub struct PendingInfo {
    pub id: u64,
    pub uuid: String,
    pub short_name: Option<String>,
    pub port: u16,
    pub source_ip: String,
}

/// 登记一条待决请求；已有待决时返回 `Err(())`（忙）。
pub fn begin(
    state: &ServerState,
    requester: PairRequester,
) -> Result<oneshot::Receiver<bool>, ()> {
    let mut guard = state.pairing.lock().unwrap();
    if guard.is_some() {
        return Err(());
    }
    let (tx, rx) = oneshot::channel();
    let id = state.next_pairing_id();
    *guard = Some(Pending {
        id,
        requester,
        created_at: Instant::now(),
        decided: tx,
    });
    Ok(rx)
}

/// 当前待决摘要。
pub fn pending_info(state: &ServerState) -> Option<PendingInfo> {
    state
        .pairing
        .lock()
        .unwrap()
        .as_ref()
        .map(|pending| PendingInfo {
            id: pending.id,
            uuid: pending.requester.uuid.clone(),
            short_name: pending.requester.short_name.clone(),
            port: pending.requester.port,
            source_ip: pending.requester.source_ip.clone(),
        })
}

/// 做出决定；无待决时报错。
pub fn decide(state: &ServerState, approve: bool) -> Result<(), String> {
    let pending = state.pairing.lock().unwrap().take();
    match pending {
        Some(pending) => {
            let _ = pending.decided.send(approve);
            Ok(())
        }
        None => Err("当前没有待处理的配对请求".to_string()),
    }
}

/// 超时清理（仅当 id 相符，避免清掉后继的新请求）。
pub fn expire(state: &ServerState, id: u64) {
    let mut guard = state.pairing.lock().unwrap();
    if guard.as_ref().map(|pending| pending.id) == Some(id) {
        *guard = None;
    }
}
