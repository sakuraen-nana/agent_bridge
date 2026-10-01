//! 数据目录解析与配置文件（`config.toml`）读写（design D5/D6，变更 ② 起为 v2 契约）。
//!
//! - 数据目录：Linux `$XDG_CONFIG_HOME/agent-bridge`（缺省 `~/.config/agent-bridge`）、
//!   Windows `%APPDATA%\agent-bridge`；解析函数以「环境取值函数 + 目标 OS」为参数，
//!   便于测试注入与提权场景复用（提权后须按调用者用户解析）。
//! - 配置文件：UTF-8 TOML，`toml_edit` 最小侵入更新（保留注释与未知键）；
//!   Unix 上文件 0600、目录首次创建 0700；写入采用同目录临时文件 + 原子改名。
//! - v2 内容：`[device]`（uuid / short_name / long_term_token / workdir）与可选的
//!   `[[peer]]` 多段；缺 uuid 或 long_term_token 视为缺省补全（不算损坏）。
//! - 损坏（TOML 解析失败）：原名另存 `config.toml.bak-<时间戳>` 后重建，
//!   并在 `LoadOutcome.notice` 里带回供界面提示。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, value};

use crate::error::AppError;
use crate::identity;

/// 数据目录名（两平台相同的目录名）。
pub const APP_DIR_NAME: &str = "agent-bridge";

/// 配置文件名。
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// 默认服务端口。
pub const DEFAULT_PORT: u16 = 37777;

/// 目标操作系统分支（供测试注入覆盖两平台）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetOs {
    /// Linux / macOS 同规则（XDG）。
    Unix,
    /// Windows（APPDATA）。
    Windows,
}

/// 当前编译目标的 OS。
pub fn current_os() -> TargetOs {
    if cfg!(windows) {
        TargetOs::Windows
    } else {
        TargetOs::Unix
    }
}

/// 解析数据目录（依赖注入版）：`vars` 为环境变量取值函数，`os` 为目标平台。
pub fn data_dir_from(
    vars: &dyn Fn(&str) -> Option<String>,
    os: TargetOs,
) -> Result<PathBuf, AppError> {
    match os {
        TargetOs::Windows => {
            let appdata = vars("APPDATA").filter(|v| !v.is_empty());
            match appdata {
                Some(base) => Ok(PathBuf::from(base).join(APP_DIR_NAME)),
                None => Err(AppError::DataDirUnavailable(
                    "环境变量 APPDATA 未设置".to_string(),
                )),
            }
        }
        TargetOs::Unix => {
            // XDG 规范：XDG_CONFIG_HOME 须为绝对路径，否则忽略
            let xdg = vars("XDG_CONFIG_HOME").filter(|v| v.starts_with('/'));
            if let Some(base) = xdg {
                return Ok(PathBuf::from(base).join(APP_DIR_NAME));
            }
            let home = vars("HOME").filter(|v| !v.is_empty());
            match home {
                Some(home) => Ok(PathBuf::from(home).join(".config").join(APP_DIR_NAME)),
                None => Err(AppError::DataDirUnavailable(
                    "环境变量 HOME 未设置且未指定 XDG_CONFIG_HOME".to_string(),
                )),
            }
        }
    }
}

/// 解析数据目录（真实环境版）。
pub fn resolve_data_dir() -> Result<PathBuf, AppError> {
    data_dir_from(&|k| std::env::var(k).ok(), current_os())
}

/// 当前用户主目录（配置未指定 `workdir` 时的默认工作目录）。
pub fn user_home() -> Result<PathBuf, AppError> {
    let vars = |k: &str| std::env::var(k).ok();
    match current_os() {
        TargetOs::Windows => vars("USERPROFILE")
            .or_else(|| vars("HOME"))
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| AppError::DataDirUnavailable("环境变量 USERPROFILE 未设置".to_string())),
        TargetOs::Unix => vars("HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| AppError::DataDirUnavailable("环境变量 HOME 未设置".to_string())),
    }
}

/// 设备段（`[device]`）的读取结果。
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// 设备 UUID（持久化，应用正常流程不改变）。
    pub uuid: String,
    /// 本机默认短名；`None` 表示未设置。
    pub short_name: Option<String>,
    /// 长期 token（持久化凭据；仅手动重置）。
    pub long_term_token: String,
    /// 默认工作目录（配置原值）；`None` 表示缺省（取用户主目录）。
    pub workdir: Option<String>,
}

/// 对端设备条目（`[[peer]]`）。
#[derive(Debug, Clone)]
pub struct Peer {
    /// 对端设备 UUID（必填；缺失或非法的条目在读取时跳过）。
    pub uuid: String,
    /// 本机为该对端起的短名（可空）。
    pub short_name: Option<String>,
    /// 对端地址（主机名或 IP）。
    pub address: String,
    /// 对端服务端口（缺省 37777）。
    pub port: u16,
    /// 对端提供给我方的 token（可空；为空时 CLI 使用会报配置错误）。
    pub token: String,
}

