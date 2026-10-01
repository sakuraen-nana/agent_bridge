## ADDED Requirements

### Requirement: 服务端启动与端口
应用随启动 SHALL 在本机启动服务端：监听 TCP 端口 37777 并绑定全部网络接口。端口已被占用时 MUST NOT 自动更换端口——SHALL 输出指明原因的错误，并在应用界面显著提示（含排查方向）。应用退出时服务端 SHALL 随之停止（托盘常驻等生命周期调整见后续变更）。

#### Scenario: 启动即提供通道
- **WHEN** 应用启动完成
- **THEN** 服务端在 37777 端口可用，携带有效 token 的 `POST /hello` 可被调用

#### Scenario: 端口被占用时明确失败且可见
- **WHEN** 37777 已被其他进程监听时启动应用
- **THEN** 服务端不启动、不尝试其他端口，应用界面显著提示端口占用及排查方向

### Requirement: 请求留痕（日志文件）
服务端 SHALL 将每个请求（含被拒的）追加写入数据目录下的日志文件 `server.log`，面向人类可读：请求行（时间、来源地址、方法、路径）、token 脱敏状态、请求参数与处理结果。通过认证的请求参数 SHALL 完整记录；exec 的实时输出内容 MUST NOT 写入日志（仅记录命令、结束状态、退出码、耗时、是否超时与其输出字节数）；download 记录目标路径与发送结果，MUST NOT 记录文件内容。日志文件达到 1 MiB 时 SHALL 轮转：现文件改名为 `server.log.1`（替换旧轮转文件），新请求写入新的 `server.log`。token 的任何取值 MUST NOT 写入日志（只记脱敏状态与片段）。

#### Scenario: 被拒请求留痕
- **WHEN** 任一端点收到缺失或不匹配 token 的请求
- **THEN** `server.log` 出现该请求的时间、来源、路径与 token 脱敏状态，且不含请求体解析或执行的痕迹

#### Scenario: 成功请求参数完整留痕
- **WHEN** 一条有效 exec 请求被处理
- **THEN** 日志完整记录 `command` / `cwd` / `timeout_seconds` 与结束状态（退出码、耗时、是否超时），但不含其输出内容

#### Scenario: 日志轮转
- **WHEN** `server.log` 达到 1 MiB
- **THEN** 它被改名为 `server.log.1`，后续请求写入新的 `server.log`

### Requirement: 双 token 认证
所有端点（含 hello）SHALL 要求请求携带正确 token（URL 查询参数 `?token=`）。认证 SHALL 使用恒定时间比较，且 SHALL 接受两类 token 的任意其一：本次会话 token 或长期 token。校验失败或缺失时 MUST 不产生业务响应（统一 404 并关闭连接），响应 MUST NOT 区分「token 错误」「token 类型」与「路径不存在」。被拒请求的请求体仅可限量读取用于本地留痕，MUST NOT 被解析或执行。

#### Scenario: 会话 token 可用
- **WHEN** 携带本次启动的会话 token 调用任一端点
- **THEN** 请求进入正常处理流程

#### Scenario: 长期 token 可用
- **WHEN** 携带长期 token 调用任一端点
- **THEN** 请求进入正常处理流程

#### Scenario: 无效 token 统一 404
- **WHEN** 任一端点收到缺失或错误的 token
- **THEN** 返回 404 并关闭连接，不执行任何业务逻辑，对请求方不泄露任何能力信息

### Requirement: 双 token 生命周期
会话 token SHALL 在每次应用启动时即时生成（密码学安全随机源，≥24 字节熵）并仅存进程内存；服务端重启后旧会话 token MUST 失效；MUST NOT 写入文件、日志或任何持久化渠道。长期 token SHALL 在首次运行（配置中尚无该值时）生成并持久化于 `config.toml`，长期有效、仅可手动重置；重置后旧值 MUST 立即失效。长期 token 属凭据，MUST NOT 出现在任何日志；仅可在用户显式命令（CLI `token show` / `token reset`）或界面显式操作时输出。任何 token 取值都 MUST NOT 写入 `server.log`（留痕只记录脱敏状态）。

#### Scenario: 会话 token 随重启轮换
- **WHEN** 应用重启后以旧会话 token 调用端点
- **THEN** 被 404 拒绝；本次启动的新会话 token 可用

#### Scenario: 长期 token 跨重启有效
- **WHEN** 以长期 token 调用端点，随后重启应用再次调用
- **THEN** 两次均进入正常处理流程

#### Scenario: 重置长期 token
- **WHEN** 用户通过 CLI `token reset` 重置长期 token
- **THEN** 旧长期 token 立即被拒（404），新长期 token 生效且已持久化

#### Scenario: token 不落日志
- **WHEN** 任一请求被留痕
- **THEN** `server.log` 中不含任何 token 取值（仅状态与脱敏片段）

