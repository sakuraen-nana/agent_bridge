//! 开机自启：Linux 家目录往返与 Windows 注册表注入回放（design D5）。

use std::collections::HashMap;
use std::path::Path;

use agent_bridge::autostart::{disable_in, enable_in, status_in, windows_enable_with, windows_status_with};
use agent_bridge::firewall::CommandRunner;

struct MockRunner {
    responses: HashMap<String, Result<(i32, String), String>>,
}

impl CommandRunner for MockRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<(i32, String), String> {
        let key = format!("{program} {}", args.join(" "));
        self.responses
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Err(format!("command not found: {key}")))
    }
}

#[cfg(not(windows))]
#[test]
fn linux_enable_disable_roundtrip_is_idempotent() {
    let home = tempfile::tempdir().unwrap();
    let exe = Path::new("/opt/agent-bridge/app");

    assert!(!status_in(home.path()).unwrap().enabled);

    let after_enable = enable_in(home.path(), exe).unwrap();
    assert!(after_enable.enabled);
    let file = home.path().join(".config/autostart/agent-bridge.desktop");
    assert!(file.is_file());
    let content = std::fs::read_to_string(&file).unwrap();
    assert!(content.contains("Exec=/opt/agent-bridge/app"), "{content}");
    assert!(content.contains("[Desktop Entry]"));

    // 幂等：重复启用仍是同一份
    let again = enable_in(home.path(), exe).unwrap();
    assert!(again.enabled);
    let entries = std::fs::read_dir(home.path().join(".config/autostart"))
        .unwrap()
        .count();
    assert_eq!(entries, 1, "重复启用不得产生多份");

    let after_disable = disable_in(home.path()).unwrap();
    assert!(!after_disable.enabled);
    assert!(!file.exists());
    // 幂等：重复停用不报错
    assert!(!disable_in(home.path()).unwrap().enabled);
}

#[test]
fn windows_status_and_enable_via_injected_runner() {
    let missing = MockRunner {
        responses: HashMap::new(),
    };
    assert!(!windows_status_with(&missing).unwrap().enabled);

    let present = MockRunner {
        responses: [(
            "reg query HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run /v agent-bridge"
                .to_string(),
            Ok((0, "agent-bridge    REG_SZ    \"C:\\app\\agent_bridge_app.exe\"".to_string())),
        )]
        .into_iter()
        .collect(),
    };
    assert!(windows_status_with(&present).unwrap().enabled);

    // 写入：校验参数形状与带引号的 exe 路径
    let mut responses = HashMap::new();
    let expected_key = "reg add HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run /v agent-bridge /t REG_SZ /d \"C:\\app\\agent_bridge_app.exe\" /f".to_string();
    responses.insert(expected_key, Ok((0, "The operation completed successfully.".to_string())));
    let adding = MockRunner { responses };
    windows_enable_with(&adding, Path::new(r"C:\app\agent_bridge_app.exe")).unwrap();
}
