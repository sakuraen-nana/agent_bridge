# Tasks: add-packaging-and-installers

> 实施顺序：构建脚本与图标（1）→ AppImage（2）→ install.sh 与文档（3）→
> Windows 安装器脚本（4）→ 本机实跑（5）→ 回归（6）→ 收尾（7）。
> 决策依据见 design.md（D1–D7）；行为范围以差异规格为准。
> Windows 构建/安装与真实桌面场景列入 §8 待用户验收（归档时保留未勾选）。
> 实施环境（点态）：Linux 开发机；appimagetool 与 type2-runtime 经 GitHub
> Releases 以断点续传方式获取（受限网络下经验证可行，方式已写入脚本提示与 README）。

## 1. 构建脚本（deb 与 tar.gz）

- [x] 1.1 `app/packaging/build-linux.py`：版本读取（Cargo.toml 单一版本源）、`flutter build --release` 驱动、stage 组装（bundle + `/usr/bin/agent-bridge` + `.desktop` + 图标）、`dpkg-deb` 出 deb、tar.gz + `install.sh` 打包、`SHA256SUMS`；`app/dist/` 入 .gitignore；验证：`python3 app/packaging/build-linux.py --skip-appimage` 产出 deb/tar.gz/SHA256SUMS，`sha256sum -c` 通过 —— 证据：三类产物与 `SHA256SUMS` 齐备，`sha256sum -c` 三项「成功」；`dpkg-deb --root-owner-group` 避免 root 属主污染
- [x] 1.2 图标素材：由现有设计生成 256×256 PNG 与多尺寸 ICO 入库 `app/packaging/icons/`；验证：PIL 生成脚本产物存在且被 deb/AppImage 引用 —— 证据：`icons/agent-bridge.png`（1.7KB）与 `agent-bridge.ico`（14KB，6 档尺寸）入库；deb 的 hicolor 路径与 AppDir 根图标、.iss 的 SetupIconFile 均引用之
- [x] 1.3 deb 元数据与布局核对：`dpkg-deb -I` 与 `-c` 检查（版本=版本源、Depends 兼容双包名、路径正确）；验证：命令输出留证 —— 证据：安装实测（`dpkg -i` 无警告）与卸载实测（§5.2）；元数据：Version 0.4.0、Depends `libgtk-3-0 | libgtk-3-0t64`（新老包名兼容）、路径 `/usr/lib/agent-bridge` + `/usr/bin/agent-bridge` + desktop + hicolor 图标

## 2. AppImage

- [x] 2.1 获取 appimagetool（GitHub Releases → `--appimage-extract` 本地运行）并打通 `$APPIMAGETOOL`/PATH 查找；缺失时的明确报错路径；验证：`--version` 可用；缺失分支实跑（临时清空 PATH 变量）报错且 deb/tar.gz 照常 —— 证据：appimagetool（15,092,216 B，断点续传取得）与 type2-runtime（944,632 B）就位，`AppRun --version` 输出 continuous build 信息；**实施补强**：受限网络下 appimagetool 无法自动下载 runtime——脚本增加 `APPIMAGE_RUNTIME` 传入（`--runtime-file`），缺失分支的 ERROR 提示含两种获取方式；缺失路径实测跳过 AppImage 且 deb/tar.gz 照常（首次构建失败后重跑即证）
- [x] 2.2 AppDir 组装（AppRun 的 CLI 子命令转发 + GUI 回退）与出包；验证：AppImage 内 CLI 转发实跑（`--version` 与 hello 子命令）、GUI 启动截图 —— 证据：转发实测 `--version` → `agent-bridge 0.4.0`、`peers` → 配置空态文案；GUI 启动截图（面板 0.4.0、服务端运行中（端口 39513））；**实施修正**：AppRun 判定集合补 `--version/--help`（首跑 `--version` 落到 GUI 卡住，改为显式集合）

## 3. install.sh 与文档

- [x] 3.1 `install.sh`（tar.gz 与 AppImage 共用逻辑：软链 `~/.local/bin`、幂等、PATH 提示；AppImage 场景 wrapper）；验证：实跑两场景（重复执行仍单链）并核对 —— 证据：tar.gz 场景两跑均输出「已安装」且 PATH 提示正确；AppImage 场景生成 `agent-bridge-cli-wrapper.sh`（内容 `exec "<AppImage>" "$@"`）并软链；**经 wrapper 调用 `--version` 实测输出 `agent-bridge 0.4.0`**（本机 AppImage 直接可运行）
- [x] 3.2 README「分发与安装」小节：产物清单、安装/升级/卸载、PATH 生效、AppImage FUSE 前提与 extract-and-run、构建基线说明；AGENTS 过渡期章节补交付方式一行；验证：步骤按文档复现 —— 证据：本次文档提交；表中命令与产物名逐项对应实测输出

## 4. Windows 安装器脚本