### Requirement: Hello 问候 API
`POST /hello` SHALL 返回 JSON：应用版本、设备 UUID、本机短名（未设置时为 null）、主机名、运行用户名、系统名与版本、平台标识、默认工作目录、本机局域网 IP 列表、服务启动时刻。该端点同样受统一 token 认证约束（双 token 任一）。

#### Scenario: 问候返回设备与系统信息
- **WHEN** 携带有效 token 调用 `POST /hello`
- **THEN** 响应包含上述全部字段；UUID 与本机配置文件一致，短名与本机配置一致，默认工作目录为配置的 `workdir`（缺省时为用户主目录）

### Requirement: Exec 命令执行 API
`POST /exec` SHALL 接受 JSON 请求体：`command`（必填，整条 shell 命令字符串）、`cwd`（可选，工作目录）、`timeout_seconds`（可选，超时秒数，缺省 1800）。命令 SHALL 以运行应用的用户身份执行：POSIX 使用 `/bin/sh -c`，Windows 使用 `cmd /C`；工作目录缺省为配置的 `workdir`，`cwd` 相对路径基于该目录解释。响应 SHALL 以 chunked 流式返回 NDJSON 事件：输出以 `{"type":"output","data":…}` 逐段推送（stdout 与 stderr 合并；解码 UTF-8 优先、不可解时回退平台本地编码，均 errors=replace），结束时以 `{"type":"exit","code":<退出码>,"duration_ms":<耗时>}` 收尾。命令超时 MUST 终止进程树并以含 `"timed_out":true` 的 exit 事件收尾；客户端在流式期间断开 MUST 终止进程树。命令之间 MUST NOT 强制互斥（并发语义由调用方负责）。

#### Scenario: 长命令流式返回
- **WHEN** 执行一条持续产出输出的长命令
- **THEN** 客户端在命令运行期间即陆续收到 output 事件，结束后收到携带真实退出码的 exit 事件

#### Scenario: 超时终止进程树
- **WHEN** 命令运行超过 timeout_seconds
- **THEN** 进程树（含派生的后台子进程）被终止，响应以 timed_out 为 true 的 exit 事件收尾

#### Scenario: 客户端断开不遗留进程
- **WHEN** 客户端在命令执行期间断开连接
- **THEN** 服务器终止该命令的进程树，不遗留仍在运行的孤儿进程

#### Scenario: 相对工作目录
- **WHEN** 请求提供相对路径形式的 cwd
- **THEN** 以配置的默认工作目录为基准解释执行

### Requirement: Download 文件下载 API
`POST /download` SHALL 接受 JSON 请求体 `{"path": "<文件路径>"}`（相对路径基于默认工作目录解释），以 `application/octet-stream` 分块流式返回该文件的完整内容，并正确设置 Content-Length。路径不存在或指向目录时 MUST 返回明确的 JSON 错误（区分「不存在」与「是目录」）。

#### Scenario: 下载文件内容完整
- **WHEN** 携带有效 token 请求一个存在的文件路径
- **THEN** 响应体为该文件的完整字节内容且 Content-Length 与文件大小一致

#### Scenario: 下载路径错误明确报错
- **WHEN** 请求的路径不存在或为目录
- **THEN** 返回携带具体原因的 JSON 错误（区分两种情形），而非空内容或半途截断

### Requirement: 多设备配置与短名寻址
客户端配置 SHALL 支持多台设备条目（`[[peer]]`）：`uuid`（必填）、`short_name`（可空）、`address`（主机）、`port`（缺省 37777）、`token`（对端设备提供给本机的 token）。短名比较与判重 SHALL 不区分大小写（去首尾空白后按 Unicode 小写折叠，与「本机默认短名」同一口径）。**冲突规则**：同一比较键对应多台 peer 时，涉及的各 peer 短名 SHALL 全部视为无效——不得再以该短名寻址，直至用户改名解除冲突；**按 UUID 寻址 SHALL 恒可用**。以短名寻址无匹配时 SHALL 明确报错（未知名）；命中冲突键时 SHALL 报错并指出冲突涉及的各 UUID 与解除方式（改名或用 UUID）。短名与 UUID 寻址规则为 CLI 与后续 GUI 共享的同一实现。

#### Scenario: 短名精确寻址
- **WHEN** 某短名（比较键）唯一对应一台 peer，命令以该短名指代设备
- **THEN** 命令作用于该设备

#### Scenario: 大小写不敏感匹配
- **WHEN** 以不同大小写形式使用同一短名（比较键仍唯一）
- **THEN** 与原名等效命中

#### Scenario: 冲突各方全部无效化
- **WHEN** 两台 peer 的短名比较键相同（如「Dev」与「dev」）
- **THEN** 以该短名寻址任一设备均被拒绝（报错指出冲突涉及的 UUID 与解除方式）；以两台的 UUID 寻址均正常