/// 加载结果：设备配置 + 对端列表 + 可选的界面提示（配置损坏重建时为 `Some`）。
#[derive(Debug, Clone)]
pub struct LoadOutcome {
    pub device: DeviceConfig,
    pub peers: Vec<Peer>,
    pub notice: Option<String>,
}

/// 读取或创建配置（对外入口）。
pub fn load_or_create(data_dir: &Path) -> Result<LoadOutcome, AppError> {
    Ok(load_document(data_dir)?.1)
}

/// 设置（`Some`）或清空（`None`）本机短名并立即持久化。
///
/// `Some` 时先按 design D7 口径校验与规范化；非法输入返回可读错误、原值不变。
pub fn set_short_name(data_dir: &Path, name: Option<&str>) -> Result<DeviceConfig, AppError> {
    let (mut doc, _) = load_document(data_dir)?;
    let normalized = match name {
        Some(raw) => Some(identity::validate_short_name(raw)?),
        None => None,
    };
    match &normalized {
        Some(n) => {
            doc["device"]["short_name"] = value(n.as_str());
        }
        None => {
            if let Some(table) = doc["device"].as_table_mut() {
                table.remove("short_name");
            }
        }
    }
    write_document(&data_dir.join(CONFIG_FILE_NAME), &doc)?;
    Ok(device_config_from_doc(&doc, normalized))
}

/// 重置长期 token 并持久化（CLI `token reset`）；返回新值。
pub fn reset_long_term_token(data_dir: &Path) -> Result<String, AppError> {
    let token = identity::new_token();
    set_long_term_token(data_dir, &token)?;
    Ok(token)
}

/// 写入指定长期 token（持久化）。
pub fn set_long_term_token(data_dir: &Path, token: &str) -> Result<(), AppError> {
    let (mut doc, _) = load_document(data_dir)?;
    ensure_device_table(&mut doc);
    doc["device"]["long_term_token"] = value(token);
    write_document(&data_dir.join(CONFIG_FILE_NAME), &doc)
}

/// 读取长期 token（缺则按补全语义生成并写回）。
pub fn read_long_term_token(data_dir: &Path) -> Result<String, AppError> {
    Ok(load_document(data_dir)?.1.device.long_term_token)
}

/// 加载文档（内部）：返回（文档，加载结果）。文件缺失则创建，损坏则备份重建。
fn load_document(data_dir: &Path) -> Result<(DocumentMut, LoadOutcome), AppError> {
    ensure_data_dir(data_dir)?;
    let path = data_dir.join(CONFIG_FILE_NAME);

    if !path.exists() {
        let uuid = identity::new_uuid();
        let token = identity::new_token();
        let mut doc = DocumentMut::new();
        doc["device"] = device_item(&uuid, &token);
        write_document(&path, &doc)?;
        return Ok((
            doc,
            LoadOutcome {
                device: DeviceConfig {
                    uuid,
                    short_name: None,
                    long_term_token: token,
                    workdir: None,
                },
                peers: Vec::new(),
                notice: None,
            },
        ));
    }

    let text = fs::read_to_string(&path).map_err(|e| AppError::ConfigRead(e.to_string()))?;
    match text.parse::<DocumentMut>() {
        Ok(mut doc) => {
            // 缺 uuid / long_term_token：视为缺省补全（不算损坏，保留其余内容与注释）
            let mut repaired = false;
            let uuid = match device_uuid_from_doc(&doc) {
                Some(uuid) => uuid,
                None => {
                    let uuid = identity::new_uuid();
                    ensure_device_table(&mut doc);
                    doc["device"]["uuid"] = value(uuid.as_str());
                    repaired = true;
                    uuid
                }
            };
            let long_term_token = match device_long_term_token_from_doc(&doc) {
                Some(token) => token,
                None => {
                    let token = identity::new_token();
                    ensure_device_table(&mut doc);
                    doc["device"]["long_term_token"] = value(token.as_str());
                    repaired = true;
                    token
                }
            };
            if repaired {
                write_document(&path, &doc)?;
            }
            let short_name = device_short_name_from_doc(&doc);
            let workdir = device_workdir_from_doc(&doc);
            let peers = peers_from_doc(&doc);
            Ok((
                doc,
                LoadOutcome {
                    device: DeviceConfig {
                        uuid,
                        short_name,
                        long_term_token,
                        workdir,
                    },
                    peers,
                    notice: None,
                },
            ))
        }
        Err(parse_err) => {
            // 损坏：备份原文件（重名追加序号）后重建，并带回提示
            let backup = unique_backup_path(data_dir);
            fs::rename(&path, &backup)
                .map_err(|e| AppError::ConfigWrite(format!("备份损坏配置失败：{e}")))?;
            let uuid = identity::new_uuid();
            let token = identity::new_token();
            let mut doc = DocumentMut::new();
            doc["device"] = device_item(&uuid, &token);
            write_document(&path, &doc)?;
            let backup_name = backup
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok((
                doc,
                LoadOutcome {
                    device: DeviceConfig {
                        uuid,
                        short_name: None,
                        long_term_token: token,
                        workdir: None,
                    },
                    peers: Vec::new(),
                    notice: Some(format!(
                        "配置文件无法解析（{parse_err}），原文件已备份为 {backup_name}，并已重建默认配置",
                    )),
                },
            ))
        }
    }
}

