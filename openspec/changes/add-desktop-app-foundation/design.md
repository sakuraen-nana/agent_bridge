## Context

动机与路线图见 `proposal.md`；行为契约见本变更差异规格（`specs/agent-bridge-app/spec.md`）与既有主规格。

实现面的现状与约束（决定本设计的取舍）：

- 仓库现为纯 Python 标准库工具（`run.py` + `src/agent_bridge/` + `tests/`），本次起冻结、**不改动**；新应用与其并存
- 已确认的关键决策（与用户逐项敲定）：子目录并行、逐步替换；**启动即全程管理员**（提权与防火墙属变更 ③）；托盘常驻 + 可选开机自启（变更 ③）；平台安装包分发、CLI 注册进 PATH（变更 ⑤）
- 后续变更依赖本变更定下的骨架：② 服务端核心与 CLI、③ 权限与防火墙、④ 发现与配对、⑤ 打包发布
- 开发机为 Linux，Windows 侧的构建与运行只能列入「待用户验收清单」，不虚标
- 仓库自足原则不变：文档不出现具体机器名与部署路径；语言中文；「归档即 bump」等既有规则延续

## Goals / Non-Goals

**Goals:**

- 建立 `app/` 子项目：Flutter 桌面前端 + Rust 核心（flutter_rust_bridge 生成绑定），Linux 上可构建、可测试、可运行；Windows 具备构建路径（列入用户验收）
- 落实本变更规格：设备 UUID、本机默认短名、数据目录与 `config.toml` 契约、启动信息面板
- 仓库级过渡期规则与文档就位（AGENTS.md / README / openspec config.yaml），供后续四个变更依循

**Non-Goals:**

- 不实现服务端/HTTP 协议/CLI/双 token/短名寻址（变更 ②）
- 不实现提权、防火墙、托盘常驻、开机自启、剪贴板导出（变更 ③）
- 不实现局域网发现与配对（变更 ④）
- 不做安装器与 PATH 注册（变更 ⑤）
- 不做 macOS 与移动端适配（`flutter create` 后续一条命令可补平台目录，本次不生成）
- 不改动 Python 版任何文件

## Decisions

### D1 子项目位置与工程布局：`app/`（Flutter 根 + 内嵌 Rust crate）

`app/` 为 Flutter 工程根（`app/lib/` Dart 代码、`app/linux/`、`app/windows/` 平台目录）；Rust crate 置于 `app/rust/`，采用 flutter_rust_bridge 的标准布局（含 `flutter_rust_bridge.yaml` 与生成的绑定层）。后续变更的 `agent-bridge` CLI（变更 ②）作为同一 crate 的 bin 目标构建，安装器（变更 ⑤）从该 crate 取产物。

- 理由：新应用是一个整体交付物，收进单一子目录使仓库根保持「Python 版 + `app/`」两清；frb 工具链按惯例就工作在这个布局上，避免自创布局与 codegen 打架。
- 备选：Rust 放仓库根 `rust/` 独立 workspace —— 否决（frb codegen 路径、Flutter 插件胶水都要额外配置，收益不明）；另起仓库 —— 否决（用户明确要求本仓库子目录）。

### D2 工具链与版本锁定：Flutter stable + Rust stable + flutter_rust_bridge v2

实施时取当时稳定版，**具体版本组合（Flutter / Dart / Rust / frb）写入 README 开发章节**；`app/pubspec.lock` 与 `app/rust/Cargo.lock` 均入库（应用而非库，锁死依赖）。Flutter 工程只生成 `linux` / `windows` 平台目录。

- 理由：frb 的版本矩阵敏感（codegen、runtime crate、Dart 包三者需配套），锁文件 + README 记录是唯一可复现的做法；平台目录按需生成可减少噪音，后续加 macOS 只需补平台目录。
- 备选：不锁版本 —— 否决（不同机器构建结果漂移，排障成本高）。

### D3 生成的绑定代码入库

frb codegen 产物（`frb_generated.rs` / `frb_generated.dart` 等）提交入库；仅在改动 Rust 桥接 API 时重跑 codegen 并单独提交。

- 理由：克隆仓库即可构建（无需先装 codegen 工具）；Android/CI 场景 frb 官方亦推荐入库。单独提交避免 codegen 噪音混进手写改动。
- 备选：build 时动态生成 —— 否决（给每个构建者强加 codegen 工具链，且 Flutter 侧热重载路径更脆）。

