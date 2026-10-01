//! 开机自启（design D5）：状态 = 系统机制现状；仅作用于调用者用户范围。
//!
//! - Linux：`~/.config/autostart/agent-bridge.desktop`（Exec 指向本应用；提权由
//!   应用启动逻辑处理）；写入用「临时文件 + rename」幂等。
//! - Windows：`HKCU\...\Run` 项（经 `reg`，命令执行走 [`CommandRunner`] 注入点）。

use std::path::PathBuf;

use crate::config;
use crate::error::AppError;
use crate::firewall::CommandRunner;

/// Linux 自启文件名。
pub const AUTOSTART_FILE_NAME: &str = "agent-bridge.desktop";
/// Windows 自启注册表键。
pub const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
/// Windows 自启值名。
pub const WINDOWS_VALUE_NAME: &str = "agent-bridge";

/// 自启状态（进入界面）。
#[derive(Debug, Clone)]
pub struct AutostartStatus {
    pub enabled: bool,
    pub mechanism: String,
    pub detail: String,
}

/// 查询自启状态。
pub fn status() -> Result<AutostartStatus, AppError> {
    #[cfg(windows)]
    {
        status_windows(&crate::firewall::SystemRunner)
    }
    #[cfg(not(windows))]
    {
        status_unix()
    }
}

/// 开启自启（幂等）。
pub fn enable() -> Result<AutostartStatus, AppError> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| AppError::Server(e.to_string()))?;
        enable_windows(&crate::firewall::SystemRunner, &exe)?;
        status_windows(&crate::firewall::SystemRunner)
    }
    #[cfg(not(windows))]
    {
        enable_unix()
    }
}

/// 关闭自启（幂等）。
pub fn disable() -> Result<AutostartStatus, AppError> {
    #[cfg(windows)]
    {
        let _ = crate::firewall::SystemRunner.run(
            "reg",
            &["delete", WINDOWS_RUN_KEY, "/v", WINDOWS_VALUE_NAME, "/f"],
        );
        status_windows(&crate::firewall::SystemRunner)
    }
    #[cfg(not(windows))]
    {
        disable_unix()
    }
}

// ---------- Linux ----------

#[cfg(not(windows))]
fn desktop_path(home: &std::path::Path) -> PathBuf {
    home.join(".config").join("autostart").join(AUTOSTART_FILE_NAME)
}

#[cfg(not(windows))]
fn desktop_entry(exec: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=agent-bridge\nComment=局域网远程执行桥（提权由应用启动时自行请求）\nExec={exec}\nX-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(not(windows))]
fn status_unix() -> Result<AutostartStatus, AppError> {
    status_in(&config::user_home()?)
}

/// 以指定家目录查询（可单测）。
#[cfg(any(not(windows), test))]
pub fn status_in(home: &std::path::Path) -> Result<AutostartStatus, AppError> {
    let path = desktop_path(home);
    Ok(AutostartStatus {
        enabled: path.is_file(),
        mechanism: "autostart .desktop".to_string(),
        detail: path.display().to_string(),
    })
}

#[cfg(not(windows))]
fn enable_unix() -> Result<AutostartStatus, AppError> {
    let exe = std::env::current_exe().map_err(|e| AppError::Server(e.to_string()))?;
    enable_in(&config::user_home()?, &exe)
}

/// 以指定家目录启用（可单测）。
#[cfg(any(not(windows), test))]
pub fn enable_in(home: &std::path::Path, exe: &std::path::Path) -> Result<AutostartStatus, AppError> {
    let path = desktop_path(home);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Server(e.to_string()))?;
    }
    let content = desktop_entry(&exe.to_string_lossy());
    // 原子替换写入（幂等）
    let tmp = path.with_extension("desktop.tmp");
    std::fs::write(&tmp, content).map_err(|e| AppError::Server(e.to_string()))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        AppError::Server(e.to_string())
    })?;
    status_in(home)
}

#[cfg(not(windows))]
fn disable_unix() -> Result<AutostartStatus, AppError> {
    disable_in(&config::user_home()?)
}

/// 以指定家目录停用（可单测）。
#[cfg(any(not(windows), test))]
pub fn disable_in(home: &std::path::Path) -> Result<AutostartStatus, AppError> {
    let path = desktop_path(home);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| AppError::Server(e.to_string()))?;
    }
    status_in(home)
}

// ---------- Windows（经注入点，便于单测输出解析） ----------

#[cfg(windows)]
fn status_windows(runner: &dyn CommandRunner) -> Result<AutostartStatus, AppError> {
    windows_status_with(runner).map_err(AppError::Server)
}

pub fn windows_status_with(runner: &dyn CommandRunner) -> Result<AutostartStatus, String> {
    match runner.run("reg", &["query", WINDOWS_RUN_KEY, "/v", WINDOWS_VALUE_NAME]) {
        Ok((0, _)) => Ok(AutostartStatus {
            enabled: true,
            mechanism: r"HKCU\...\Run".to_string(),
            detail: format!(r"{WINDOWS_RUN_KEY} → {WINDOWS_VALUE_NAME}"),
        }),
        _ => Ok(AutostartStatus {
            enabled: false,
            mechanism: r"HKCU\...\Run".to_string(),
            detail: format!(r"{WINDOWS_RUN_KEY} → {WINDOWS_VALUE_NAME}（未设置）"),
        }),
    }
}

#[cfg(windows)]
fn enable_windows(runner: &dyn CommandRunner, exe: &std::path::Path) -> Result<(), AppError> {
    windows_enable_with(runner, exe).map_err(AppError::Server)
}

/// 经注入执行器写入 Run 项（可单测）。
pub fn windows_enable_with(runner: &dyn CommandRunner, exe: &std::path::Path) -> Result<(), String> {
    let value = format!("\"{}\"", exe.to_string_lossy());
    let (code, out) = runner.run(
        "reg",
        &[
            "add",
            WINDOWS_RUN_KEY,
            "/v",
            WINDOWS_VALUE_NAME,
            "/t",
            "REG_SZ",
            "/d",
            &value,
            "/f",
        ],
    )?;
    if code == 0 {
        Ok(())
    } else {
        Err(format!("写入自启注册表失败：{out}"))
    }
}