/// 确保数据目录存在；首次创建时在 Unix 上置 0700（尽力而为，失败不阻断）。
fn ensure_data_dir(dir: &Path) -> Result<(), AppError> {
    if dir.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|e| AppError::DataDirIo(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

/// 原子写入：同目录临时文件（Unix 0600）→ `rename` 替换目标。
fn write_document(path: &Path, doc: &DocumentMut) -> Result<(), AppError> {
    let dir = path
        .parent()
        .ok_or_else(|| AppError::ConfigWrite("配置路径没有父目录".to_string()))?;
    let tmp = dir.join(format!("{CONFIG_FILE_NAME}.tmp-{}", std::process::id()));

    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&tmp)
        .map_err(|e| AppError::ConfigWrite(e.to_string()))?;
    file.write_all(doc.to_string().as_bytes())
        .map_err(|e| AppError::ConfigWrite(e.to_string()))?;
    let _ = file.sync_all();
    drop(file);

    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::ConfigWrite(e.to_string())
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // rename 后权限随临时文件保留，这里再显式设一次以防由其他路径写入
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// 构造新的 `[device]` 表（以 `Item` 形态便于直接赋值）。
fn device_item(uuid: &str, long_term_token: &str) -> Item {
    let mut table = Table::new();
    table["uuid"] = value(uuid);
    table["long_term_token"] = value(long_term_token);
    Item::Table(table)
}

/// 确保 `[device]` 是表（非表时替换为空表，原内容本就无 uuid 可读）。
fn ensure_device_table(doc: &mut DocumentMut) {
    if !doc.get("device").map(Item::is_table).unwrap_or(false) {
        doc["device"] = Item::Table(Table::new());
    }
}

/// 从文档读 uuid：必须是可解析的 UUID，否则视为缺失。
fn device_uuid_from_doc(doc: &DocumentMut) -> Option<String> {
    doc.get("device")?
        .get("uuid")?
        .as_str()
        .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        .map(str::to_string)
}

/// 从文档读长期 token：非空即有效。
fn device_long_term_token_from_doc(doc: &DocumentMut) -> Option<String> {
    doc.get("device")?
        .get("long_term_token")?
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// 从文档读短名：原样取值（手工编辑以文件为准），仅空串视为未设置。
fn device_short_name_from_doc(doc: &DocumentMut) -> Option<String> {
    doc.get("device")?
        .get("short_name")?
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// 从文档读默认工作目录：非空即有效（路径有效性在使用时校验）。
fn device_workdir_from_doc(doc: &DocumentMut) -> Option<String> {
    doc.get("device")?
        .get("workdir")?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 依据文档组装设备配置（指定短名覆盖文档取值，用于 set_short_name 回读）。
fn device_config_from_doc(doc: &DocumentMut, short_name: Option<String>) -> DeviceConfig {
    DeviceConfig {
        uuid: device_uuid_from_doc(doc).unwrap_or_default(),
        short_name,
        long_term_token: device_long_term_token_from_doc(doc).unwrap_or_default(),
        workdir: device_workdir_from_doc(doc),
    }
}

/// 从文档解析 `[[peer]]` 多段；缺 uuid/address 或 uuid 非法的条目跳过（不改写文件）。
pub fn peers_from_doc(doc: &DocumentMut) -> Vec<Peer> {
    let mut out = Vec::new();
    let Some(tables) = doc.get("peer").and_then(Item::as_array_of_tables) else {
        return out;
    };
    for table in tables.iter() {
        let uuid = table
            .get("uuid")
            .and_then(|v| v.as_str())
            .filter(|s| uuid::Uuid::parse_str(s).is_ok())
            .map(str::to_string);
        let address = table
            .get("address")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let (Some(uuid), Some(address)) = (uuid, address) else {
            continue;
        };
        let short_name = table
            .get("short_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string);
        let port = table
            .get("port")
            .and_then(|v| v.as_integer())
            .and_then(|n| u16::try_from(n).ok())
            .filter(|p| *p > 0)
            .unwrap_or(DEFAULT_PORT);
        let token = table
            .get("token")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        out.push(Peer {
            uuid,
            short_name,
            address,
            port,
            token,
        });
    }
    out
}

/// 生成唯一的备份路径：`config.toml.bak-<时间戳>`（重名追加 `-N`）。
fn unique_backup_path(data_dir: &Path) -> PathBuf {
    let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
    let base = format!("{CONFIG_FILE_NAME}.bak-{ts}");
    let mut candidate = data_dir.join(&base);
    let mut n = 1;
    while candidate.exists() {
        candidate = data_dir.join(format!("{base}-{n}"));
        n += 1;
    }
    candidate
}
