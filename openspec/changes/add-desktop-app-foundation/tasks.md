# Tasks: add-desktop-app-foundation

> 实施顺序：脚手架（1）→ Rust 核心（2）→ 桥接与 GUI（3）→ 测试（4）→ Linux 实跑（5）→
> 文档与规则（6）→ 收尾（8）；Windows 侧构建与运行见 §7 待用户验收清单，未实跑不勾选。
> 决策依据见 design.md（D1–D12）；行为范围以差异规格 `specs/agent-bridge-app/spec.md` 为准，
> 不做规格外实现（服务端 / CLI / 提权 / 防火墙 / 配对 / 打包均属后续变更）。
> 文档边界按既有约定：本仓库不记录使用方的机器名、IP 与部署路径，需指代环境处用通用角色。

## 1. 脚手架与构建链路

- [ ] 1.1 以 flutter_rust_bridge 创建 `app/` 子项目（Flutter 工程 + `app/rust/` crate，仅生成 `linux` / `windows` 平台目录）；验证：`flutter build linux` 与 `cargo build --manifest-path app/rust/Cargo.toml` 均成功、产物存在
- [ ] 1.2 落 Rust 依赖选型（uuid / toml_edit / sysinfo / sys-locale / chrono / thiserror+anyhow / frb runtime，按 design D5）；验证：`cargo build` 成功，`app/rust/Cargo.lock` 与 `app/pubspec.lock` 存在且入库
- [ ] 1.3 补 `.gitignore`（`app/build/`、`app/rust/target/`、`.dart_tool/` 等构建产物；锁文件与 codegen 产物不受影响）；验证：`git status` 干净、锁文件与生成物均被跟踪
- [ ] 1.4 跑通 frb codegen 并将生成物入库（design D3）；验证：重跑 `flutter_rust_bridge_codegen generate` 后无未提交 diff

## 2. Rust 核心：数据目录、配置、身份、系统信息

- [ ] 2.1 数据目录解析（Linux `XDG_CONFIG_HOME` / `HOME`、Windows `APPDATA`；预留注入覆盖点，按 design D5）；验证：单测覆盖各分支与注入路径
- [ ] 2.2 `config.toml` 读写：缺失时创建（Linux 目录 0700 / 文件 0600）、原子替换写入、保留未知键与注释（toml_edit，按 design D6）；验证：单测断言权限与「往返不丢未知键/注释」，Linux 另实跑核对权限
- [ ] 2.3 损坏文件处置：解析失败 → 原名另存 `config.toml.bak-<时间戳>`、重建默认配置、返回界面提示标记；合法但缺字段视为缺省补全（不算损坏）；验证：单测覆盖两种情形
- [ ] 2.4 UUID：首次启动生成（v4）并持久化于配置，此后沿用；验证：单测断言两次读取一致、值格式为合法 UUID v4
- [ ] 2.5 短名：按 design D7 口径（trim、1–32 字符、禁空白与控制字符、Unicode 小写折叠判重）实现设置与清空；验证：单测覆盖边界（空 / 1 / 32 / 33 字符、含空白、控制字符、大小写折叠、清空）
- [ ] 2.6 系统信息采集（应用版本、平台、区域语言、本地时间、CPU、内存、IP，按 design D8 口径）；验证：单测通过；与系统工具的一致性由 5.2 实跑抽查

## 3. 桥接面与 GUI

- [ ] 3.1 frb 桥接 API：初始化快照（版本 / UUID / 短名 / 系统信息）与短名写操作（Result 错误映射，按 design D4）；验证：`cargo test` 通过且 Flutter 侧调用联调成功
- [ ] 3.2 信息面板：字段齐全、未设置短名与无 IP 时明确占位、支持手动刷新；验证：Linux 实跑核对（见 5.2）
- [ ] 3.3 短名编辑交互：非法输入拒绝并给可读提示、可清空、保存即时生效；验证：Linux 实跑（见 5.3）+ widget 测试
- [ ] 3.4 Flutter 侧 service 抽象（widget 测试可注入假实现，不依赖真 Rust 库）；验证：widget 测试通过

