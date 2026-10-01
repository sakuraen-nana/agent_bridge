//! 提权：重启命令构造、PATH 查找与 root 直通（design D1）。

use std::collections::HashMap;
use std::path::Path;

use agent_bridge::elevation::{ElevationOutcome, build_relaunch_plan, ensure, find_in_path};

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

#[test]
fn relaunch_plan_carries_whitelisted_env_and_caller_config_home() {
    let env = env_of(&[
        ("DISPLAY", ":0"),
        ("XAUTHORITY", "/home/u/.Xauthority"),
        ("UNRELATED", "secret"),
    ]);
    let plan = build_relaunch_plan(
        &env,
        Path::new("/opt/ab/app"),
        Path::new("/usr/bin/pkexec"),
        Some(Path::new("/home/u/.config")),
    )
    .unwrap();
    assert_eq!(plan.program, Path::new("/usr/bin/pkexec"));
    assert!(plan.args.contains(&"DISPLAY=:0".to_string()));
    assert!(plan.args.contains(&"XAUTHORITY=/home/u/.Xauthority".to_string()));
    assert!(
        plan.args.contains(&"XDG_CONFIG_HOME=/home/u/.config".to_string()),
        "须显式传递调用者配置根：{:?}",
        plan.args
    );
    assert!(!plan.args.iter().any(|a| a.contains("UNRELATED")), "白名单外变量不得透传");
    assert_eq!(plan.args.last().unwrap(), "/opt/ab/app");
}

#[test]
fn relaunch_plan_requires_config_home() {
    let env = env_of(&[("DISPLAY", ":0")]);
    assert!(build_relaunch_plan(&env, Path::new("/x"), Path::new("/p"), None).is_err());
}

#[test]
fn find_in_path_locates_existing_binary() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("pkexec");
    std::fs::write(&bin, b"#!/bin/sh\n").unwrap();
    let found = find_in_path("pkexec", Some(dir.path().to_string_lossy().into_owned()));
    assert_eq!(found.as_deref(), Some(bin.as_path()));
    assert!(find_in_path("pkexec", Some("/nonexistent-dir".to_string())).is_none());
    assert!(find_in_path("pkexec", None).is_none());
}

#[test]
fn ensure_as_root_continues_with_admin() {
    if agent_bridge::config::current_euid() != 0 {
        eprintln!("跳过：需以 root 运行（本机开发环境即 root）");
        return;
    }
    match ensure() {
        ElevationOutcome::Continue(status) => {
            assert!(status.admin, "root 下应判定为已具备管理员权限");
            assert!(status.detail.contains("已具备"));
        }
        ElevationOutcome::Relaunching => panic!("root 下不应发起提权重启"),
    }
}
