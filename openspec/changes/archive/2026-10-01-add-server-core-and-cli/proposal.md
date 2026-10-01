## Why

路线图第 2/5 步：Python 版的通道能力（hello / exec / download、token 认证、请求留痕）尚未迁入新应用——桌面应用目前只有界面与设备身份。本次把服务端核心与命令行落进 Rust，并按既定决策引入**协议 v2**：设备 UUID 与双 token（会话短期 + 长期手动重置）、客户端配置升级为多设备（`[[peer]]`）与短名寻址（短名唯一、冲突各方全部无效化）、交付 `agent-bridge` 命令行。完成后新应用具备与 Python 版对等的通道能力，并为后续变更（③ 权限与防火墙、④ 发现与配对、⑤ 打包发布）提供协议与配置基础。

## What Changes

- **Rust 服务端**（随应用启动的进程内服务）：监听 TCP 37777；端点 `POST /hello`、`POST /exec`、`POST /download`。`exec` 保持 NDJSON 流式事件（`output` / `exit`）、超时终止与断开终止语义；`download` 为 octet-stream + Content-Length、错误区分「不存在 / 是目录」；端口被占用时明确失败、不自动换端口
- **双 token 认证**：会话 token（每次应用启动轮换、仅存内存、不落盘）+ 长期 token（首次运行生成、持久化于 `config.toml`、仅手动重置）；认证接受二者其一，恒定时间比较，失败统一 404；token 值永不写入日志
- **请求留痕**：数据目录下日志文件 `server.log`（时间 / 来源 / 路径 / token 脱敏状态 / 参数 / 结果；1 MiB 轮转），替代 Python 版的控制台留痕
- **配置 v2**：`[device]` 增加 `long_term_token`、`workdir`（默认工作目录，缺省为用户主目录）；新增 `[[peer]]` 多段：`uuid` / `short_name` / `address` / `port` / `token`
- **短名寻址规则**：比较不区分大小写；同一短名（比较键）对应多台设备时，涉及的各 peer 短名**全部无效化**——只能按 UUID 寻址，直到改名解除冲突；寻址规则为 CLI 与（后续）GUI 共享
- **CLI `agent-bridge`**（Rust bin，与 GUI 共用数据目录与配置）：`peers`、`hello <设备>`、`exec <设备> <命令> [--timeout]`（远端退出码透传）、`download <设备> <路径> [--out]`、`token show|reset`（本机长期 token）；设备 = 短名或 UUID；退出码沿用 Python 版约定（0 成功 / 1 业务失败 / 2 用法或配置错误 / 3 网络失败 / 4 token 被拒）
- **GUI 最小联动**：应用启动即起服务端；服务端启动失败（如端口占用）在界面显著提示并说明排查方向
- **规格**：`agent-bridge-app` 新增「服务端启动与端口」「双 token 认证」「双 token 生命周期」「Hello（v2）」「Exec（v2）」「Download（v2）」「多设备配置与短名寻址」「设备 CLI」；「配置文件契约（首版）」更名并升级为「配置文件契约（v2）」

**BREAKING**：无。新应用尚无既有用户；Python 版不受影响（两实现各自独立运行，§形态兼容仅作验证手段）。

## Capabilities

### New Capabilities

（无——本次全部落在既有能力 `agent-bridge-app` 内）

### Modified Capabilities

- `agent-bridge-app`：新增服务端 / API / 双 token / 多设备配置与短名寻址 / CLI 五组要求；配置文件契约由首版升级为 v2

## Impact

- 新增 Rust 依赖：`axum` + `tokio`（HTTP 服务）、`reqwest`（CLI 客户端，禁用默认 TLS——仅局域网明文 HTTP）、`clap`（命令行解析）、`subtle`（恒定时间比较）、`rand`（token 随机源）、`futures-util`（流适配）；锁文件随提交更新
- 新增二进制目标 `agent-bridge`（同 crate 的 bin；PATH 注册属变更 ⑤）
- 数据目录新增 `server.log`（及其轮转文件）；`config.toml` 增加凭据字段——仍仅当前用户可读写（0600），不得入库
- GUI：应用启动流程接入服务端启停与错误提示（`app/lib/` 小改）
- 不受影响：Python 版全部文件；`agent-bridge` 能力主规格
- 验证：全部 API 与 CLI 语义可在 Linux 本机端到端验证（回环地址）；Windows 侧列入待用户验收清单；另以 Python 版客户端作跨实现互操作抽查
