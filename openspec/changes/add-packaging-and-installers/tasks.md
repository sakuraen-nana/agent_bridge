# Tasks: add-packaging-and-installers

> 实施顺序：构建脚本与图标（1）→ AppImage（2）→ install.sh 与文档（3）→
> Windows 安装器脚本（4）→ 本机实跑（5）→ 回归（6）→ 收尾（7）。
> 决策依据见 design.md（D1–D7）；行为范围以差异规格为准。
> Windows 构建/安装与真实桌面 FUSE 场景列入 §8 待用户验收（归档时保留未勾选）。

## 1. 构建脚本（deb 与 tar.gz）

- [ ] 1.1 `app/packaging/build-linux.py`：版本读取（Cargo.toml 单一版本源）、`flutter build --release` 驱动、stage 组装（bundle + `/usr/bin/agent-bridge` + `.desktop` + 图标）、`dpkg-deb` 出 deb、tar.gz + `install.sh` 打包、`SHA256SUMS`；`app/dist/` 入 .gitignore；验证：`python3 app/packaging/build-linux.py --skip-appimage` 产出 deb/tar.gz/SHA256SUMS，`sha256sum -c` 通过
- [ ] 1.2 图标素材：由现有设计生成 256×256 PNG 与多尺寸 ICO 入库 `app/packaging/icons/`；验证：PIL 生成脚本产物存在且被 deb/AppImage 引用
- [ ] 1.3 deb 元数据与布局核对：`dpkg-deb -I` 与 `-c` 检查（版本=版本源、Depends 兼容双包名、路径正确）；验证：命令输出留证

## 2. AppImage

- [ ] 2.1 获取 appimagetool（GitHub Releases → `--appimage-extract` 本地运行）并打通 `$APPIMAGETOOL`/PATH 查找；缺失时的明确报错路径；验证：`--version` 可用；缺失分支实跑（临时清空 PATH 变量）报错且 deb/tar.gz 照常
- [ ] 2.2 AppDir 组装（AppRun 的 CLI 子命令转发 + GUI 回退）与出包；验证：AppImage 内 CLI 转发实跑（`--version` 与 hello 子命令）、GUI 启动截图

## 3. install.sh 与文档

- [ ] 3.1 `install.sh`（tar.gz 与 AppImage 共用逻辑：软链 `~/.local/bin`、幂等、PATH 提示；AppImage 场景 wrapper）；验证：实跑两场景（重复执行仍单链）并核对
- [ ] 3.2 README「分发与安装」小节：产物清单、安装/升级/卸载、PATH 生效、AppImage FUSE 前提与 extract-and-run、构建基线说明；AGENTS 过渡期章节补交付方式一行；验证：步骤按文档复现

## 4. Windows 安装器脚本

- [ ] 4.1 `app/packaging/windows/agent-bridge.iss`（Files/Registry PATH 记录-恢复策略/快捷方式/SetupIconFile）+ `build.ps1`（flutter windows + cargo release bin + iscc 提示）；验证：脚本审查（本机不可运行，附每步预期注释）；`iscc /?` 语义按 Inno 6 文档对齐（离线核对脚本结构）
- [ ] 4.2 ico 素材就位并被脚本引用；验证：文件存在、脚本引用路径一致

## 5. 本机实跑（Linux）

- [ ] 5.1 全量构建：三类产物 + 校验和；留证（ls -l、sha256sum -c）
- [ ] 5.2 deb 安装链路：`dpkg -i` → `command -v agent-bridge` 与 `--version`（=版本源）→ 从 `/usr/lib/agent-bridge` 启动 GUI（Xvfb 截图）→ `dpkg -r` 卸载 → 残留核对（`/usr/bin/agent-bridge` 不存在等）
- [ ] 5.3 tar.gz 链路：解压至临时目录 → 直接运行 CLI 与 GUI（截图）→ `install.sh` 用户级软链核对（幂等重跑）
- [ ] 5.4 AppImage 链路：运行（CLI 转发：`--version`、`hello`；GUI 截图；无 FUSE 时 `--appimage-extract-and-run`）→ 随附 `install.sh` 核对
- [ ] 5.5 版本一致性：产物文件名 / `--version` / 面板「应用版本」三处一致（截图/输出留证）

## 6. 测试回归

- [ ] 6.1 Rust / Flutter / Python 三套全量照跑；验证：全绿（84 / 18+6 / 69）

## 7. 收尾

- [ ] 7.1 分提交推送（构建脚本 / AppImage / 文档 / Windows 脚本 / 证据各自成提交）；验证：`git status` 干净、与远端一致
- [ ] 7.2 证据登记：勾选附证据；未实跑不勾选
- [ ] 7.3 归档前版本推进：`0.4.0` → `0.5.0`；产物按新版重建验证；验证：三处版本一致、提交推送
- [ ] 7.4 归档后核对：五个变更全部归档、主规格完整、路线图闭环说明写入 AGENTS 状态行

## 8. 待用户验收清单（需真机人工操作）

- [ ] 8.1 **Windows**：运行 `build.ps1` 出安装器（Inno Setup 6）；安装 → cmd 中 `agent-bridge --version`/`hello` 可用；GUI 启动与 UAC 行为
- [ ] 8.2 **Windows**：卸载 → 文件、PATH 条目、快捷方式清理干净
- [ ] 8.3 **Linux 真实桌面**：安装 deb → 桌面菜单可见并启动；`agent-bridge` 命令可用
- [ ] 8.4 **Linux 真实桌面**：AppImage 双击/命令行启动（含 FUSE 前提与 extract-and-run 备选）

## 9. 跟进项（本变更不实现，记录于此）

- [ ] 9.1 CI 构建流水线与产物发布渠道 —— 需要时另立
- [ ] 9.2 旧发行版（glibc 较老）的兼容构建（容器化基线或静态链接评估）—— 需要时另立
- [ ] 9.3 签名与公证（Windows 代码签名 / Linux 仓库签名）—— 依赖发布渠道