### D4 Rust 桥接面以「快照」为形态，Flutter 侧极薄

变更 ① 的桥接 API 只有两个方向：`初始化/读取快照`（返回版本、UUID、短名、系统信息等结构体）与 `写操作`（设置/清空短名，返回 `Result`）。错误用 `thiserror` 定义枚举经 frb 返回，Dart 侧只做展示与刷新，不持有业务状态副本；UI 用 Flutter 原生状态（`setState` + 服务封装），不引入状态管理库。

- 理由：业务真相在 Rust（配置文件、后续的 token 与协议），Flutter 只是视图；变更 ① 界面简单，库级状态管理收益为负。
- 备选：serde_json 字符串传参 —— 否决（丢失类型安全，frb v2 原生支持结构体）；引入 Riverpod/Provider —— 暂缓，界面复杂化（变更 ③④）时再评估。

### D5 Rust 依赖选型（变更 ① 范围）

`uuid`（v4 + serde）、`toml_edit`（配置读写）、`sysinfo`（OS/CPU/内存/网络接口）、`sys-locale`（区域与语言）、`chrono`（本地日期时间）、`thiserror` + `anyhow`（错误处理）、`flutter_rust_bridge`（桥接运行时）。

- `toml_edit` 而非 `serde` + `toml`：配置由应用管理又允许手工编辑，`toml_edit` 支持最小侵入更新——保留注释与未知键。变更 ② 扩展 `[[peer]]` 段（多设备配置）时可平滑演进；未来版本读到含未知段的配置也不会把它们吞掉。
- 网络地址取 `sysinfo` 的接口列表（复用现有依赖），而非 `local-ip-address` 等专用 crate。
- 数据目录解析**自行实现**（Linux 读 `XDG_CONFIG_HOME`/`HOME`，Windows 读 `APPDATA`），不用 `directories` crate：变更 ③ 引入提权后必须按「调用者用户」解析（`SUDO_USER`/`PKEXEC_UID`），自持解析便于注入覆盖，不让库的默认行为挡路。

### D6 数据目录与配置文件的落盘口径

- 目录：首次启动按需创建（Linux 上同时 `chmod 0700`）；文件 `config.toml` 以 0600 创建（Linux）；Windows 依赖用户目录默认 ACL，不额外处理。
- 写入：同目录临时文件 + `std::fs::rename` 原子替换（Windows 上 Rust 标准库经 `MoveFileExW` 支持覆盖改名）。
- 损坏（TOML 解析失败）：原文件改名为 `config.toml.bak-<时间戳>`，重建默认配置，界面提示；**合法但缺字段**（如有 `[device]` 段而无 `uuid`）不算损坏——视为缺省并补全写回。
- 首版 schema：`[device]` 段的 `uuid`、`short_name`；后续变更以新增段/键扩展，不做破坏性改动。

### D7 短名规则：Unicode 口径一次定死

去首尾空白（Unicode `trim`）后按 Unicode 标量计数 1–32；禁用 Unicode 空白与控制字符；判重比较键 = trim 后 Unicode 小写折叠。该口径同时是变更 ② 客户端配置中「短名唯一性/冲突无效化」的判定基础，规格已写明，实现不得偏离。

- 理由：规则跨变更共享（本机短名、对端短名、冲突判定三处），在骨架变更里定死可避免后续各变更各写一套。
- 备选：ASCII-only —— 否决（中文环境用户会输入中文短名）；不 trim —— 否决（粘贴带空格几乎必然发生）。

### D8 信息面板取值口径（可核对性优先）

| 字段 | 取值 | 备注 |
| --- | --- | --- |
| 应用版本 | Rust 常量（新应用版本源，起步 `0.0.0`） | 归档即 bump 延续适用，本变更归档时推进为 `0.1.0` |
| 平台 | `sysinfo` 的系统名与版本 | 展示随系统更新，不做映射美化 |
| 区域与语言 | `sys-locale` 的 BCP-47 标签（如 `zh-CN`） | 原样展示标签 |
| 时间 | `chrono` 本地时间，含时区偏移 | 面板载入时取值，另有手动刷新 |
| CPU | 型号字符串 + 物理核心数 | `sysinfo` |
| 内存 | 总量 + 可用量（人性化单位） | `sysinfo` |
| IP | 所有非环回、非 link-local 的 IPv4（地址 + 接口名） | 无地址时以「无」占位 |