## 4. 测试

- [ ] 4.1 Rust 单测全绿；验证：`cargo test`（app/rust）退出码 0，覆盖 §2 全部条目
- [ ] 4.2 Flutter 测试全绿；验证：`flutter test`（app/）退出码 0
- [ ] 4.3 Python 版回归未受影响；验证：`python3 -m unittest discover -s tests` 全绿、用例数与改动前一致

## 5. Linux 实跑验证（本机）

- [ ] 5.1 首次启动：数据目录与 `config.toml` 被创建、文件权限 0600、UUID 已生成；验证：命令与输出留证
- [ ] 5.2 面板字段与系统工具抽查一致（CPU 核心数、内存总量、IP 列表）；验证：留证，逐项对照
- [ ] 5.3 短名全流程：设置 → 重启后持久；清空 → 持久；非法输入被拒且原值不变；验证：留证
- [ ] 5.4 手工编辑 `config.toml` 生效、注释保留；改坏后启动 → 生成 `bak-<时间戳>` 备份并重建；验证：留证
- [ ] 5.5 只读安装位置运行：将应用产物所在目录置为只读后启动；验证：应用正常运行，状态仍写入数据目录

## 6. 文档与仓库规则（过渡期）

- [ ] 6.1 `README.md`：增补「桌面应用（开发中）」章节（工具链版本组合、构建与运行命令、数据目录位置、路线图一句话），保留 Python 版章节；验证：按 README 步骤可复现构建；守卫检查（既有约定）零命中
- [ ] 6.2 `AGENTS.md`：增补「过渡期双实现」章节（规则适用范围、版本双源、测试命令分列，按 design D11）；验证：与 design D11 逐条一致
- [ ] 6.3 `openspec/config.yaml`：context 增补新应用技术栈与过渡期事实；验证：`openspec validate --strict --no-interactive` 通过

## 7. 待用户验收清单（需在 Windows 机器上人工操作）

- [ ] 7.1 按 README 在 Windows 机器备好构建工具链并构建应用；**预期**：构建成功，产出可运行的应用
- [ ] 7.2 运行应用核对信息面板；**预期**：数据目录位于 `%APPDATA%\agent-bridge`，UUID、短名占位、系统信息展示正确
- [ ] 7.3 设置短名并重启应用；**预期**：短名持久生效；再把 `config.toml` 手工改坏一次；**预期**：启动时生成 `bak-<时间戳>` 备份、重建配置并出现提示

## 8. 收尾

- [ ] 8.1 分提交推送（脚手架 / Rust 核心 / 桥接与 GUI / 测试 / 文档各自成提交）；验证：`git status` 干净、与远端一致、已推送提交不改写历史
- [ ] 8.2 证据登记：勾选各项并附证据（提交哈希 / 命令 / 退出码 / 产物名）；未在目标平台（Linux / Windows）实跑的不勾选
- [ ] 8.3 归档前版本推进：新应用版本源 `0.0.0` → `0.1.0`（按 AGENTS.md 开发流程第 6 条，「归档即 bump」延续适用于新版本源）；验证：界面展示与代码常量一致，提交并推送

## 9. 跟进项（本变更不实现，记录于此）

- [ ] 9.1 服务端核心（hello / exec / download）、双 token（会话 + 长期）、多设备配置与短名寻址、`agent-bridge` CLI 与 PATH 注册 —— 变更 ② / ⑤
- [ ] 9.2 提权、防火墙、托盘常驻、开机自启、剪贴板导出 —— 变更 ③（数据目录按调用者用户解析的注入点已在 design D5 预留）
- [ ] 9.3 局域网发现与配对 —— 变更 ④
- [ ] 9.4 macOS 平台目录与适配、应用 ID 最终取值与图标 —— 需要时另立
