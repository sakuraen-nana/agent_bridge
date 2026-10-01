# Tasks: add-desktop-app-foundation

> 实施顺序：脚手架（1）→ Rust 核心（2）→ 桥接与 GUI（3）→ 测试（4）→ Linux 实跑（5）→
> 文档与规则（6）→ 收尾（8）；Windows 侧构建与运行见 §7 待用户验收清单（归档时保留未勾选，
> 待具备 Windows 机器后由用户执行；结果如与预期不符，另立修复变更）。
> 决策依据见 design.md（D1–D12）；行为范围以差异规格 `specs/agent-bridge-app/spec.md` 为准，
> 不做规格外实现（服务端 / CLI / 提权 / 防火墙 / 配对 / 打包均属后续变更）。
> 文档边界按既有约定：本仓库不记录使用方的机器名、IP 与部署路径，需指代环境处用通用角色。
> 实施环境（点态）：Linux 开发机；无可用外网代理，依赖获取走国内镜像（pub 走
> pub.flutter-io.cn、crates 走 USTC sparse 索引），镜像选择不写入仓库。

## 1. 脚手架与构建链路

- [x] 1.1 以 flutter_rust_bridge 创建 `app/` 子项目（Flutter 工程 + `app/rust/` crate，仅生成 `linux` / `windows` 平台目录）；验证：`flutter build linux` 与 `cargo build --manifest-path app/rust/Cargo.toml` 均成功、产物存在 —— 证据：提交 `2f4b13c`；`flutter build linux` → `✓ Built build/linux/x64/release/bundle/agent_bridge_app`（含 `libagent_bridge.so`）；`cargo build` 成功（环境：Flutter 3.44.1 / Dart 3.12.1 / Rust 1.99.0 / frb 2.11.1）
- [x] 1.2 落 Rust 依赖选型（uuid / toml_edit / sysinfo / sys-locale / chrono / thiserror+anyhow / frb runtime，按 design D5）；验证：`cargo build` 成功，`app/rust/Cargo.lock` 与 `app/pubspec.lock` 存在且入库 —— 证据：提交 `2f4b13c`；实际版本：uuid 1.26.1、toml_edit 0.25.15、sysinfo 0.39.6、sys-locale 0.3.2、chrono 0.4.45、thiserror 2.0.21、anyhow 1.0.104（dev: tempfile 3.27.0）
- [x] 1.3 补 `.gitignore`（`app/build/`、`app/rust/target/`、`.dart_tool/` 等构建产物；锁文件与 codegen 产物不受影响）；验证：`git status` 干净、锁文件与生成物均被跟踪 —— 证据：`git check-ignore` 对 `app/build`、`app/rust/target`、`app/.dart_tool`、`rust_builder/cargokit/build_tool/.dart_tool` 四路径全部命中（IGNORED）；提交 `2f4b13c` 共 109 个文件入库，含两把锁文件与 frb 生成物
- [x] 1.4 跑通 frb codegen 并将生成物入库（design D3）；验证：重跑 `flutter_rust_bridge_codegen generate` 后无未提交 diff —— 证据：codegen `Done!`；生成的 `frb_generated.rs` 与 `lib/src/rust/*.dart` 入库（提交 `2f4b13c`）；实施注：桥接面变更后重跑一次（simple → init/device），生成物随实现提交更新

## 2. Rust 核心：数据目录、配置、身份、系统信息

- [x] 2.1 数据目录解析（Linux `XDG_CONFIG_HOME` / `HOME`、Windows `APPDATA`；预留注入覆盖点，按 design D5）；验证：单测覆盖各分支与注入路径 —— 证据：`tests/config_test.rs` 5 个目录解析用例（Windows APPDATA、缺 APPDATA 报错、XDG 绝对路径优先、相对 XDG 被忽略回退 HOME、缺 HOME 报错）；`cargo test` 全绿
- [x] 2.2 `config.toml` 读写：缺失时创建（Linux 目录 0700 / 文件 0600）、原子替换写入、保留未知键与注释（toml_edit，按 design D6）；验证：单测断言权限与「往返不丢未知键/注释」，Linux 另实跑核对权限 —— 证据：单测 `config_file_permissions_are_0600`、`manual_edit_takes_effect_and_unknown_keys_survive_write`（注释与 `[future_section]` 段在写入后保留）；实跑场景 D：修复写回后 `# 手工添加的注释` 原样保留
- [x] 2.3 损坏文件处置：解析失败 → 原名另存 `config.toml.bak-<时间戳>`、重建默认配置、返回界面提示标记；合法但缺字段视为缺省补全（不算损坏）；验证：单测覆盖两种情形 —— 证据：单测 `corrupt_config_is_backed_up_and_rebuilt`、`missing_uuid_is_repaired_keeping_short_name`；实跑场景 C：`这不是 toml [[['` → 生成 `config.toml.bak-20261001231155`，界面横幅完整显示解析错误与备份文件名
- [x] 2.4 UUID：首次启动生成（v4）并持久化于配置，此后沿用；验证：单测断言两次读取一致、值格式为合法 UUID v4 —— 证据：单测 `creates_config_with_uuid_and_reuses_it`；实跑：多轮启动 UUID 恒定（`05bd614b-…` 场景间保持；修复场景新生成 `c0b0c481-…` 后跨重启保持）
- [x] 2.5 短名：按 design D7 口径（trim、1–32 字符、禁空白与控制字符、Unicode 小写折叠判重）实现设置与清空；验证：单测覆盖边界（空 / 1 / 32 / 33 字符、含空白、控制字符、大小写折叠、清空）—— 证据：`tests/identity_test.rs` 5 项全绿（含全角空格 U+3000、`\u{7}` 控制字符、希腊字母折叠）；清空路径由 `manual_edit_…_survive_write` 断言键移除
- [x] 2.6 系统信息采集（应用版本、平台、区域语言、本地时间、CPU、内存、IP，按 design D8 口径）；验证：单测通过；与系统工具的一致性由 5.2 实跑抽查 —— 证据：`tests/sysinfo_view_test.rs` 通过；抽查对照见 5.2

