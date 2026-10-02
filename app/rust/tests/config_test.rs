//! 配置模块集成测试：数据目录解析、创建/沿用、损坏备份重建、手工编辑与原子写。

use std::collections::HashMap;
use std::fs;

use agent_bridge::config::{self, CONFIG_FILE_NAME, TargetOs};

fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

#[test]
fn windows_dir_uses_appdata() {
    let dir = config::data_dir_from(
        &env_of(&[("APPDATA", r"C:\Users\me\AppData\Roaming")]),
        TargetOs::Windows,
    )
    .unwrap();
    assert_eq!(
        dir,
        std::path::PathBuf::from(r"C:\Users\me\AppData\Roaming").join("agent-bridge")
    );
}

#[test]
fn windows_dir_missing_appdata_errors() {
    assert!(config::data_dir_from(&env_of(&[]), TargetOs::Windows).is_err());
}

#[test]
fn unix_dir_prefers_absolute_xdg() {
    let dir = config::data_dir_from(
        &env_of(&[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/home/me")]),
        TargetOs::Unix,
    )
    .unwrap();
    assert_eq!(dir, std::path::PathBuf::from("/xdg/agent-bridge"));
}

#[test]
fn unix_dir_ignores_relative_xdg_and_falls_back_to_home() {
    let dir = config::data_dir_from(
        &env_of(&[("XDG_CONFIG_HOME", "relative/path"), ("HOME", "/home/me")]),
        TargetOs::Unix,
    )
    .unwrap();
    assert_eq!(dir, std::path::PathBuf::from("/home/me/.config/agent-bridge"));
}

#[test]
fn unix_dir_missing_home_errors() {
    assert!(config::data_dir_from(&env_of(&[]), TargetOs::Unix).is_err());
}

#[test]
fn creates_config_with_uuid_and_reuses_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("agent-bridge");

    let first = config::load_or_create(&dir).unwrap();
    assert!(first.notice.is_none());
    assert!(uuid::Uuid::parse_str(&first.device.uuid).is_ok());
    assert!(first.device.short_name.is_none());

    let second = config::load_or_create(&dir).unwrap();
    assert_eq!(first.device.uuid, second.device.uuid, "重启后应沿用同一 UUID");
}

#[cfg(unix)]
#[test]
fn config_file_permissions_are_0600() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let _ = config::load_or_create(tmp.path()).unwrap();
    let mode = fs::metadata(tmp.path().join(CONFIG_FILE_NAME))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn corrupt_config_is_backed_up_and_rebuilt() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join(CONFIG_FILE_NAME), "这不是 toml [[[").unwrap();

    let outcome = config::load_or_create(tmp.path()).unwrap();
    assert!(outcome.notice.is_some(), "应带回重建提示");
    assert!(uuid::Uuid::parse_str(&outcome.device.uuid).is_ok());

    let backups: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
        .collect();
    assert_eq!(backups.len(), 1, "应恰好有一个备份文件");

    let again = config::load_or_create(tmp.path()).unwrap();
    assert!(again.notice.is_none(), "重建后的配置应可直接加载");
}

#[test]
fn manual_edit_takes_effect_and_unknown_keys_survive_write() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(CONFIG_FILE_NAME);
    fs::write(
        &path,
        "# 手写注释\n[device]\nuuid = \"00000000-0000-4000-8000-000000000000\"\nshort_name = \"手改名\"\n\n[future_section]\nkey = \"keep\"\n",
    )
    .unwrap();

    let outcome = config::load_or_create(tmp.path()).unwrap();
    assert_eq!(outcome.device.short_name.as_deref(), Some("手改名"));

    config::set_short_name(tmp.path(), Some("新名")).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("# 手写注释"), "注释应保留");
    assert!(
        text.contains("[future_section]") && text.contains("keep"),
        "未知段应保留"
    );
    assert!(text.contains("新名"));

    config::set_short_name(tmp.path(), None).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("short_name"), "清空后不应残留键");
    let cleared = config::load_or_create(tmp.path()).unwrap();
    assert!(cleared.device.short_name.is_none());
    assert_eq!(
        cleared.device.uuid, "00000000-0000-4000-8000-000000000000",
        "手写 uuid 应保留"
    );
}

