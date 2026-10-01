//! `agent-bridge` 命令行入口（薄壳；实现见 `agent_bridge::cli`）。

fn main() {
    std::process::exit(agent_bridge::cli::run());
}
