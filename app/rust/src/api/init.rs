//! 桥接面：初始化快照与系统信息刷新（含服务端启动，design D11）。

use std::sync::{Mutex, OnceLock};

use crate::config;
use crate::identity;
use crate::server::{self, ServerConfig, ServerHandle};
use crate::version;

pub use crate::sysinfo_view::SystemSnapshot;

/// 服务端状态（信息面板 / 横幅展示用）。
#[derive(Debug, Clone)]
pub struct ServerSnapshot {
    /// 是否已启动。
    pub running: bool,
    /// 监听端口。
    pub port: u16,
    /// 启动失败时的可读原因。
    pub error: Option<String>,
}

/// 应用启动快照（信息面板的数据面）。
#[derive(Debug, Clone)]
pub struct AppSnapshot {
    /// 应用版本（新应用版本源）。
    pub version: String,
    /// 设备 UUID（持久）。
    pub uuid: String,
    /// 本机默认短名；`None` 表示未设置。
    pub short_name: Option<String>,
    /// 一次性界面提示（如配置损坏重建）；正常时为 `None`。
    pub notice: Option<String>,
    /// 系统信息。
    pub system: SystemSnapshot,
    /// 服务端状态。
    pub server: ServerSnapshot,
}

/// 进程级运行上下文（会话 token 与服务端句柄）。
struct AppRuntime {
    session_token: String,
    server: Mutex<Option<ServerHandle>>,
}

static APP_RUNTIME: OnceLock<AppRuntime> = OnceLock::new();

fn app_runtime() -> &'static AppRuntime {
    APP_RUNTIME.get_or_init(|| AppRuntime {
        session_token: identity::new_token(),
        server: Mutex::new(None),
    })
}

/// 确保服务端已启动（幂等；同一进程只启动一个实例）。
fn ensure_server(data_dir: &std::path::Path, workdir: Option<String>) -> ServerSnapshot {
    let runtime = app_runtime();
    let mut guard = runtime.server.lock().unwrap();
    if let Some(handle) = guard.as_ref() {
        return ServerSnapshot {
            running: true,
            port: handle.port(),
            error: None,
        };
    }
    // AGENT_BRIDGE_PORT：仅作命令行实跑的临时覆盖口（不面向用户文档）
    let port = std::env::var("AGENT_BRIDGE_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(config::DEFAULT_PORT);
    let server_config = ServerConfig::new(
        port,
        data_dir.to_path_buf(),
        runtime.session_token.clone(),
        workdir,
    );
    match server::start(server_config) {
        Ok(handle) => {
            let snapshot = ServerSnapshot {
                running: true,
                port: handle.port(),
                error: None,
            };
            *guard = Some(handle);
            snapshot
        }
        Err(e) => ServerSnapshot {
            running: false,
            port,
            error: Some(e.to_string()),
        },
    }
}

/// 初始化：解析数据目录、读取/创建配置，启动服务端，返回完整快照。
pub async fn app_init() -> anyhow::Result<AppSnapshot> {
    let data_dir = config::resolve_data_dir()?;
    let outcome = config::load_or_create(&data_dir)?;
    let server = ensure_server(&data_dir, outcome.device.workdir.clone());
    Ok(AppSnapshot {
        version: version::APP_VERSION.to_string(),
        uuid: outcome.device.uuid,
        short_name: outcome.device.short_name,
        notice: outcome.notice,
        system: crate::sysinfo_view::collect(),
        server,
    })
}

/// 仅刷新系统信息（面板的「刷新」动作；身份、配置与服务端状态不变）。
pub async fn refresh_system() -> anyhow::Result<SystemSnapshot> {
    Ok(crate::sysinfo_view::collect())
}
