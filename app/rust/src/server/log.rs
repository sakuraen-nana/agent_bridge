//! 请求留痕：数据目录 `server.log`，1 MiB 轮转（design D8）。
//!
//! 面向人类可读：请求行 + 缩进明细；token 一律脱敏（只记状态与片段）；
//! exec 输出内容与 download 文件内容不写入日志。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::AppError;

/// 追加式请求日志。
pub struct RequestLog {
    path: PathBuf,
    rotated_path: PathBuf,
    file: Option<File>,
    max_bytes: u64,
    written: u64,
}

impl RequestLog {
    pub fn open(data_dir: &Path, max_bytes: u64) -> Result<Self, AppError> {
        let path = data_dir.join("server.log");
        let rotated_path = data_dir.join("server.log.1");
        let file = open_append(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            rotated_path,
            file: Some(file),
            max_bytes,
            written,
        })
    }

    fn rotate_if_needed(&mut self) -> Result<(), AppError> {
        if self.written < self.max_bytes {
            return Ok(());
        }
        // 先关闭句柄（Windows 上 rename 要求无占用），再轮转并重开
        self.file.take();
        let _ = fs::remove_file(&self.rotated_path);
        fs::rename(&self.path, &self.rotated_path)
            .map_err(|e| AppError::Server(format!("日志轮转失败：{e}")))?;
        self.file = Some(open_append(&self.path)?);
        self.written = 0;
        Ok(())
    }

    /// 写一行（失败静默：留痕不阻断业务）。
    pub fn line(&mut self, text: &str) {
        let _ = self.try_line(text);
    }

    fn try_line(&mut self, text: &str) -> Result<(), AppError> {
        self.rotate_if_needed()?;
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| AppError::Server("日志句柄缺失".to_string()))?;
        let mut buf = String::with_capacity(text.len() + 1);
        buf.push_str(text);
        buf.push('\n');
        file.write_all(buf.as_bytes())
            .map_err(|e| AppError::Server(e.to_string()))?;
        let _ = file.flush();
        self.written += buf.len() as u64;
        Ok(())
    }

    /// 请求行：`[时间] #id 方法 路径 · 来自 来源 · token=状态`
    pub fn request_line(
        &mut self,
        id: u64,
        method: &str,
        path: &str,
        source: &str,
        token_state: &str,
    ) {
        self.line(&format!(
            "[{}] #{id} {method} {path} · 来自 {source} · token={token_state}",
            now()
        ));
    }

    /// 明细行（缩进）。
    pub fn detail(&mut self, text: &str) {
        self.line(&format!("    {text}"));
    }
}

/// 当前本地时间（日志前缀）。
pub fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// token 脱敏片段（前 4 位 + 省略号）。
pub fn mask_token(token: &str) -> String {
    let head: String = token.chars().take(4).collect();
    format!("{head}…")
}

fn open_append(path: &Path) -> Result<File, AppError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| AppError::Server(format!("打开日志失败：{e}")))
}