- 理由：验收方式是「与系统工具输出抽查一致」（见规格场景），取值口径必须直白、可对照，不引入换算或美化误差。

### D9 应用标识与界面语言

产品名 `agent-bridge`；应用 ID 暂定 `io.agentbridge.app`（变更 ⑤ 打包时最终定）。界面文案中文，不引入 intl/l10n 框架；数据类字段（区域标签、IP 等）按原样展示。

- 理由：当前唯一使用群为中文；l10n 框架引入成本与收益在变更 ⑤ 再评估。

### D10 测试与验证分层

- Rust 单测（`cargo test`）：配置创建/沿用/损坏备份重建/原子写、UUID 持久、短名规则边界（长度、空白、控制字符、大小写折叠）、数据目录解析（环境变量注入）。
- Flutter widget 测试：信息面板字段渲染与短名编辑交互——桥接层抽一个可注入的 service 接口，测试注假实现，不依赖真 Rust 库。
- Linux 实跑（本机）：启动应用核对面板、修改短名、重启后核对持久化、检查配置目录与文件权限；全部留证。
- Windows：构建与运行列入「待用户验收清单」，不虚标。

### D11 仓库过渡期文档与规则改写

- **AGENTS.md**：新增「过渡期双实现」章节——两实现并存及各自适用规则（Python 版冻结：仅修致命缺陷，零依赖/单入口规则仅约束它；新应用 `app/`：Flutter + Rust，依赖以锁文件约束）；版本规则双源表述（新应用版本源以 `0.0.0` 起步、归档即 bump 适用于它；Python 版 `BRIDGE_VERSION` 冻结于 0.3.0）；测试命令分列（unittest 与 cargo/flutter 测试）。仓库定位、数据安全红线、提交约定等其余条款不变。
- **README.md**：顶部说明两形态；新增「桌面应用（开发中）」章节：构建前提（Flutter/Rust 工具链版本组合）、构建与运行命令、数据目录位置、功能进度一句话引用 OpenSpec；Python 版原章节保留（仍有效）。
- **openspec/config.yaml**：`context` 增补新应用技术栈与过渡期事实。

### D12 五个变更的边界文件（后续变更只增不挪）

本变更建立的文件与目录边界：`app/lib/`（Dart UI）、`app/rust/src/`（Rust：`config.rs` 配置、`identity.rs` 身份、`sysinfo_view.rs` 面板取值、`api/` 桥接面）、`app/rust/tests/`（单测）。变更 ② 在 `app/rust/src/` 内加 `server/`、`client/` 与 bin；变更 ③④ 主要动 `app/lib/` 与新增模块；变更 ⑤ 只动打包配置。边界内扩展，避免横向挪动。

## Risks / Trade-offs

- [frb/Flutter/Rust 版本矩阵漂移，构建在他机复现失败] → 版本组合写入 README，lock 文件入库；Linux 侧先行实跑验证
- [首次引入第三方依赖与代码生成，供应链面扩大] → 依赖取主流 crate 并锁 `Cargo.lock`；生成物入库使构建无需 codegen
- [Windows 构建与运行未经本机验证（`%APPDATA%` 路径、权限语义）] → 列入待用户验收清单，证据齐全才勾选
- [「全程管理员」运行对数据目录的影响（Linux root 下 `HOME=/root`）] → 变更 ③ 的议题；本设计以 D5 的自持解析预留注入点，本变更（不提权）不受影响
- [codegen 产物入库带来 diff 噪音] → 仅在 Rust API 变化时重跑、单独提交
- [Flutter Linux 构建依赖系统库（GTK3、clang、ninja 等），机器间差异] → README 记明前置依赖；不同发行版差异随用户验收暴露
- [手工编辑配置的注释在应用写入后是否保留] → `toml_edit` 保留注释与未知键；应用只做键值级更新，不整体重排

## Migration Plan

无部署态迁移：新应用此前不存在，Python 版使用方不受任何影响。回滚即回退提交（`app/` 整体移除、文档还原），无数据面影响。新应用自本变更起走「归档即 bump」（起步 `0.0.0`，本变更归档推进为 `0.1.0`）。

## Open Questions

- 应用 ID 的最终取值与图标资源 —— 变更 ⑤ 打包时定，不影响本变更产物
- 是否引入 Flutter 状态管理库与 intl/l10n —— 界面复杂度（变更 ③④）上来后评估
