//! 短名寻址与冲突无效化（design D5）。

use agent_bridge::config::Peer;
use agent_bridge::peers::{is_conflicted_key, resolve_peer, ResolveError};

const A: &str = "11111111-1111-4111-8111-111111111111";
const B: &str = "22222222-2222-4222-8222-222222222222";
const C: &str = "33333333-3333-4333-8333-333333333333";

fn peer(uuid: &str, short: Option<&str>, addr: &str) -> Peer {
    Peer {
        uuid: uuid.to_string(),
        short_name: short.map(str::to_string),
        address: addr.to_string(),
        port: 37777,
        token: "t".to_string(),
    }
}

#[test]
fn unique_short_name_hits_case_insensitively() {
    let peers = vec![peer(A, Some("Dev-A"), "10.0.0.1"), peer(B, Some("prod"), "10.0.0.2")];
    assert_eq!(resolve_peer(&peers, "dev-a").unwrap().uuid, A);
    assert_eq!(resolve_peer(&peers, " DEV-A ").unwrap().uuid, A);
    assert_eq!(resolve_peer(&peers, "PROD").unwrap().uuid, B);
}

#[test]
fn uuid_lookup_always_available_even_with_conflicts() {
    let peers = vec![peer(A, Some("dev"), "10.0.0.1"), peer(B, Some("DEV"), "10.0.0.2")];
    assert_eq!(resolve_peer(&peers, A).unwrap().uuid, A);
    assert_eq!(resolve_peer(&peers, B).unwrap().uuid, B);
}

#[test]
fn conflicting_key_invalidates_every_involved_peer() {
    let peers = vec![
        peer(A, Some("dev"), "10.0.0.1"),
        peer(B, Some("DEV"), "10.0.0.2"),
        peer(C, Some("prod"), "10.0.0.3"),
    ];
    match resolve_peer(&peers, "Dev") {
        Err(ResolveError::Conflict { uuids, .. }) => {
            assert_eq!(uuids.len(), 2);
            assert!(uuids.contains(&A.to_string()) && uuids.contains(&B.to_string()));
        }
        other => panic!("应判为冲突，实际：{other:?}"),
    }
    // 冲突逐键独立：其他键不受影响
    assert_eq!(resolve_peer(&peers, "prod").unwrap().uuid, C);
    assert!(is_conflicted_key(&peers, "dev"));
    assert!(!is_conflicted_key(&peers, "prod"));
}

#[test]
fn unknown_reference_reports_unknown() {
    let peers = vec![peer(A, Some("dev"), "10.0.0.1")];
    assert!(matches!(
        resolve_peer(&peers, "nope"),
        Err(ResolveError::Unknown { .. })
    ));
    assert!(matches!(
        resolve_peer(&peers, ""),
        Err(ResolveError::Unknown { .. })
    ));
}

#[test]
fn unnamed_peer_not_reachable_by_short_name() {
    let peers = vec![peer(A, None, "10.0.0.1")];
    assert!(matches!(
        resolve_peer(&peers, "anything"),
        Err(ResolveError::Unknown { .. })
    ));
    assert_eq!(resolve_peer(&peers, A).unwrap().uuid, A);
}
