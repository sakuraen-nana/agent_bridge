## Context

动机见 `proposal.md`；行为契约见本变更差异规格。实现面的现状与约束（决定取舍）：

- `app/` 已有：`config.rs`（数据目录解析、toml_edit 最小侵入读写、0600/0700、原子写、损坏备份重建）、`identity.rs`（UUID 与短名口径）、`sysinfo_view.rs`、`api/`（frb 桥接面）；frb 生成物入库。
- 本机实施环境：Linux 开发机（依赖走国内镜像）；Windows 只能列入用户验收；无图形会话时以 Xvfb + 独立 dbus 会话做 GUI 实跑（变更 ① 已建立的做法）。
- 后续变更依赖本变更的产出：③（权限/防火墙/托盘/剪贴板：用会话 token）、④（发现与配对：UDP 信标、配对返回长期 token）、⑤（打包：`agent-bridge` 二进制进 PATH）。
- Python 版冻结；其端点形态（同端口、同路径、同 NDJSON 事件）可作跨实现互操作抽查，但不是规格承诺。

## Goals / Non-Goals

**Goals:**

- Rust 服务端达到与 Python 版对等的通道能力，并叠加协议 v2：双 token、配置 v2（peers）、短名寻址与冲突无效化、日志文件留痕、`agent-bridge` CLI
- 全部 API 与 CLI 语义在 Linux 本机可端到端验证（回环地址 + 进程内服务）

**Non-Goals:**

- 托盘常驻、开机自启、提权、防火墙、剪贴板与 GUI 的 peer 管理界面 —— 变更 ③（GUI 只做本变更所需的最小接线）
- UDP 发现与配对流程 —— 变更 ④（本变更只备好 peers 数据形态与寻址规则）
- 安装包与 PATH 注册 —— 变更 ⑤（本变更交付 cargo 构建的 `agent-bridge` 二进制）
- 不做与 Python 客户端的兼容性承诺（仅作抽查手段）

## Decisions

### D1 HTTP 栈：axum 0.8 + tokio（独立 runtime）

- 理由：流式响应体（`Body::from_stream`）与分块 NDJSON 是本变更核心形态，axum 直接支持；生态成熟、错误处理清晰。
- 备选：裸 hyper（路由/流处理全手写，成本高）、tiny_http（阻塞式，流式与断开检测都别扭）——否决。
- 服务端运行在**专用 tokio runtime**（后台线程，`Runtime` 持于 `ServerHandle` 内）：生命周期独立于 frb 调用栈，`stop()` 即 drop runtime；不借用 frb 的运行时。

### D2 服务端启动形态与测试注入口

`server::start(ServerConfig) -> Result<ServerHandle>`，`ServerConfig { port: u16, data_dir: PathBuf, session_token: Arc<str>, long_term_token: Arc<RwLock<Arc<str>>>, workdir: PathBuf }`。GUI 于 `app_init` 时以默认端口 37777 启动；启动失败（如端口占用）不 panic——错误进入快照供界面展示（D13）。测试可用任意端口（进程内起服务）；另提供环境变量 `AGENT_BRIDGE_PORT` 仅作命令行实跑的临时覆盖口（不写入面向用户的文档，README 不宣传）。

### D3 token 生成、比较与更新

- 生成：`getrandom`（v0.3，纯随机源）取 32 字节 → 手写 hex 编码（64 字符，≥24 字节熵达标）。会话 token 每次进程启动生成一次；长期 token 在配置加载时缺则补全。
- 比较：`subtle::ConstantTimeEq` 逐字节恒时比较；对两类 token 各比较一次后取或（不做类型区分性早退），失败统一 404。
- 重置：配置写回 + `RwLock<Arc<str>>` 内存镜像即时更新——重置立即生效，无需重启。

### D4 配置 v2 读取与扩展（沿用 toml_edit 口径）

- 新 helper：读 `workdir`（缺省=用户主目录）、读/补全 `long_term_token`、`peers() -> Vec<Peer>`、`set_long_term_token()`。
- `[[peer]]` 解析容错：缺 `uuid` 或 `uuid` 非法、缺 `address` 的条目**跳过不显示**（文件原文保留、不改写）；`port` 缺省 37777；`short_name`/`token` 可空。
- 补全写回与损坏备份的既有语义（变更 ① 的 `load_document` 通道）不变：缺 `long_term_token` 属「缺省补全」，不算损坏、不备份。

### D5 短名寻址与冲突无效化（CLI 与后续 GUI 共享）

`resolve_peer(ref) -> Result<Peer, ResolveError>`：

1. `ref` 可解析为 UUID → 取**第一个** uuid 相等的条目（重复条目按文件序取先者，不视为冲突）；
2. 否则按比较键（trim + Unicode 小写折叠）统计：恰 1 条 → 命中；0 条 → `Unknown`；≥2 条 → `Conflict { uuids }`。
- `Conflict` 的报错文案给出涉及的各 UUID 与解除方式（改名或用 UUID）；CLI 退出码 2。
- 冲突是**逐键**判定：不同键之间互不影响；「全部无效化」= 该键下所有条目均不可用短名寻址。

### D6 exec：进程树终止与流式事件