#[test]
fn missing_uuid_is_repaired_keeping_short_name() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join(CONFIG_FILE_NAME),
        "[device]\nshort_name = \"保留我\"\n",
    )
    .unwrap();

    let outcome = config::load_or_create(tmp.path()).unwrap();
    assert!(
        uuid::Uuid::parse_str(&outcome.device.uuid).is_ok(),
        "应补全 uuid"
    );
    assert_eq!(
        outcome.device.short_name.as_deref(),
        Some("保留我"),
        "短名不应被吞掉"
    );

    let text = fs::read_to_string(tmp.path().join(CONFIG_FILE_NAME)).unwrap();
    assert!(text.contains(&outcome.device.uuid), "补全结果应写回");
}

const VALID_UUID: &str = "00000000-0000-4000-8000-000000000000";
const VALID_UUID_2: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn ensure_default_short_name_fills_from_device_name_and_persists() {
    let tmp = tempfile::tempdir().unwrap();
    let filled = config::ensure_default_short_name(tmp.path(), "my-machine").unwrap();
    assert_eq!(filled.as_deref(), Some("my-machine"));

    let outcome = config::load_or_create(tmp.path()).unwrap();
    assert_eq!(outcome.device.short_name.as_deref(), Some("my-machine"));
    let text = fs::read_to_string(tmp.path().join(CONFIG_FILE_NAME)).unwrap();
    assert!(text.contains("my-machine"), "补全结果应写回文件");
}

#[test]
fn ensure_default_short_name_trims_device_name() {
    let tmp = tempfile::tempdir().unwrap();
    let filled = config::ensure_default_short_name(tmp.path(), "  my-machine  ").unwrap();
    assert_eq!(filled.as_deref(), Some("my-machine"), "应按短名规则去首尾空白");
}

#[test]
fn ensure_default_short_name_keeps_existing_name() {
    let tmp = tempfile::tempdir().unwrap();
    config::set_short_name(tmp.path(), Some("既有名")).unwrap();

    let kept = config::ensure_default_short_name(tmp.path(), "other-machine").unwrap();
    assert_eq!(kept.as_deref(), Some("既有名"));
    let text = fs::read_to_string(tmp.path().join(CONFIG_FILE_NAME)).unwrap();
    assert!(!text.contains("other-machine"), "不应覆盖已有非空短名");
}

#[test]
fn ensure_default_short_name_invalid_device_name_keeps_empty() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(
        config::ensure_default_short_name(tmp.path(), "bad name")
            .unwrap()
            .is_none(),
        "含空白的设备名应保持为空"
    );
    assert!(
        config::ensure_default_short_name(tmp.path(), &"x".repeat(33))
            .unwrap()
            .is_none(),
        "超长设备名应保持为空"
    );
    assert!(
        config::ensure_default_short_name(tmp.path(), "   ")
            .unwrap()
            .is_none(),
        "空白设备名应保持为空"
    );
    let text = fs::read_to_string(tmp.path().join(CONFIG_FILE_NAME)).unwrap();
    assert!(!text.contains("short_name"), "非法设备名不应写入短名键");
}

#[test]
fn ensure_default_short_name_refills_after_clear() {
    let tmp = tempfile::tempdir().unwrap();
    config::ensure_default_short_name(tmp.path(), "my-machine").unwrap();
    config::set_short_name(tmp.path(), None).unwrap();
    assert!(config::load_or_create(tmp.path())
        .unwrap()
        .device
        .short_name
        .is_none());

    let refilled = config::ensure_default_short_name(tmp.path(), "my-machine").unwrap();
    assert_eq!(refilled.as_deref(), Some("my-machine"), "清空后再次调用应填回");
}

#[test]
fn ensure_default_short_name_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let first = config::ensure_default_short_name(tmp.path(), "my-machine").unwrap();
    let text_before = fs::read_to_string(tmp.path().join(CONFIG_FILE_NAME)).unwrap();

    let second = config::ensure_default_short_name(tmp.path(), "my-machine").unwrap();
    assert_eq!(first, second);
    let text_after = fs::read_to_string(tmp.path().join(CONFIG_FILE_NAME)).unwrap();
    assert_eq!(text_before, text_after, "二次调用不应重写文件");
}

