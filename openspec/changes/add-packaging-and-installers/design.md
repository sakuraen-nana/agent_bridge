## Context

动机见 `proposal.md`；契约见差异规格。约束与现状：

- 应用束已可构建（`app/build/linux/x64/release/bundle/`，含 `agent_bridge_app` 可执行与 `lib/`）；CLI 二进制在 `app/rust/target/{debug,release}/agent-bridge`。
- 本机工具链：`dpkg-deb`、`fakeroot` 在；`appimagetool`/`linuxdeploy` 缺（apt 无包），GitHub HTTPS 可达——经 GitHub Releases 获取 `appimagetool` 的 AppImage，以 `--appimage-extract` 方式本地运行（免 FUSE）。
- 仓库习惯：脚本用 Python 标准库；产物不入口库。
- Windows 侧只交付脚本（构建在 Windows 上执行），全部机制走用户验收。

## Goals / Non-Goals

**Goals:**

- 三类 Linux 产物一条命令构建、本机可安装/可运行/可卸载验证；Windows 安装器脚本就绪
- 安装后 `agent-bridge` 命令在 bash/cmd 直接可用（用户原始硬需求）

**Non-Goals:**

- 不做 CI 产物流水线（后续需要时另立）
- 不做跨发行版/旧 glibc 兼容矩阵（为本机构建基线，文档注明；tar.gz 供旧发行版用户自行构建）
- 不做自动更新、不做签名（无发布渠道，签名留给分发方）

## Decisions

### D1 构建脚本：`app/packaging/build-linux.py`（标准库）

步骤：`flutter build linux --release` → 组装 `stage/`（应用束 + `usr/bin/agent-bridge`（release CLI）+ `.desktop` + 图标）→ `dpkg-deb --build`（以 fakeroot 赋属主）→ `tar.gz`（含 `install.sh`）→ AppImage（详见 D3）→ `SHA256SUMS`。产物落 `app/dist/`（gitignore）。版本读取：解析 `app/rust/Cargo.toml` 的 `package.version`（唯一版本源）。子命令开关：`--skip-appimage` 等便于分步复跑。

### D2 deb 布局与元数据

- 文件：`/usr/lib/agent-bridge/`（bundle 原样）、`/usr/bin/agent-bridge`（CLI）、`/usr/share/applications/agent-bridge.desktop`（`Exec=/usr/lib/agent-bridge/agent_bridge_app`；提权由应用启动逻辑自理）、`/usr/share/icons/hicolor/256x256/apps/agent-bridge.png`。
- 元数据：`Package: agent-bridge`、`Version`=版本源、`Architecture: amd64`、`Depends: libgtk-3-0 | libgtk-3-0t64, libglib2.0-0 | libglib2.0-0t64`（兼容新老包名）、`Section: utils`、通用 Maintainer 字符串（不写个人/机器信息）。
- 不做 postinst/prerm（dpkg 自身完成文件与 PATH）；卸载即删。

### D3 AppImage：AppRun CLI 转发 + 工具获取

- AppDir：`AppRun`（脚本：`argv[1]` ∈ {peers,hello,exec,download,token} → `exec usr/bin/agent-bridge "$@"`；否则 `exec usr/lib/agent-bridge/agent_bridge_app`）、`usr/bin/agent-bridge`、`usr/lib/agent-bridge/`（bundle）、根级 `.desktop` 与 `agent-bridge.png`。
- 工具：构建脚本按序找 `appimagetool`（PATH）→ `$APPIMAGETOOL`（环境变量，允许指向本地已下载文件）→ 都没有则报错（含 GitHub Releases 获取与 `--appimage-extract` 免 FUSE 用法说明），deb/tar.gz 照常产出。
- 运行端前提：FUSE2 或 `--appimage-extract-and-run`——写入 README。

### D4 `install.sh`（tar.gz 与 AppImage 内）

用户级幂等安装：把 `agent-bridge` 软链到 `~/.local/bin`（AppImage 场景软链到随附 wrapper——wrapper `exec <所在一侧的 AppImage> "$@"`，借 AppRun 转发；tar.gz 场景直接软链真实 CLI）。若 `~/.local/bin` 不在 PATH 给出追加提示。卸载 = 删软链。

### D5 Windows：Inno Setup 脚本 + 构建脚本

- `app/packaging/windows/agent-bridge.iss`：`DefaultDirName={autopf}\agent-bridge`、`PrivilegesRequired=admin`（与应用清单的 UAC 语义一致）、安装 bundle 与 `agent-bridge.exe`（CLI）、`[Registry]` 写 `HKCU\Environment;Path` 追加安装目录（`uninsdeletevalue` 配合精确移除策略：安装时记录原值、卸载恢复——Inno 的 `ChangesEnvironment=yes` 处理广播；具体见脚本注释）、开始菜单快捷方式；`SetupIconFile` 用生成的 ico。
- `app/packaging/windows/build.ps1`：`flutter build windows --release` → `cargo build --release --bin agent-bridge` → `iscc`（缺失时提示安装 Inno Setup 6）。
- 图标：复用/生成 `agent-bridge.ico`（PIL 由现有设计生成多尺寸）。

### D6 图标与素材

由 `assets/tray_icon.png` 同源设计生成 256×256 PNG（桌面/AppImage）与多尺寸 ICO（Windows 安装器），一并入库 `app/packaging/icons/`。

### D7 验证

实跑为主：构建 → 校验和 → `dpkg -i` 安装 → `command -v agent-bridge` 与 `--version` → 从 `/usr/lib` 启动 GUI（Xvfb 截图）→ `dpkg -r` 卸载 → 路径与文件核对；tar.gz 解压运行 + `install.sh`；AppImage 直接运行（CLI 转发与 GUI）+ `install.sh`。Windows 与真实桌面 FUSE 场景进用户验收。

## Risks / Trade-offs

- [构建基线 glibc 较新，旧发行版无法运行] → README 注明基线（Ubuntu 24.04/glibc 2.39 构建）；旧发行版用户走源码构建
- [appimagetool 获取依赖 GitHub 可达] → 该网络已实测可达；不可达时脚本明确报错且不阻断 deb/tar.gz
- [Windows 脚本未在本机验证] → 全部机制列入验收清单，脚本内注释写清每步预期
- [PATH 追加的卸载精确性（避免误删用户同名项）] → 采用「安装记录原值、卸载恢复」策略并写入脚本注释
- [AppImage 运行端 FUSE 缺失] → 文档给出 `--appimage-extract-and-run` 备选

## Migration Plan

无部署态迁移；新增交付物。回滚即不再分发产物。发布流程（版本来源、产物命名、校验和）以本变更规格为准，供后续版本沿用。