## 3. 桥接面与 GUI

- [x] 3.1 frb 桥接 API：初始化快照（版本 / UUID / 短名 / 系统信息）与短名写操作（Result 错误映射，按 design D4）；验证：`cargo test` 通过且 Flutter 侧调用联调成功 —— 证据：`app_init` / `refresh_system` / `set_short_name` 生成（`lib/src/rust/api/{init,device}.dart`）；端到端集成测试实调通过；错误映射：frb 以 anyhow Debug 序列化（含 backtrace），展示层已截断（见 §4.2 证据与提交 `31fbe5a`）
- [x] 3.2 信息面板：字段齐全、未设置短名与无 IP 时明确占位、支持手动刷新；验证：Linux 实跑核对（见 5.2）—— 证据：实跑截图逐字段核对通过（「未设置」「无」占位由 widget 测试另覆盖）；刷新按钮逻辑由 widget 测试 `刷新系统信息` 覆盖
- [x] 3.3 短名编辑交互：非法输入拒绝并给可读提示、可清空、保存即时生效；验证：Linux 实跑（见 5.3）+ widget 测试 —— 证据：集成测试（真实 UI）与 widget 测试（假实现）双路径通过；实跑确认错误文案「短名无效：不能包含空白或控制字符（发现 ' '）」
- [x] 3.4 Flutter 侧 service 抽象（widget 测试可注入假实现，不依赖真 Rust 库）；验证：widget 测试通过 —— 证据：`lib/src/bridge_service.dart` 抽象 + `test/home_page_test.dart` 7 项全绿

## 4. 测试

- [x] 4.1 Rust 单测全绿；验证：`cargo test`（app/rust）退出码 0，覆盖 §2 全部条目 —— 证据：`test result: ok. 10 + 5 + 1 = 16 passed; 0 failed`，退出码 0
- [x] 4.2 Flutter 测试全绿；验证：`flutter test`（app/）退出码 0 —— 证据：`flutter test` → `+7: All tests passed!`；另 `flutter analyze` → `No issues found!`；端到端 `flutter test integration_test -d linux`（Xvfb 虚拟显示 + 隔离 XDG_CONFIG_HOME）→ `+1: All tests passed!`、退出码 0
- [x] 4.3 Python 版回归未受影响；验证：`python3 -m unittest discover -s tests` 全绿、用例数与改动前一致 —— 证据：`Ran 69 tests in 27.5s · OK (skipped=2)`（与改动前一致；2 项 skip 为既有的平台限定用例）

## 5. Linux 实跑验证（本机）

- [x] 5.1 首次启动：数据目录与 `config.toml` 被创建、文件权限 0600、UUID 已生成；验证：命令与输出留证 —— 证据：全新启动后 `ls -la` → 目录 `drwx------`(0700)、`-rw-------`(0600) `config.toml` 55 字节，含 `uuid = "05bd614b-efc2-4d0a-81ef-a2e8e649836c"`；截图核对面板 UUID 与文件一致
- [x] 5.2 面板字段与系统工具抽查一致（CPU 核心数、内存总量、IP 列表）；验证：留证，逐项对照 —— 证据：CPU「i5-14600KF（8 物理核）」对 `nproc`=8（lscpu 同型号）；内存「总计 7.7 GiB · 可用 5.1 GiB」对 `free -h`「7.7Gi / 5.2Gi」（采样时刻差异）；IP 六条对 `ip -4` 非环回全集一致（ens33 + docker0 + 4 个 br-*）；时间与 `date`、区域与 LANG=zh_CN.UTF-8（面板 zh-CN）一致
- [x] 5.3 短名全流程：设置 → 重启后持久；清空 → 持久；非法输入被拒且原值不变；验证：留证 —— 证据：集成测试（真实 UI→frb→Rust→文件）：保存「端到端-A」→ 文件落地 → 重新初始化读回；`含 空格` 被拒且原值不变；清空后键移除且读回为 null。实跑另证：手工设置的「保留我」在后续多轮启动中持续显示
- [x] 5.4 手工编辑 `config.toml` 生效、注释保留；改坏后启动 → 生成 `bak-<时间戳>` 备份并重建；验证：留证 —— 证据：场景 B（手工改 `short_name = "手改名"` → 面板与输入框均显示）；场景 C（写坏 → 备份 + 重建 + 界面提示，截图留证）；场景 D（保留注释与短名、补全 UUID 写回）
- [x] 5.5 只读安装位置运行：将应用产物所在目录置为只读后启动；验证：应用正常运行，状态仍写入数据目录 —— 证据：产物拷贝至只读目录（`chmod -R a-w`）后以非 root 用户（`nobody`）运行成功；数据写入该用户 HOME 下 `~/.config/agent-bridge/`，目录 0700、文件 0600、属主正确；安装目录未被写入