#[test]
fn long_term_token_created_persisted_and_repaired() {
    let tmp = tempfile::tempdir().unwrap();
    let first = config::load_or_create(tmp.path()).unwrap();
    let token = first.device.long_term_token.clone();
    assert_eq!(token.len(), 64, "32 字节 → 64 位十六进制");
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(first.notice.is_none());

    let second = config::load_or_create(tmp.path()).unwrap();
    assert_eq!(second.device.long_term_token, token, "跨加载应沿用");

    // 移除该键 → 视为缺省补全（生成新值写回、不算损坏、无备份）
    let path = tmp.path().join(CONFIG_FILE_NAME);
    let text = fs::read_to_string(&path).unwrap();
    let cleaned: String = text
        .lines()
        .filter(|line| !line.starts_with("long_term_token"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, cleaned).unwrap();

    let third = config::load_or_create(tmp.path()).unwrap();
    assert_ne!(third.device.long_term_token, token, "应生成新 token");
    assert!(third.notice.is_none(), "缺字段不算损坏");
    let backups = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".bak-"))
        .count();
    assert_eq!(backups, 0, "不应产生备份");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(&third.device.long_term_token), "新值应写回");
}

#[test]
fn peers_parsed_with_tolerance_and_file_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(CONFIG_FILE_NAME);
    let content = format!(
        "# 注释\n[device]\nuuid = \"{VALID_UUID}\"\nlong_term_token = \"{}\"\n\n[[peer]]\nuuid = \"{VALID_UUID_2}\"\nshort_name = \"dev-a\"\naddress = \"10.0.0.5\"\n\n[[peer]]\nuuid = \"00000000-0000-4000-8000-000000000002\"\naddress = \"10.0.0.6\"\nport = 40000\ntoken = \"tok\"\n\n[[peer]]\nshort_name = \"bad\"\naddress = \"10.0.0.7\"\n",
        "a".repeat(64)
    );
    fs::write(&path, &content).unwrap();

    let outcome = config::load_or_create(tmp.path()).unwrap();
    assert_eq!(outcome.peers.len(), 2, "缺 uuid 的条目应被跳过");
    assert_eq!(outcome.peers[0].short_name.as_deref(), Some("dev-a"));
    assert_eq!(outcome.peers[0].port, 37777, "端口缺省 37777");
    assert_eq!(outcome.peers[1].port, 40000);
    assert_eq!(outcome.peers[1].token, "tok");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        content,
        "读取不应改写文件"
    );
}

#[test]
fn workdir_read_and_default_none() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(CONFIG_FILE_NAME);
    fs::write(
        &path,
        format!(
            "[device]\nuuid = \"{VALID_UUID}\"\nlong_term_token = \"{}\"\nworkdir = \"/tmp\"\n",
            "a".repeat(64)
        ),
    )
    .unwrap();
    let outcome = config::load_or_create(tmp.path()).unwrap();
    assert_eq!(outcome.device.workdir.as_deref(), Some("/tmp"));

    let tmp2 = tempfile::tempdir().unwrap();
    let outcome = config::load_or_create(tmp2.path()).unwrap();
    assert!(outcome.device.workdir.is_none(), "缺省应为 None");
}

const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\nalice:x:1000:1000:Alice:/home/alice:/bin/bash\nbob:x:1001:1001:Bob:/home/bob:/bin/sh\n";

#[test]
fn caller_home_from_sudo_user_then_pkexec_uid() {
    let env = env_of(&[("SUDO_USER", "alice")]);
    assert_eq!(
        config::caller_home_from(&env, 0, PASSWD),
        Some(std::path::PathBuf::from("/home/alice"))
    );
    // SUDO_USER 缺省时看 PKEXEC_UID（数字 UID）
    let env = env_of(&[("PKEXEC_UID", "1001")]);
    assert_eq!(
        config::caller_home_from(&env, 0, PASSWD),
        Some(std::path::PathBuf::from("/home/bob"))
    );
    // 非 root 场景不解释（euid != 0）
    let env = env_of(&[("SUDO_USER", "alice")]);
    assert_eq!(config::caller_home_from(&env, 1000, PASSWD), None);
    // root / 未知用户 → None
    let env = env_of(&[("SUDO_USER", "root")]);
    assert_eq!(config::caller_home_from(&env, 0, PASSWD), None);
    let env = env_of(&[("SUDO_USER", "ghost")]);
    assert_eq!(config::caller_home_from(&env, 0, PASSWD), None);
}

