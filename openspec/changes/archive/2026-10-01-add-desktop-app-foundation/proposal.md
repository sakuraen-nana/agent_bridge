## Why

agent-bridge 目前是「整目录拷贝 + 任意 Python 3」的脚本工具，能力上限已显现：跨平台体验依赖目标机的 Python 环境、权限与防火墙需人工处理、设备之间没有身份，配对与多设备配置管理无从谈起。经与用户确认，工具将演进为 **Flutter 桌面 GUI + Rust 核心**的跨平台应用（目标 Windows / Linux；暂不含 macOS 与移动端，代码保持可移植），以「子目录并行、逐步替换」迁移——Python 版冻结（仅修致命缺陷），新应用按五个变更依次落地：① 基座与设备身份（本次）→ ② 服务端核心与 CLI → ③ 权限与防火墙 → ④ 发现与配对 → ⑤ 打包发布。

本次交付第一步：新应用基座（子项目脚手架与构建链路）、设备身份（UUID 与本机默认短名）与启动信息面板，并把仓库级文档与规则改写为「过渡期双实现」形态，使后续四个变更有可依的规格与规则。

## What Changes

- 新增子项目 `app/`：Flutter 桌面前端 + Rust 核心（以 flutter_rust_bridge 生成绑定），承载新应用的 UI 与业务逻辑
- 设备身份：首次启动自动生成 UUID（v4）并持久化；「本机默认短名」可在应用内设置（1–32 字符、禁止空白与控制字符、比较不区分大小写），未设置时为空
- 启动信息面板：应用启动后展示本机 UUID、短名、平台、地区与语言、本地时间、CPU、内存、局域网 IP
- 数据目录与配置文件：两平台用户级数据目录下的 `config.toml`（首版含 `[device]` 段），配置文件仅当前用户可读写
- OpenSpec 新增能力 `agent-bridge-app`，描述新应用的行为契约；既有 `agent-bridge` 能力描述的 Python 版过渡期冻结并存，最终被替换
- 仓库过渡期规则与文档：AGENTS.md 增补「过渡期双实现」条款（Python 版冻结仅修致命缺陷；新应用技术栈与构建前提；版本规则对新版本的适用）；README 增补新应用章节；openspec/config.yaml 上下文同步
- 版本：新应用版本源（Rust 常量）从 `0.0.0` 起步，归档前按既有「归档即 bump」规则推进至 `0.1.0`；Python 版 `BRIDGE_VERSION` 冻结于 `0.3.0`，不再随新应用推进

**BREAKING**：无。本变更不改动 Python 版任何代码与行为；两实现过渡期并存。

## Capabilities

### New Capabilities

- `agent-bridge-app`: 新一代跨平台桌面应用（Flutter GUI + Rust 核心）的应用形态、设备身份（UUID 自动生成与持久化、本机默认短名）、数据目录与配置文件契约、启动信息面板

### Modified Capabilities

（无——既有 `agent-bridge` 能力所描述的 Python 版本变不改动）

## Impact

- 新增：`app/` 子项目（Flutter 工程 + Rust crate/workspace、`Cargo.lock` 入库）及 flutter_rust_bridge 生成物
- 开发前提：构建新应用需 Flutter SDK 与 Rust 工具链（README 记载版本要求）；**运行的**分发产物不要求目标机安装这两者
- 新增第三方依赖（Rust crates 与 Flutter 包，如 uuid / serde / toml / sysinfo 等）：仅新应用使用；Python 版仍零第三方依赖
- 文档：`AGENTS.md`、`README.md`、`openspec/config.yaml` 增补过渡期章节
- 不受影响：`run.py`、`src/agent_bridge/`、`tests/`（Python 版原样保留）；`openspec/specs/agent-bridge/` 主规格
- 验证：Linux 侧本机构建与实跑；Windows 侧构建与运行列入「待用户验收清单」
