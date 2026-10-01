//! 数据目录解析与配置文件（`config.toml`）读写（design D5/D6）。
//!
//! - 数据目录：Linux `$XDG_CONFIG_HOME/agent-bridge`（缺省 `~/.config/agent-bridge`）、
//!   Windows `%APPDATA%\agent-bridge`；解析函数以「环境取值函数 + 目标 OS」为参数，
//!   便于测试注入与变更 ③ 提权场景复用（提权后须按调用者用户解析）。
//! - 配置文件：UTF-8 TOML，`toml_edit` 最小侵入更新（保留注释与未知键）；
//!   Unix 上文件 0600、目录首次创建 0700；写入采用同目录临时文件 + 原子改名。
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

/// 设备段（`[device]`）的读取结果。
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// 设备 UUID（持久化，应用正常流程不改变）。
    pub uuid: String,
    /// 本机默认短名；`None` 表示未设置。
    pub short_name: Option<String>,
}

/// 加载结果：设备配置 + 可选的界面提示（配置损坏重建时为 `Some`）。
#[derive(Debug, Clone)]
pub struct LoadOutcome {
    pub device: DeviceConfig,
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
    Ok(DeviceConfig {
        uuid: device_uuid_from_doc(&doc).unwrap_or_default(),
        short_name: normalized,
    })
}

/// 加载文档（内部）：返回（文档，加载结果）。文件缺失则创建，损坏则备份重建。
fn load_document(data_dir: &Path) -> Result<(DocumentMut, LoadOutcome), AppError> {
    ensure_data_dir(data_dir)?;
    let path = data_dir.join(CONFIG_FILE_NAME);

    if !path.exists() {
        let uuid = identity::new_uuid();
        let mut doc = DocumentMut::new();
        doc["device"] = device_item(&uuid);
        write_document(&path, &doc)?;
        return Ok((
            doc,
            LoadOutcome {
                device: DeviceConfig {
                    uuid,
                    short_name: None,
                },
                notice: None,
            },
        ));
    }

    let text = fs::read_to_string(&path).map_err(|e| AppError::ConfigRead(e.to_string()))?;
    match text.parse::<DocumentMut>() {
        Ok(mut doc) => {
            let uuid = match device_uuid_from_doc(&doc) {
                Some(uuid) => uuid,
                None => {
                    // 合法 TOML 但缺 uuid（或值非法）：视为缺省补全，保留其余内容（design D6）
                    let uuid = identity::new_uuid();
                    ensure_device_table(&mut doc);
                    doc["device"]["uuid"] = value(uuid.as_str());
                    write_document(&path, &doc)?;
                    uuid
                }
            };
            let short_name = device_short_name_from_doc(&doc);
            Ok((
                doc,
                LoadOutcome {
                    device: DeviceConfig { uuid, short_name },
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
            let mut doc = DocumentMut::new();
            doc["device"] = device_item(&uuid);
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
                    },
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
fn device_item(uuid: &str) -> Item {
    let mut table = Table::new();
    table["uuid"] = value(uuid);
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

/// 从文档读短名：原样取值（手工编辑以文件为准），仅空串视为未设置。
fn device_short_name_from_doc(doc: &DocumentMut) -> Option<String> {
    doc.get("device")?
        .get("short_name")?
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
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
