//! 服务端共享状态与配置（design D2/D3）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::config;
use crate::error::AppError;

use super::log::RequestLog;

/// 服务端启动配置。
pub struct ServerConfig {
    /// 监听端口；0 表示由系统分配（测试用）。
    pub port: u16,
    /// 数据目录（配置与日志所在）。
    pub data_dir: PathBuf,
    /// 本次会话 token（仅存内存）。
    pub session_token: String,
    /// 配置中默认工作目录的原值；`None` 表示取用户主目录。
    pub workdir: Option<String>,
    /// 日志轮转阈值（默认 1 MiB；测试可调小）。
    pub log_max_bytes: u64,
    /// 配对请求等待上限（秒；默认 120，测试可调小）。
    pub pair_timeout_secs: u64,
}

impl ServerConfig {
    pub fn new(
        port: u16,
        data_dir: PathBuf,
        session_token: String,
        workdir: Option<String>,
    ) -> Self {
        Self {
            port,
            data_dir,
            session_token,
            workdir,
            log_max_bytes: 1024 * 1024,
            pair_timeout_secs: crate::pairing::PAIRING_TIMEOUT_SECS,
        }
    }
}

/// 服务端运行期状态（所有 handler 共享）。
pub struct ServerState {
    /// 实际绑定端口。
    pub port: u16,
    pub data_dir: PathBuf,
    pub session_token: String,
    pub workdir: PathBuf,
    pub started_at: String,
    /// 配对请求等待上限（秒）。
    pub pair_timeout_secs: u64,
    request_seq: AtomicU64,
    pairing_seq: AtomicU64,
    log: Mutex<RequestLog>,
    long_term_cache: Mutex<LongTermCache>,
    device_cache: Mutex<DeviceCache>,
    /// 发现功能的最近活跃设备表（design D1）。
    pub discovery_table: Mutex<crate::discovery::DiscoveryTable>,
    /// 发现功能的不可用原因（UDP 端口占用等；None 表示可用）。
    pub discovery_error: Mutex<Option<String>>,
    /// 当前待决的配对请求（单条；design D2）。
    pub pairing: Mutex<Option<crate::pairing::Pending>>,
}

/// 长期 token 缓存：按配置文件 mtime 刷新（CLI 端重置可即时生效，无需重启服务）。
struct LongTermCache {
    mtime: Option<SystemTime>,
    token: String,
}

/// 设备身份缓存（uuid / 短名）。
struct DeviceCache {
    mtime: Option<SystemTime>,
    uuid: String,
    short_name: Option<String>,
}

impl ServerState {
    pub fn new(config: &ServerConfig, bound_port: u16) -> Result<Self, AppError> {
        let workdir = match config.workdir.as_deref() {
            Some(raw) => PathBuf::from(raw),
            None => config::user_home()?,
        };
        let log = RequestLog::open(&config.data_dir, config.log_max_bytes)?;
        let initial_token = config::read_long_term_token(&config.data_dir).unwrap_or_default();
        let mtime = config_file_mtime(&config.data_dir);
        Ok(Self {
            port: bound_port,
            data_dir: config.data_dir.clone(),
            session_token: config.session_token.clone(),
            workdir,
            started_at: chrono::Local::now()
                .format("%Y-%m-%d %H:%M:%S %:z")
                .to_string(),
            pair_timeout_secs: config.pair_timeout_secs,
            request_seq: AtomicU64::new(0),
            pairing_seq: AtomicU64::new(0),
            log: Mutex::new(log),
            long_term_cache: Mutex::new(LongTermCache {
                mtime,
                token: initial_token,
            }),
            device_cache: Mutex::new(DeviceCache {
                mtime: None,
                uuid: String::new(),
                short_name: None,
            }),
            discovery_table: Mutex::new(crate::discovery::DiscoveryTable::default()),
            discovery_error: Mutex::new(None),
            pairing: Mutex::new(None),
        })
    }

    /// 请求序号（1 起、递增；仅用于日志对照）。
    pub fn next_request_id(&self) -> u64 {
        self.request_seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// 配对请求序号。
    pub fn next_pairing_id(&self) -> u64 {
        self.pairing_seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// 当前长期 token（按配置 mtime 刷新缓存）。
    pub fn long_term_token(&self) -> String {
        let mtime = config_file_mtime(&self.data_dir);
        let mut cache = self.long_term_cache.lock().unwrap();
        if cache.mtime != mtime {
            if let Ok(token) = config::read_long_term_token(&self.data_dir) {
                cache.token = token;
            }
            cache.mtime = mtime;
        }
        cache.token.clone()
    }

    /// 当前设备身份（uuid、短名；按配置 mtime 刷新缓存）。
    pub fn device_snapshot(&self) -> (String, Option<String>) {
        let mtime = config_file_mtime(&self.data_dir);
        let mut cache = self.device_cache.lock().unwrap();
        if cache.mtime != mtime {
            if let Ok(outcome) = config::load_or_create(&self.data_dir) {
                cache.uuid = outcome.device.uuid;
                cache.short_name = outcome.device.short_name;
            }
            cache.mtime = mtime;
        }
        (cache.uuid.clone(), cache.short_name.clone())
    }

    /// 请求留痕（持锁写入；失败不阻断业务）。
    pub fn log(&self) -> std::sync::MutexGuard<'_, RequestLog> {
        self.log.lock().unwrap()
    }
}

fn config_file_mtime(data_dir: &Path) -> Option<SystemTime> {
    std::fs::metadata(data_dir.join(config::CONFIG_FILE_NAME))
        .and_then(|m| m.modified())
        .ok()
}