- 生成命令：POSIX `/bin/sh -c`、Windows `cmd /C`；`tokio::process::Command`，`process_group(0)`（Unix 置子进程为组长）、stdout 与 stderr **合流**（stderr 接 stdout 管道）。
- 输出：读线程按块切分 → `{"type":"output","data":…}`；解码 UTF-8 优先，非法字节 Windows 上以 `encoding_rs` GBK 回退、其余平台以替换字符（errors=replace 语义）。
- 终止语义：超时（`tokio::time::timeout`）或客户端断开（axum drop Body 流 → 事件通道关闭）时 `killpg(-pid, SIGKILL)`（Windows 以 `taskkill /PID <pid> /T /F` 尽力阻止进程树）；Drop 守卫保证不遗留。
- 结束事件 `{"type":"exit","code":…,"duration_ms":…,"timed_out":…}`（`timed_out` 仅在超时时出现——与 Python 版形态一致）。
- 备选：stdout/stderr 分别读再交错 —— 否决（合并语义与 Python 版一致且实现简单）。

### D7 download：流式与错误 JSON

`tokio::fs::metadata` 预判：不存在 → 404 `{"error":"路径不存在…"}`；是目录 → 400 `{"error":"路径是目录…"}`；成功 → `Content-Length` + `ReaderStream`（tokio-util）分块。

### D8 请求留痕：`server.log`（1 MiB 轮转）

- 记录：请求行（时间/来源/方法/路径）、token 脱敏状态（如「缺失」「不匹配（前 4 位 abc…）」「有效」）、参数（认证通过者完整；被拒者仅限量体预览，不解析不执行）、结果摘要；exec 记 command/cwd/timeout 与结束状态（code/duration/timed_out/输出字节数），**不记输出内容**；download 记路径与发送字节数。
- 实现：`Mutex<File>`（追加）；写入前查大小 ≥1 MiB → 轮转（rename 覆盖 `server.log.1`）；格式化在锁内完成，避免交错。
- 备选：不落文件的纯界面日志 —— 否决（无 GUI 也能排障是本工具的基本诉求；界面日志视图属后续变更）。

### D9 CLI：clap + reqwest（流式），共享 lib 配置代码

- 子命令：`peers` / `hello <设备>` / `exec <设备> <命令> [--timeout]` / `download <设备> <路径> [--out]` / `token show|reset`（clap derive）。
- 网络：`reqwest`（`default-features=false`，仅 http1 + json + stream——明文 HTTP 是设计前提，不引 TLS）（hmm——reqwest 需要至少 http1 特性；TLS 不需要）。
- exec 流式：`bytes_stream` 按行切分 NDJSON，逐事件打印 `output`，`exit` 决定退出码；超时由**服务端**执行（CLI `--timeout` 透传）。
- exit code：0/1/2/3/4 映射见规格；`exec` 透传远端码（0–255）。
- 复用 `config.rs` / `identity.rs`（同 crate 的 lib 目标）：CLI 与 GUI 读同一份配置，短名解析同一实现。
- `token show|reset` 只操作本机配置，不发起网络请求。

### D10 二进制目标与测试通道

`app/rust/Cargo.toml` 增 `[[bin]] name = "agent-bridge"`（src/bin/agent-bridge.rs）。集成测试用 `env!("CARGO_BIN_EXE_agent-bridge")` 取得构建出的二进制，与进程内服务端做端到端（起服务 → 跑 CLI → 断言输出与退出码 → 停服务）。

### D11 GUI 最小接线

- `AppSnapshot` 增 `server: ServerSnapshot { running: bool, port: u16, error: Option<String> }`；`app_init` 时启动服务端并把结果并入快照；启动失败时 `error` 携带可读原因。
- `home_page.dart`：`server.error` 非空时显示显著 banner（与配置重建提示同形态）；正常时面板标注「服务端：运行中（端口 37777）」。
- 会话 token 的界面使用（剪贴板）留待变更 ③；本变更桥接面新增 `session_token()` 只读接口备用？——不，`不需要就不加`：③ 再加，避免死代码。
- 服务端停止：应用进程退出即结束（③ 的托盘生命周期再调整）。

### D12 交付边界（后续变更只增不挪）

新增模块：`src/server/{mod,state,auth,log,hello,exec,download}.rs`、`src/peers.rs`（寻址）、`src/bin/agent-bridge.rs`、`src/cli/{mod,…}.rs`（子命令实现，bin 薄壳）。配置扩展进 `src/config.rs`。

## Risks / Trade-offs

- [axum/tokio 等新依赖的获取与编译成本] → 锁文件入库；依赖走镜像（实施环境已配置）；不引入 TLS 等重依赖
- [Windows 进程树终止依赖 `taskkill /T`，语义不如 Unix 进程组干净] → 列 Windows 验收项；Unix 侧完整实现并在 Linux 全量验证
- [日志并发交错/轮转竞态] → 单 Mutex 内完成「判大小→轮转→写入」；单文件顺序追加
- [CLI 与 GUI 并发写配置（token reset 与 GUI 同时保存）] → 沿用原子替换写（最后写者胜）；README 说明「重命名/重置等操作建议单端进行」（不做跨进程锁，保持简单）
- [Python 版互操作抽查可能暴露形态差异] → 仅作验证手段：发现的差异按「新应用规格为准」记录，不因此改规格（除非破坏通道能力本身）
- [会话 token 每次启动轮换，对端配置里存会话 token 会在重启后失效] → 属设计预期：CLI 的 404 提示文案引导到对端重新获取（长期 token 或重新配对）；剪贴板/配对流程（③/④）负责分发长期 token

## Migration Plan

无部署态迁移（新应用尚无用户）。已有 `config.toml`（v1 内容）被就地升级：加载时补全新字段，原注释与未知键保留；无需人工操作。回滚即回退提交。

## Open Questions

- GUI 的 peer 管理界面（增删改与冲突高亮）放变更 ③ 还是 ④ —— 届时时评估，不影响本变更产出