#[test]
fn data_dir_ctx_prefers_xdg_then_caller_home_then_home() {
    let vars = env_of(&[("HOME", "/root"), ("XDG_CONFIG_HOME", "/xdg")]);
    let dir = config::data_dir_from_ctx(
        &vars,
        TargetOs::Unix,
        Some(std::path::Path::new("/home/alice")),
    )
    .unwrap();
    assert_eq!(dir, std::path::PathBuf::from("/xdg/agent-bridge"), "XDG 优先");

    let vars = env_of(&[("HOME", "/root")]);
    let dir = config::data_dir_from_ctx(
        &vars,
        TargetOs::Unix,
        Some(std::path::Path::new("/home/alice")),
    )
    .unwrap();
    assert_eq!(
        dir,
        std::path::PathBuf::from("/home/alice/.config/agent-bridge"),
        "提权时落在调用者目录"
    );

    let vars = env_of(&[("HOME", "/root")]);
    let dir = config::data_dir_from_ctx(&vars, TargetOs::Unix, None).unwrap();
    assert_eq!(dir, std::path::PathBuf::from("/root/.config/agent-bridge"));
}

#[test]
fn add_peer_appends_then_updates_idempotently() {
    use agent_bridge::config::Peer;

    let tmp = tempfile::tempdir().unwrap();
    let _ = config::load_or_create(tmp.path()).unwrap();
    let path = tmp.path().join(CONFIG_FILE_NAME);
    // 预置注释，验证写入不吞注释
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("# 顶部注释\n{text}")).unwrap();

    let first = Peer {
        uuid: VALID_UUID.to_string(),
        short_name: Some("dev-a".to_string()),
        address: "10.0.0.5".to_string(),
        port: 37777,
        token: "t1".to_string(),
    };
    config::add_peer(tmp.path(), &first).unwrap();
    let outcome = config::load_or_create(tmp.path()).unwrap();
    assert_eq!(outcome.peers.len(), 1);
    assert_eq!(outcome.peers[0].short_name.as_deref(), Some("dev-a"));

    // 幂等覆盖（同 uuid、去短名、换址换 token）
    let updated = Peer {
        uuid: VALID_UUID.to_string(),
        short_name: None,
        address: "10.0.0.6".to_string(),
        port: 40001,
        token: "t2".to_string(),
    };
    config::add_peer(tmp.path(), &updated).unwrap();
    let outcome = config::load_or_create(tmp.path()).unwrap();
    assert_eq!(outcome.peers.len(), 1, "同 uuid 不得重复添加");
    assert_eq!(outcome.peers[0].address, "10.0.0.6");
    assert_eq!(outcome.peers[0].port, 40001);
    assert_eq!(outcome.peers[0].token, "t2");
    assert!(outcome.peers[0].short_name.is_none());
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("# 顶部注释"), "注释应保留");
    assert!(!text.contains("dev-a"), "短名移除后不应残留");

    // 追加第二台
    let second = Peer {
        uuid: VALID_UUID_2.to_string(),
        short_name: Some("dev-b".to_string()),
        address: "10.0.0.7".to_string(),
        port: 37777,
        token: "t3".to_string(),
    };
    config::add_peer(tmp.path(), &second).unwrap();
    assert_eq!(config::load_or_create(tmp.path()).unwrap().peers.len(), 2);
}

#[test]
fn concurrent_first_loads_do_not_collide() {
    // 复现并锁定修复：三个线程对「尚不存在」的配置并发 load_or_create
    // （曾因临时文件名仅含 pid 而互抢 rename 报 ENOENT；见变更 ④ 实跑发现）
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let handles: Vec<_> = (0..3)
        .map(|_| {
            let dir = dir.clone();
            std::thread::spawn(move || config::load_or_create(&dir).map(|o| o.device.uuid))
        })
        .collect();
    for handle in handles {
        let uuid = handle.join().unwrap().expect("并发首启不应失败");
        assert!(uuid::Uuid::parse_str(&uuid).is_ok());
    }
    let final_uuid = config::load_or_create(&dir).unwrap().device.uuid;
    assert!(uuid::Uuid::parse_str(&final_uuid).is_ok());
    // 无临时文件残留
    let leftovers = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .count();
    assert_eq!(leftovers, 0, "不应残留临时文件");
}

#[test]
fn reset_long_term_token_persists_new_value() {
    let tmp = tempfile::tempdir().unwrap();
    let old = config::load_or_create(tmp.path()).unwrap().device.long_term_token;
    let new_token = config::reset_long_term_token(tmp.path()).unwrap();
    assert_ne!(old, new_token);
    assert_eq!(new_token.len(), 64);
    assert_eq!(
        config::read_long_term_token(tmp.path()).unwrap(),
        new_token
    );
    let text = fs::read_to_string(tmp.path().join(CONFIG_FILE_NAME)).unwrap();
    assert!(text.contains(&new_token));
}
