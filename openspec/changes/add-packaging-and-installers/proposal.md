## Why

路线图第 5/5 步（收官）。新应用功能已完整（设备身份、服务端与 CLI、权限/防火墙/托盘、发现与配对），但交付形态仍是「开发者手动构建的 build 目录」——普通用户无法安装、shell 里没有 `agent-bridge` 命令、开机自启与桌面集成无从谈起。本变更把应用变为**可分发的安装产物**：Linux 出 `.deb` 与 `AppImage`（另附便携 `tar.gz`），Windows 出安装器（Inno Setup 脚本随仓库，构建在 Windows 上执行）；任一安装方式完成后，shell（bash / cmd）中即可直接使用 `agent-bridge` 客户端命令。

## What Changes

- **构建脚本（仓库内，Python 标准库）**：`app/packaging/build-linux.py` 一次构建三类产物到 `dist/`：
  - `agent-bridge_<版本>_amd64.deb`——安装包布局：`/usr/lib/agent-bridge/`（应用束）、`/usr/bin/agent-bridge`（CLI 入口，PATH 直接可用）、`/usr/share/applications/agent-bridge.desktop`（桌面入口）、图标；依赖声明（GTK 运行库等）；卸载即删除
  - `agent-bridge-<版本>-x86_64.AppImage`——自包含单文件；AppRun 支持 **CLI 转发**（首个参数为 CLI 子命令时执行内嵌 CLI，否则启动 GUI）；随 AppImage 附 `install.sh`（用户级把 CLI 软链到 `~/.local/bin`）
  - `agent-bridge-<版本>-linux-x86_64.tar.gz`——便携包（解压即用 + 同款 `install.sh`）
  - 校验和清单 `SHA256SUMS`
- **Windows 安装器脚本**：`app/packaging/windows/agent-bridge.iss`（Inno Setup）：安装应用束与 CLI，把安装目录加入 PATH（安装/卸载时注册与移除），开始菜单与桌面快捷方式可选；`build.ps1` 供在 Windows 上一条命令出安装器（本仓库不含预编译产物）
- **版本一致性**：全部产物版本号取自新应用版本源（`app/rust/Cargo.toml` 的 `package.version`），构建脚本读取而非另立
- **文档**：README「分发与安装」小节（产出物、安装、升级、卸载、PATH 生效方式、AppImage 的 FUSE 前提与 `install.sh`）；AGENTS 过渡期章节补交付方式
- **规格**：新增「Linux 安装与分发产物」「Windows 安装器」「安装后命令行可用」三组要求

**BREAKING**：无（纯增量交付物）。

## Capabilities

### New Capabilities

（无——全部落在 `agent-bridge-app` 内）

### Modified Capabilities

- `agent-bridge-app`：新增安装与分发产物 / Windows 安装器 / 命令行可用三组要求

## Impact

- 新增 `app/packaging/`（构建脚本、Inno 脚本、install.sh、图标复用 assets）；`dist/` 为构建产物（gitignore 排除）
- 本机（Linux）可端到端验证：构建三类产物 → 安装 deb 到本机（属本工具自身文件，验证后卸载）→ PATH 内 `agent-bridge` 可用、GUI 从安装位置启动 → 卸载后清理；tar.gz 解压运行与 install.sh；AppImage 运行（本机无 FUSE 时以 extract-and-run 验证，FUSE 前提写入文档）
- AppImage 构建工具（appimagetool）在本机缺失：构建脚本在其可得时生成 AppImage、缺失时明确报错并仍然产出 deb 与 tar.gz（工具获取方式写入文档）；本次实施经便携方式获取并实跑验证
- Windows 安装器仅交付脚本与构建说明，构建/安装/卸载/ PATH 生效在 Windows 机器上验收
- 版本推进：归档前 `0.4.0` → `0.5.0`（产物版本随之一致）