#### Scenario: 未知名明确报错
- **WHEN** 以不存在的短名寻址
- **THEN** 明确报错（不匹配任何 peer），按本地配置错误类处置

### Requirement: 设备 CLI
应用 SHALL 提供命令行 `agent-bridge`（与 GUI 共用数据目录与 `config.toml`），子命令：
- `peers`：列出配置中的设备（短名及冲突标记、UUID、地址），不发起网络请求
- `hello <设备>`：校验并打印目标设备信息
- `exec <设备> <命令> [--timeout <秒>]`：流式转发输出；以远端命令退出码作为自身退出码
- `download <设备> <远程路径> [--out <本地路径>]`：缺省保存为当前目录同名文件
- `token show` / `token reset`：显示 / 重置本机长期 token（重置后打印新值）

`<设备>` SHALL 为短名或 UUID（寻址规则见「多设备配置与短名寻址」）。CLI 退出码 SHALL 沿用既有约定：0 成功（exec 时为远端退出码）／1 业务失败（如远端路径不存在）／2 本地配置与用法错误（含找不到设备、短名冲突）／3 网络连接失败或流式中断／4 token 被拒（404）。token 取值只可输出到终端，MUST NOT 写入任何日志文件。

#### Scenario: exec 退出码透传
- **WHEN** 通过 exec 执行一条失败（非零退出）的远端命令
- **THEN** CLI 以相同非零码退出，输出内容与远端命令输出一致

#### Scenario: 以 UUID 绕过短名冲突
- **WHEN** 短名冲突存在，用户以 UUID 指代设备执行 hello / exec / download
- **THEN** 命令正常作用于该设备

#### Scenario: peers 标记冲突
- **WHEN** 配置中存在短名冲突时运行 `peers`
- **THEN** 输出对冲突涉及的设备明确标记短名无效，并提示改用 UUID 或改名

#### Scenario: token reset 后旧值失效
- **WHEN** 运行 `token reset`
- **THEN** 打印新的长期 token；随后以旧值调用本机服务端被 404 拒绝

#### Scenario: token 被拒的处置提示
- **WHEN** hello / exec / download 收到 404
- **THEN** 提示 token 可能已失效（被重置或对端轮换），指引到对端重新获取

## MODIFIED Requirements

### Requirement: 配置文件契约（v2）
应用 SHALL 以数据目录中的配置文件 `config.toml` 持久化本能力的状态；编码 UTF-8、格式 TOML；含 `[device]` 段与可选的 `[[peer]]` 多段：

- `[device]`：`uuid`（设备身份）、`short_name`（可空）、`long_term_token`（长期凭据，首次运行自动生成）、`workdir`（默认工作目录；该键缺省时取用户主目录）
- `[[peer]]`：对端设备条目，字段与寻址规则见「多设备配置与短名寻址」

配置文件 SHALL 仅当前用户可读写（Linux 权限 0600；Windows 依赖用户目录的默认访问控制）；文件含长期 token，**属凭据文件——MUST NOT 入库、MUST NOT 进入任何同步渠道**。应用 SHALL 容忍手工编辑：内容合法时以文件为准；缺 `uuid` / `long_term_token` 等字段时 SHALL 视为缺省补全（生成并写回，不算损坏、不产生备份）；文件损坏（解析失败）时 MUST NOT 静默覆盖——SHALL 将原文件另存为同目录下文件名含 `bak` 与时间戳的备份，以默认值重建配置，并在界面提示用户。应用写入配置 SHALL 采用原子替换（先写临时文件再改名），MUST NOT 留下半写状态；未知键与注释 SHALL 在写入后保留。

#### Scenario: 手工编辑生效
- **WHEN** 用户手工修改配置文件（如 `short_name` 或新增 `[[peer]]`）后启动应用
- **THEN** 应用读取新内容生效，文件中的其他内容不被丢弃

#### Scenario: 补齐缺失的凭据字段
- **WHEN** 配置合法但缺 `long_term_token`（如被手工删除）
- **THEN** 应用生成新长期 token 并写回；不视为损坏、不产生备份文件

#### Scenario: 损坏文件备份后重建
- **WHEN** 配置文件内容无法解析（如手工编辑引入了语法错误）
- **THEN** 应用将原文件另存为含时间戳的备份、重建默认配置，并在界面提示用户；备份文件可找回原内容

#### Scenario: 权限仅当前用户
- **WHEN** 在 Linux 上首次启动应用后查看配置文件权限
- **THEN** 权限为 0600（仅当前用户可读写）

## RENAMED Requirements

- FROM: `### Requirement: 配置文件契约（首版）`
- TO: `### Requirement: 配置文件契约（v2）`
