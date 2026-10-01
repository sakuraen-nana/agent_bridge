//! 管理员权限检测与提权重启（design D1）。
//!
//! 启动链路：已是管理员 → 直接继续；否则图形会话 + `pkexec` 可用 → 以待提权
//! 方式重启自身（本实例退出）；失败/不可得 → 受限模式（不退出，界面横幅提示）。

use std::path::{Path, PathBuf};

/// 提权状态（进入快照供界面展示）。
#[derive(Debug, Clone)]
pub struct ElevationStatus {
    /// 当前是否具备管理员权限。
    pub admin: bool,
    /// 受限模式原因 / 权限说明（进入面板「管理员权限」行）。
    pub detail: String,
}

/// 初始化入口的决策结果。
pub enum ElevationOutcome {
    /// 继续运行（携带权限状态）。
    Continue(ElevationStatus),
    /// 已发起提权重启：本实例应立即退出。
    Relaunching,
}

/// 启动时确保管理员权限。
pub fn ensure() -> ElevationOutcome {
    let env = |k: &str| std::env::var(k).ok();
    let elevated = is_admin();
    #[cfg(unix)]
    {
        decide_unix(&env, elevated)
    }
    #[cfg(windows)]
    {
        decide_windows(elevated)
    }
}

/// 当前进程是否具备管理员权限（两端统一；`is_elevated` crate 仅覆盖 Windows）。
pub fn is_admin() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(windows)]
    {
        is_elevated::is_elevated()
    }
}

/// 纯决策（Unix，供单测注入环境）：`pkexec_path` 由调用方解析。
fn decide_with(
    env: &dyn Fn(&str) -> Option<String>,
    elevated: bool,
    pkexec_path: Option<&Path>,
    exe: &Path,
    config_home: Option<&Path>,
) -> ElevationOutcome {
    if elevated {
        return ElevationOutcome::Continue(ElevationStatus {
            admin: true,
            detail: "已具备（root/管理员）".to_string(),
        });
    }
    if env("DISPLAY").filter(|v| !v.is_empty()).is_none()
        && env("WAYLAND_DISPLAY").filter(|v| !v.is_empty()).is_none()
    {
        return restricted("无图形会话（DISPLAY/WAYLAND_DISPLAY 未设置），无法发起提权请求");
    }
    let Some(pkexec) = pkexec_path else {
        return restricted("系统未提供 pkexec，无法发起提权请求");
    };
    match build_relaunch_plan(env, exe, pkexec, config_home) {
        Ok(plan) => match std::process::Command::new(&plan.program)
            .args(&plan.args)
            .status()
        {
            Ok(status) if status.success() => ElevationOutcome::Relaunching,
            Ok(status) => restricted(&format!(
                "提权请求被取消或失败（pkexec 退出码 {:?}）",
                status.code()
            )),
            Err(e) => restricted(&format!("无法调用 pkexec：{e}")),
        },
        Err(reason) => restricted(&reason),
    }
}

fn restricted(reason: &str) -> ElevationOutcome {
    ElevationOutcome::Continue(ElevationStatus {
        admin: false,
        detail: format!("受限模式：{reason}"),
    })
}

/// 提权重启命令（可单测的纯构造）。
pub struct LaunchPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// 构造 `pkexec env <白名单变量> XDG_CONFIG_HOME=<调用者> <exe>`。
pub fn build_relaunch_plan(
    env: &dyn Fn(&str) -> Option<String>,
    exe: &Path,
    pkexec: &Path,
    config_home: Option<&Path>,
) -> Result<LaunchPlan, String> {
    let mut args = vec!["env".to_string()];
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        if let Some(value) = env(key).filter(|v| !v.is_empty()) {
            args.push(format!("{key}={value}"));
        }
    }
    let Some(config_home) = config_home else {
        return Err("无法确定调用者数据目录（XDG 配置根缺失）".to_string());
    };
    args.push(format!("XDG_CONFIG_HOME={}", config_home.display()));
    args.push(exe.to_string_lossy().into_owned());
    Ok(LaunchPlan {
        program: pkexec.to_path_buf(),
        args,
    })
}

/// 在 PATH 中查找可执行文件（可单测）。
pub fn find_in_path(program: &str, path_var: Option<String>) -> Option<PathBuf> {
    let path = path_var?;
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
fn decide_unix(env: &dyn Fn(&str) -> Option<String>, elevated: bool) -> ElevationOutcome {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => return restricted(&format!("无法定位应用可执行文件：{e}")),
    };
    let pkexec = find_in_path("pkexec", std::env::var("PATH").ok());
    // 调用者的配置根 = 数据目录的父目录（此时未提权，解析结果即调用者目录）
    let config_home = crate::config::resolve_data_dir()
        .ok()
        .and_then(|dir| dir.parent().map(Path::to_path_buf));
    decide_with(env, elevated, pkexec.as_deref(), &exe, config_home.as_deref())
}

#[cfg(windows)]
fn decide_windows(elevated: bool) -> ElevationOutcome {
    if elevated {
        ElevationOutcome::Continue(ElevationStatus {
            admin: true,
            detail: "已具备（管理员）".to_string(),
        })
    } else {
        // 清单为 requireAdministrator：正常情况下不会走到这里（UAC 被拒则进程不会启动）
        restricted("未获得管理员权限（Windows 清单要求管理员启动；请通过正常入口启动并以 UAC 允许）")
    }
}
