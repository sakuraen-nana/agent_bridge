//! 设备 CLI（design D9/D10）：`agent-bridge` 子命令、设备寻址与退出码约定。
//!
//! 退出码：0 成功（exec 时为远端退出码）／1 业务失败／2 本地配置与用法错误（含
//! 找不到设备、短名冲突）／3 网络连接失败或流式中断／4 token 被拒（404）。

pub mod client;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::config::{self, Peer};
use crate::identity::short_name_compare_key;
use crate::peers::{is_conflicted_key, resolve_peer};
use crate::version;

/// CLI 失败分类（映射退出码）。
pub enum CliFailure {
    /// 本地配置与用法错误（退出码 2）。
    Config(String),
    /// 业务失败（退出码 1）。
    Business(String),
    /// 网络连接失败或流式中断（退出码 3）。
    Network(String),
    /// token 被拒（退出码 4）。
    TokenRejected(String),
}

#[derive(Parser)]
#[command(
    name = "agent-bridge",
    version = version::APP_VERSION,
    about = "agent-bridge 客户端命令：经局域网操作设备"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 列出配置中的设备（短名、UUID、地址与短名冲突标记）
    Peers,
    /// 校验并打印目标设备信息
    Hello {
        /// 设备短名或 UUID
        device: String,
    },
    /// 远程执行命令（流式输出；远端退出码即本命令退出码）
    Exec {
        /// 设备短名或 UUID
        device: String,
        /// 要执行的整条 shell 命令
        command: String,
        /// 超时秒数（缺省 1800）
        #[arg(long)]
        timeout: Option<u64>,
    },
    /// 下载远程文件（缺省保存为当前目录下的同名文件）
    Download {
        /// 设备短名或 UUID
        device: String,
        /// 远程文件路径
        path: String,
        /// 保存到指定本地路径
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// 本机长期 token 管理
    Token {
        #[command(subcommand)]
        action: TokenAction,
    },
}

#[derive(Subcommand)]
enum TokenAction {
    /// 显示本机长期 token
    Show,
    /// 重置本机长期 token（打印新值；旧值立即失效）
    Reset,
}

/// CLI 入口：返回进程退出码。
pub fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            // --help / --version 走 stdout（视为成功）；用法错误走 stderr（按配置/用法错误）
            return if e.use_stderr() { 2 } else { 0 };
        }
    };
    match execute(cli) {
        Ok(code) => code,
        Err(failure) => {
            let (message, code) = match failure {
                CliFailure::Config(message) => (message, 2),
                CliFailure::Business(message) => (message, 1),
                CliFailure::Network(message) => (message, 3),
                CliFailure::TokenRejected(message) => (message, 4),
            };
            eprintln!("agent-bridge: {message}");
            code
        }
    }
}

fn execute(cli: Cli) -> Result<i32, CliFailure> {
    let data_dir =
        config::resolve_data_dir().map_err(|e| CliFailure::Config(e.to_string()))?;
    let outcome =
        config::load_or_create(&data_dir).map_err(|e| CliFailure::Config(e.to_string()))?;

    match cli.command {
        Command::Token { action } => token_command(&data_dir, action),
        Command::Peers => {
            print_peers(&outcome.peers);
            Ok(0)
        }
        Command::Hello { device } => {
            let peer = lookup(&outcome.peers, &device)?;
            let runtime = client::runtime()?;
            runtime.block_on(client::hello(peer))
        }
        Command::Exec {
            device,
            command,
            timeout,
        } => {
            let peer = lookup(&outcome.peers, &device)?;
            let runtime = client::runtime()?;
            runtime.block_on(client::exec(peer, &command, timeout))
        }
        Command::Download { device, path, out } => {
            let peer = lookup(&outcome.peers, &device)?;
            let runtime = client::runtime()?;
            runtime.block_on(client::download(peer, &path, out))
        }
    }
}

fn token_command(data_dir: &Path, action: TokenAction) -> Result<i32, CliFailure> {
    match action {
        TokenAction::Show => {
            let outcome = config::load_or_create(data_dir)
                .map_err(|e| CliFailure::Config(e.to_string()))?;
            println!("{}", outcome.device.long_term_token);
            Ok(0)
        }
        TokenAction::Reset => {
            let token = config::reset_long_term_token(data_dir)
                .map_err(|e| CliFailure::Config(e.to_string()))?;
            println!("{token}");
            eprintln!("（长期 token 已重置，旧值立即失效）");
            Ok(0)
        }
    }
}

/// 以短名 / UUID 查找设备（含 token 缺失检查）。
fn lookup<'a>(peers: &'a [Peer], device: &str) -> Result<&'a Peer, CliFailure> {
    let peer = resolve_peer(peers, device).map_err(|e| CliFailure::Config(e.to_string()))?;
    if peer.token.trim().is_empty() {
        return Err(CliFailure::Config(format!(
            "设备「{}」（{}）未配置 token——请重新配对或在配置中补充",
            peer.short_name.as_deref().unwrap_or("未命名"),
            peer.uuid
        )));
    }
    Ok(peer)
}

fn print_peers(peers: &[Peer]) {
    if peers.is_empty() {
        println!("配置中没有设备（可手工添加 [[peer]] 段，或等待配对功能）");
        return;
    }
    for peer in peers {
        let name = match &peer.short_name {
            Some(name) => {
                let key = short_name_compare_key(name);
                if is_conflicted_key(peers, &key) {
                    format!("{name}（短名冲突，无效；请改名或改用 UUID）")
                } else {
                    name.clone()
                }
            }
            None => "（未命名）".to_string(),
        };
        println!("{name}\t{}\t{}:{}", peer.uuid, peer.address, peer.port);
    }
}
