//! 多设备配置的短名解析与冲突规则（design D5；CLI 与后续 GUI 共用）。

use crate::config::Peer;
use crate::identity::short_name_compare_key;

/// 以短名 / UUID 解析目标设备时的失败原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// 不匹配任何条目。
    Unknown { reference: String },
    /// 短名比较键对应多条；涉及的各条目短名均无效化。
    Conflict { reference: String, uuids: Vec<String> },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Unknown { reference } => write!(
                f,
                "找不到设备「{reference}」：配置中没有匹配的短名或 UUID"
            ),
            ResolveError::Conflict { reference, uuids } => write!(
                f,
                "短名「{reference}」冲突（涉及 {} 台设备：{}）——冲突各方短名均已无效，请改名解除冲突，或改用 UUID 指代设备",
                uuids.len(),
                uuids.join("、")
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

/// 判断某短名比较键在全体 peer 中是否冲突（多条目同名 → 该键下短名全部无效）。
pub fn is_conflicted_key(peers: &[Peer], key: &str) -> bool {
    peers
        .iter()
        .filter(|p| {
            p.short_name
                .as_deref()
                .map(short_name_compare_key)
                .as_deref()
                == Some(key)
        })
        .count()
        > 1
}

/// 以 UUID（恒可用）或短名（比较键唯一时才可用）解析目标设备。
pub fn resolve_peer<'a>(peers: &'a [Peer], reference: &str) -> Result<&'a Peer, ResolveError> {
    if let Some(found) = peers.iter().find(|p| p.uuid == reference) {
        return Ok(found);
    }
    let key = short_name_compare_key(reference);
    let matches: Vec<&Peer> = peers
        .iter()
        .filter(|p| {
            p.short_name
                .as_deref()
                .map(short_name_compare_key)
                .as_deref()
                == Some(key.as_str())
        })
        .collect();
    match matches.len() {
        0 => Err(ResolveError::Unknown {
            reference: reference.to_string(),
        }),
        1 => Ok(matches[0]),
        _ => Err(ResolveError::Conflict {
            reference: reference.to_string(),
            uuids: matches.iter().map(|p| p.uuid.clone()).collect(),
        }),
    }
}