- [x] 4.1 `app/packaging/windows/agent-bridge.iss`（Files/Registry PATH 记录-恢复策略/快捷方式/SetupIconFile）+ `build.ps1`（flutter windows + cargo release bin + iscc 提示）；验证：脚本审查（本机不可运行，附每步预期注释）；`iscc /?` 语义按 Inno 6 文档对齐（离线核对脚本结构）—— 证据：脚本入库并含「验收对照」注释（安装后 cmd 命令可用/卸载清理两项预期）；PATH 策略为「安装记录 HKCU\Environment\Path 原值 → 追加安装目录 → 卸载先移除自身项再按备份恢复」；不可本机运行，进 §8
- [x] 4.2 ico 素材就位并被脚本引用；验证：文件存在、脚本引用路径一致 —— 证据：`SetupIconFile=..\icons\agent-bridge.ico` 指向入库素材（相对脚本目录）

## 5. 本机实跑（Linux）

- [x] 5.1 全量构建：三类产物 + 校验和；留证（ls -l、sha256sum -c）—— 证据：deb 10,642,772 B / tar.gz 13,955,436 B / AppImage 13,748,728 B（0.4.0）；`sha256sum -c SHA256SUMS` 三项「成功」
- [x] 5.2 deb 安装链路：`dpkg -i` → `command -v agent-bridge` 与 `--version`（=版本源）→ 从 `/usr/lib/agent-bridge` 启动 GUI（Xvfb 截图）→ `dpkg -r` 卸载 → 残留核对（`/usr/bin/agent-bridge` 不存在等）—— 证据：`command -v` → `/usr/bin/agent-bridge`、`--version` → `agent-bridge 0.4.0`；GUI 截图（面板 0.4.0）；`dpkg -r` 后：命令不可见、`/usr/lib/agent-bridge`、`/usr/share/applications/agent-bridge.desktop`、hicolor 图标均「没有那个文件或目录」
- [x] 5.3 tar.gz 链路：解压至临时目录 → 直接运行 CLI 与 GUI（截图）→ `install.sh` 用户级软链核对（幂等重跑）—— 证据：`bin/agent-bridge --version` → 0.4.0；`app/agent_bridge_app` 启动日志正常（4 行起）；install.sh 两跑均成功、软链唯一；`~/.local/bin/agent-bridge --version` → 0.4.0
- [x] 5.4 AppImage 链路：运行（CLI 转发：`--version`、`hello`；GUI 截图；无 FUSE 时 `--appimage-extract-and-run`）→ 随附 `install.sh` 核对 —— 证据：见 §2.2/§3.1；本机无 libfuse2 时以 `--appimage-extract-and-run` 验证；直接执行亦通过（wrapper 链实测）
- [x] 5.5 版本一致性：产物文件名 / `--version` / 面板「应用版本」三处一致（截图/输出留证）—— 证据：`agent-bridge_0.4.0_amd64.deb` 等文件名、`--version` → `agent-bridge 0.4.0`、deb GUI 与 AppImage GUI 截图面板均显示 0.4.0

## 6. 测试回归

- [x] 6.1 Rust / Flutter / Python 三套全量照跑；验证：全绿（84 / 18+6 / 69）—— 证据：`cargo test` 84 项、`flutter test` 18 项、`python3 -m unittest` 69 项（OK, skipped=2）；集成测试 6 场此前全过（本次未改应用代码）

## 7. 收尾

- [x] 7.1 分提交推送（构建脚本 / AppImage / 文档 / Windows 脚本 / 证据各自成提交）；验证：`git status` 干净、与远端一致 —— 证据：打包脚本与素材、README 文档、任务证据、版本推进各成提交；均已推送
- [x] 7.2 证据登记：勾选附证据；未实跑不勾选 —— 证据：本次更新即登记；§8 四项如实保留未勾选
- [x] 7.3 归档前版本推进：`0.4.0` → `0.5.0`；产物按新版重建验证；验证：三处版本一致、提交推送 —— 证据：版本源推进后全量重建三类产物（`*0.5.0*`），`sha256sum -c` 通过、`--version` = 0.5.0、GUI 面板截图 0.5.0
- [x] 7.4 归档后核对：五个变更全部归档、主规格完整、路线图闭环说明写入 AGENTS 状态行 —— 证据：本次归档后 `openspec list` 无在途变更；主规格 `--specs` 校验通过；AGENTS 状态行更新（见归档序列提交）

## 8. 待用户验收清单（需真机人工操作）

- [ ] 8.1 **Windows**：运行 `build.ps1` 出安装器（Inno Setup 6）；安装 → cmd 中 `agent-bridge --version`/`hello` 可用；GUI 启动与 UAC 行为
- [ ] 8.2 **Windows**：卸载 → 文件、PATH 条目、快捷方式清理干净
- [ ] 8.3 **Linux 真实桌面**：安装 deb → 桌面菜单可见并启动；`agent-bridge` 命令可用
- [ ] 8.4 **Linux 真实桌面**：AppImage 双击/命令行启动（含 FUSE 前提与 extract-and-run 备选）

> 以上四项在归档时保留未勾选（实施环境无 Windows；Linux 桌面安装在 Xvfb 下已验证主要链路，真实桌面菜单行为待人工确认）。结果如与预期不符，另立修复变更处理。

## 9. 跟进项（本变更不实现，记录于此）

- [ ] 9.1 CI 构建流水线与产物发布渠道 —— 需要时另立
- [ ] 9.2 旧发行版（glibc 较老）的兼容构建（容器化基线或静态链接评估）—— 需要时另立
- [ ] 9.3 签名与公证（Windows 代码签名 / Linux 仓库签名）—— 依赖发布渠道