## 6. 文档与仓库规则（过渡期）

- [x] 6.1 `README.md`：增补「桌面应用（开发中）」章节（工具链版本组合、构建与运行命令、数据目录位置、路线图一句话），保留 Python 版章节；验证：按 README 步骤可复现构建；守卫检查（既有约定）零命中 —— 证据：提交 `f90c474`；构建命令逐条取自本次实跑；守卫 grep（机器名 / se77 / 真实网段 / 部署路径）在 README、AGENTS 与新应用源码零新增命中（README 中既有 `192.168.1.x` 为通用示例地址，非机器事实）
- [x] 6.2 `AGENTS.md`：增补「过渡期双实现」章节（规则适用范围、版本双源、测试命令分列，按 design D11）；验证：与 design D11 逐条一致 —— 证据：提交 `f90c474`；含状态行更新、开发流程第 6 条双版本源改写、代码约束适用性注记、项目结构更新
- [x] 6.3 `openspec/config.yaml`：context 增补新应用技术栈与过渡期事实；验证：`openspec validate --strict --no-interactive` 通过 —— 证据：提交 `f90c474`；`openspec validate "add-desktop-app-foundation" --strict --no-interactive` → valid；`openspec validate --specs --strict --no-interactive` → 1 passed

## 7. 待用户验收清单（需在 Windows 机器上人工操作）

- [ ] 7.1 按 README 在 Windows 机器备好构建工具链并构建应用；**预期**：构建成功，产出可运行的应用
- [ ] 7.2 运行应用核对信息面板；**预期**：数据目录位于 `%APPDATA%\agent-bridge`，UUID、短名占位、系统信息展示正确
- [ ] 7.3 设置短名并重启应用；**预期**：短名持久生效；再把 `config.toml` 手工改坏一次；**预期**：启动时生成 `bak-<时间戳>` 备份、重建配置并出现提示

> 以上三项在归档时保留未勾选（实施环境无 Windows 机器）。结果如与预期不符，另立修复变更处理。

## 8. 收尾

- [x] 8.1 分提交推送（脚手架 / Rust 核心 / 桥接与 GUI / 测试 / 文档各自成提交）；验证：`git status` 干净、与远端一致、已推送提交不改写历史 —— 证据：`2f4b13c`（子项目基座含核心与界面与测试）、`31fbe5a`（错误文案清理与测试加固）、`52d2b13`（版本推进）、`f90c474`（过渡期文档）；均已推送 `main -> main`，未改写历史
- [x] 8.2 证据登记：勾选各项并附证据（提交哈希 / 命令 / 退出码 / 产物名）；未在目标平台（Linux / Windows）实跑的不勾选 —— 证据：本次更新即登记；§7 三项（Windows 侧）如实保留未勾选
- [x] 8.3 归档前版本推进：新应用版本源 `0.0.0` → `0.1.0`（按 AGENTS.md 开发流程第 6 条，「归档即 bump」延续适用于新版本源）；验证：界面展示与代码常量一致，提交并推送 —— 证据：提交 `52d2b13`；`app/rust/Cargo.toml` 与 `Cargo.lock`、`app/pubspec.yaml` 同步为 0.1.0；实跑截图确认面板「应用版本 0.1.0」

## 9. 跟进项（本变更不实现，记录于此）

- [ ] 9.1 服务端核心（hello / exec / download）、双 token（会话 + 长期）、多设备配置与短名寻址、`agent-bridge` CLI 与 PATH 注册 —— 变更 ② / ⑤
- [ ] 9.2 提权、防火墙、托盘常驻、开机自启、剪贴板导出 —— 变更 ③（数据目录按调用者用户解析的注入点已在 design D5 预留；提权后 Linux root GUI 的兼容处理亦属该变更）
- [ ] 9.3 局域网发现与配对 —— 变更 ④
- [ ] 9.4 macOS 平台目录与适配、应用 ID 最终取值与图标 —— 需要时另立
