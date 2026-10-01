## ADDED Requirements

### Requirement: Linux 安装与分发产物
仓库 SHALL 提供构建脚本，产出并供校验：`.deb` 安装包、`AppImage` 单文件与便携 `tar.gz`；三者文件名含版本号，版本 SHALL 取自新应用版本源（`app/rust/Cargo.toml` 的 `package.version`）而非另立。`.deb` SHALL 安装：应用束至 `/usr/lib/agent-bridge/`、CLI 入口 `/usr/bin/agent-bridge`、桌面入口（`.desktop`）与图标至系统目录、并声明运行依赖；安装完成后 shell 中 `agent-bridge` SHALL 直接可用；卸载 SHALL 移除全部随包文件与桌面入口。AppImage SHALL 自包含（含应用束与 CLI）；当首个参数为 CLI 子命令时 SHALL 转发给内嵌 CLI，否则启动图形界面；并随附 `install.sh` 可把 CLI 用户级软链至 `~/.local/bin`。`tar.gz` SHALL 解压即用并含同款 `install.sh`。构建脚本在 AppImage 工具链缺失时 MUST 明确报错并说明获取方式，且仍产出其余产物；全部产物 SHALL 附 SHA256 校验清单。

#### Scenario: 构建产出完整性
- **WHEN** 在 Linux 构建机上运行构建脚本
- **THEN** 产出 deb / AppImage / tar.gz 与 `SHA256SUMS`，文件名与版本号一致，校验和可逐一通过

#### Scenario: deb 安装后命令行可用
- **WHEN** 安装 `.deb` 后在 shell 中执行 `agent-bridge --version`
- **THEN** 输出与安装包一致的版本号（PATH 无需额外配置）

#### Scenario: deb 卸载清理
- **WHEN** 卸载该 `.deb`
- **THEN** `/usr/bin/agent-bridge`、应用束、桌面入口与图标均被移除

#### Scenario: AppImage 的 CLI 转发与 GUI
- **WHEN** 以 CLI 子命令为首参运行 AppImage（如 `./agent-bridge-*.AppImage hello <设备>`）
- **THEN** 执行内嵌 CLI；无此参数时启动图形界面

#### Scenario: install.sh 用户级安装
- **WHEN** 运行 tar.gz 或 AppImage 随附的 `install.sh`
- **THEN** CLI 被软链至 `~/.local/bin`（已存在则覆盖且幂等），并提示 PATH 生效方式

#### Scenario: 工具链缺失时的明确行为
- **WHEN** 构建机上没有任何可用的 AppImage 工具
- **THEN** 构建脚本明确报错并给出获取方式，且 deb 与 tar.gz 仍然产出

### Requirement: Windows 安装器
仓库 SHALL 提供 Inno Setup 安装器脚本与构建脚本（构建在 Windows 上执行，仓库不含预编译产物）：把应用束与 CLI 安装至安装目录；把安装目录加入**用户级 PATH**（卸载时移除该条目）；提供开始菜单入口；应用运行期的管理员提权语义不变（由执行清单触发 UAC）。卸载 SHALL 移除文件、PATH 条目与快捷方式。

#### Scenario: 安装后 cmd 中命令可用
- **WHEN** 在 Windows 上安装后新开 cmd 执行 `agent-bridge --version`
- **THEN** 输出与安装产物一致的版本号

#### Scenario: 卸载清理
- **WHEN** 卸载安装器产物
- **THEN** 安装目录、PATH 条目与快捷方式均被移除，不留残余

### Requirement: 安装后命令行可用（跨平台统一语义）
任一安装或分发方式完成后，shell（bash / cmd）SHALL 能直接调用 `agent-bridge <子命令>`；CLI SHALL 与图形界面共用同一数据目录与配置文件（安装方式不影响该契约）；`agent-bridge --version` SHALL 与所安装应用版本一致。

#### Scenario: 与 GUI 共用配置
- **WHEN** 通过任意安装方式安装后，GUI 修改本机短名，随后在 shell 用 CLI 访问同机配置
- **THEN** CLI 读到同一数据目录的同一配置（含刚修改的短名）

#### Scenario: 版本一致
- **WHEN** 对比 `agent-bridge --version`、安装产物文件名与信息面板「应用版本」
- **THEN** 三者一致
