# agent-bridge-app Specification

## Purpose

定义 agent-bridge 新一代跨平台桌面应用（Windows / Linux 图形界面）的行为契约：应用形态与数据目录、设备身份（UUID 与本机默认短名）、配置文件契约与启动信息面板。该能力以「子目录并行、逐步替换」方式接替 `agent-bridge` 能力所描述的 Python 实现，过渡期两者并存。

## Requirements

### Requirement: 应用形态与数据目录
应用 SHALL 为跨平台桌面应用（图形界面），目标平台 Windows 与 Linux。应用 SHALL 在**当前用户**的用户级数据目录维护全部持久状态：Linux 为 `$XDG_CONFIG_HOME/agent-bridge`（未设置该环境变量时为 `~/.config/agent-bridge`），Windows 为 `%APPDATA%\agent-bridge`。该目录在首次启动时 SHALL 被自动创建；应用 MUST NOT 要求安装目录可写，MUST NOT 把持久状态写入安装目录。

#### Scenario: 首次启动创建数据目录
- **WHEN** 在全新环境首次启动应用
- **THEN** 用户级数据目录被自动创建，其中含应用生成的配置文件

#### Scenario: 既有数据目录被沿用
- **WHEN** 数据目录及其中的配置文件已存在时启动应用
- **THEN** 应用沿用既有 UUID 与短名等取值，不重建、不覆盖

#### Scenario: 应用可在无写权限的安装位置运行
- **WHEN** 应用以只读的安装目录部署并启动
- **THEN** 应用正常运行，全部持久状态写入用户级数据目录而非安装目录

### Requirement: 设备身份 UUID
应用 SHALL 在首次启动时自动生成设备 UUID（v4 随机）并持久化于配置文件；此后每次启动 SHALL 沿用同一 UUID。UUID SHALL 唯一标识本设备，应用在正常使用流程中 MUST NOT 改变或重新生成它。UUID SHALL 在启动信息面板中可见。

#### Scenario: 首次启动生成并持久化
- **WHEN** 首次启动应用，随后重启应用
- **THEN** 配置文件中存在新生成的 UUID，且重启前后该值不变

#### Scenario: 多设备 UUID 互不相同
- **WHEN** 在两台不同设备上分别首次启动应用
- **THEN** 两台设备展示的 UUID 不相同

### Requirement: 本机默认短名
应用 SHALL 允许用户为当前设备设置默认短名并持久化；短名 SHALL 满足：去除首尾空白后为 1–32 个字符、MUST NOT 含空白或控制字符；比较与判重 SHALL 不区分大小写。用户 SHALL 能清空短名；未设置（或已清空）时短名 SHALL 视为空，应用 MUST NOT 自动代填（如主机名）。

#### Scenario: 设置并持久化
- **WHEN** 用户设置一个合法短名并重启应用
- **THEN** 信息面板展示该短名，配置文件中取值一致

#### Scenario: 非法短名被拒绝
- **WHEN** 用户提交含空白或控制字符、超过 32 字符、或去除首尾空白后为空的短名
- **THEN** 应用拒绝保存并给出可读提示，原短名保持不变

#### Scenario: 清空短名
- **WHEN** 用户清空短名
- **THEN** 应用持久化为「未设置」状态，面板相应展示，不再使用任何代填值

### Requirement: 启动信息面板
应用启动后 SHALL 自动展示本机信息面板，无须用户先执行任何操作。面板 SHALL 展示：应用版本、设备 UUID、本机短名（未设置时为明确占位）、操作系统名称与版本、区域与语言（系统区域设置）、当前本地日期与时间、CPU 型号与核心数、内存总量与可用量、本机局域网 IP 地址列表（无地址时以明确占位表示）、服务端运行状态（含端口或失败原因）、**管理员权限状态（含受限模式提示）**、**防火墙状态（已放行 / 未激活无需放行 / 未检测到受支持管理器需手动放行）**。各取值 SHALL 来自当前系统的实际状态。

#### Scenario: 面板字段齐全且与系统一致
- **WHEN** 应用启动后查看信息面板
- **THEN** 上述字段全部展示；抽查 CPU 核心数、内存总量与局域网 IP，与操作系统自身工具的输出一致

#### Scenario: 无局域网地址时明确占位
- **WHEN** 设备当前没有任何局域网 IP 地址
- **THEN** 面板以明确占位（如「无」）表示，而非空白或错误值

#### Scenario: 权限与防火墙状态可见
- **WHEN** 应用以受限模式启动（提权不可得）或防火墙未检出受支持管理器
- **THEN** 面板相应行明确展示受限原因与「需手动放行 37777/tcp」类指引

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

### Requirement: 服务端启动与端口
应用随启动 SHALL 在本机启动服务端：监听 TCP 端口 37777 并绑定全部网络接口。端口已被占用时 MUST NOT 自动更换端口——SHALL 输出指明原因的错误，并在应用界面显著提示（含排查方向）。应用同时 SHALL 在 UDP 端口 37778 提供发现信标监听（见「局域网发现信标」）；该 UDP 端口被占用时 MUST NOT 阻断 TCP 服务——发现功能标记不可用并提示。应用退出时服务端 SHALL 随之停止（托盘常驻等生命周期调整见后续变更）。

#### Scenario: 启动即提供通道
- **WHEN** 应用启动完成
- **THEN** 服务端在 37777 端口可用，携带有效 token 的 `POST /hello` 可被调用

#### Scenario: 端口被占用时明确失败且可见
- **WHEN** 37777 已被其他进程监听时启动应用
- **THEN** 服务端不启动、不尝试其他端口，应用界面显著提示端口占用及排查方向

#### Scenario: UDP 端口占用不阻断主服务
- **WHEN** UDP 37778 已被其它进程占用时启动应用
- **THEN** TCP 服务照常提供全部既有能力，界面提示发现功能不可用

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

### Requirement: 管理员权限运行
应用启动时 SHALL 检测并以管理员/root 权限运行：已是管理员（Unix euid 0 / Windows 管理员令牌）→ 直接继续；否则在图形会话且系统提权通道可用时（Linux：`pkexec`）SHALL 以待提权方式重启自身并退出本实例（重启须显式传递显示会话与调用者数据目录相关环境，使界面正常且配置落在调用者用户目录）。提权被取消、失败或不可得（无图形会话、无提权通道）时 MUST NOT 静默继续或反复重试：SHALL 以显著方式在界面提示「受限模式」并说明原因，防火墙放行步骤标记为未执行，其余功能保持可用。数据目录 SHALL 始终落在**调用者用户**的用户级目录：提权路径显式传递调用者的 `XDG_CONFIG_HOME`；经 `sudo` 启动且以 root 运行的情形 SHALL 依据 `SUDO_USER` / `PKEXEC_UID` 解析调用者家目录（MUST NOT 把配置与日志写进 root 用户目录）。

#### Scenario: 已是管理员时直接运行
- **WHEN** 以 root（或 Windows 管理员）启动应用
- **THEN** 不发起提权请求，服务端与界面正常工作

#### Scenario: 非管理员图形会话触发提权重启
- **WHEN** 以普通用户在图形会话启动应用且系统提供提权通道
- **THEN** 应用以待提权方式重启自身为新实例（界面正常显示），本实例退出；重启实例的数据目录仍为调用者用户目录

#### Scenario: 提权不可得时为受限模式
- **WHEN** 非管理员且无图形会话或提权通道不可用（或被用户取消）
- **THEN** 应用不退出，界面显著提示受限模式及原因；防火墙步骤标记未执行；其余功能可用

#### Scenario: sudo 启动的数据目录归属调用者
- **WHEN** 以 `sudo` 启动且环境包含 `SUDO_USER`
- **THEN** 配置与日志写入该调用者的用户级数据目录，而非 `/root`

### Requirement: 防火墙自动放行
服务端启动后应用 SHALL 检测本机防火墙并自动放行**所需端口**：实际服务端口（TCP）与发现信标端口（UDP 37778，仅当发现功能启用时）。Linux 依次探测 ufw、firewalld（命令输出 SHALL 以 `LC_ALL=C` 等固定区域执行以保证解析稳定），Windows 探测 Defender 防火墙。检测到活跃防火墙时 SHALL **幂等**加入放行（已存在等效规则则跳过）；不活跃或未检测到受支持的活跃管理器时 SHALL 记录明确状态（含「如有自管规则请手动放行所需端口」提示），MUST NOT 阻断服务。应用对防火墙的改动 SHALL 限于本工具的放行规则本身，MUST NOT 改动其它防火墙配置。检测与放行结果 SHALL 反映在启动信息面板。

#### Scenario: 活跃防火墙下幂等放行
- **WHEN** 本机 ufw（或 firewalld / Defender）处于活跃状态
- **THEN** 服务端口（TCP）与发现端口（UDP 37778）均被加入放行；重复启动不重复添加；面板显示已放行

#### Scenario: 不活跃的防火墙被跳过
- **WHEN** 检测到管理器存在但未激活（如 ufw inactive）
- **THEN** 不修改任何规则，面板说明「未激活、无需放行」

#### Scenario: 未检出受支持管理器时如实报告
- **WHEN** 仅存在裸 nftables/iptables 等不受支持的管理机制
- **THEN** 不改动规则，面板提示「未检测到受支持的活跃防火墙；如有自管规则请手动放行所需端口」

### Requirement: 托盘常驻与开机自启
托盘可用时，关闭主窗口 SHALL 隐藏窗口至系统托盘而非退出（服务端保持运行）；托盘菜单 SHALL 至少含：显示窗口、复制本机配置、退出（退出 SHALL 停止服务端并结束进程）。托盘不可用（如无 StatusNotifier host）时，应用 SHALL 降级为「关闭窗口即退出」并在界面/日志明确提示，MUST NOT 形成无入口的幽灵进程。开机自启 SHALL 提供界面开关（**默认关闭**），状态以系统机制现状为准：Linux 写入/删除 `~/.config/autostart` 下本应用的 `.desktop`（指向本应用；提权由应用启动逻辑自行请求）；Windows 写入/删除 `HKCU\...\Run` 项；开启与关闭 SHALL 幂等。

#### Scenario: 关窗隐藏且通道持续
- **WHEN** 托盘可用时用户关闭主窗口
- **THEN** 窗口隐藏、进程与服务端保持运行，`/hello` 仍可被调用

#### Scenario: 托盘菜单可用
- **WHEN** 通过托盘菜单选择「显示窗口」或「退出」
- **THEN** 分别为恢复窗口 / 停止服务端并退出进程

#### Scenario: 托盘不可用降级
- **WHEN** 运行环境不提供托盘宿主
- **THEN** 应用提示托盘不可用，关闭窗口即退出（行为与升级前一致）

#### Scenario: 开机自启开关幂等
- **WHEN** 用户开启（或关闭）开机自启两次
- **THEN** 系统机制中恰好存在（或恰好不存在）一份本应用的自启项，无重复

### Requirement: 配置片段复制到剪贴板
界面 SHALL 提供「复制本机配置」操作：生成可直接粘贴进对端 `config.toml` 的 TOML 片段（`[[peer]]` 段：设备 UUID、本机短名（未设置时省略该键）、局域网地址、端口、**本次会话短期 token**——应用重启即失效）并写入系统剪贴板。地址选择 SHALL 排除容器/虚拟网桥等虚拟接口并优先私网地址；无可用地址或服务端未运行时 MUST 明确报错并给出原因，MUST NOT 复制残缺片段。

#### Scenario: 复制含会话 token 的片段
- **WHEN** 服务端运行中点击「复制本机配置」
- **THEN** 剪贴板中出现完整 `[[peer]]` 片段；将其粘贴进对端配置后，对端以该片段即可连通本机（token 为会话 token）

#### Scenario: 会话 token 随重启失效
- **WHEN** 应用重启后对端仍使用旧片段中的 token
- **THEN** 对端请求被 404 拒绝（会话 token 已轮换）——属预期语义

#### Scenario: 无可用地址时报错
- **WHEN** 本机没有可用的非虚拟局域网地址（或服务端未运行）
- **THEN** 操作失败并明确提示原因，不复制残缺片段

### Requirement: 局域网发现信标
应用 SHALL 在固定 UDP 端口 37778 上进行局域网发现：每 3 秒向局域网广播一次信标并同时监听该端口。信标 SHALL 为 JSON，含：协议版本、设备 UUID、本机短名（未设置时为 null）、主机名、服务端口（TCP）；MUST NOT 携带任何凭据（token）。收到信标后 SHALL 记入「最近活跃设备」表（含来源 IP 与时刻），同一 UUID 以最新为准；30 秒未再收到该 UUID 的信标即从表移除。本机 UUID 的自播 SHALL 被忽略；已配对的 UUID SHALL 在呈现时标记为已配对。UDP 端口被占用时应用 MUST NOT 阻断 TCP 服务：SHALL 将发现功能标记为不可用并在界面提示。

#### Scenario: 双实例互见
- **WHEN** 同一局域网内两台设备各自运行应用
- **THEN** 各自在数秒内（广播周期 3 秒）于「最近活跃设备」中出现对端（含短名与来源 IP）

#### Scenario: 自我广播被忽略
- **WHEN** 本机收到自身发出的信标
- **THEN** 不进入设备列表

#### Scenario: 过期移除
- **WHEN** 某设备停止广播超过 30 秒
- **THEN** 其条目从设备列表移除

#### Scenario: 信标不含凭据
- **WHEN** 抓取任一信标内容
- **THEN** 其中不存在任何 token 取值（仅有协议版本、UUID、短名、主机名与端口）

#### Scenario: UDP 端口占用不阻断主服务
- **WHEN** UDP 37778 已被其它进程占用时启动应用
- **THEN** TCP 服务与既有功能照常，界面提示发现功能不可用

### Requirement: 图形化配对
应用 SHALL 提供免 token 的配对请求端点 `POST /pair/request`（全应用唯一免认证端点）：请求体含请求方 UUID、短名（可空）、服务端口；来源 IP 由连接取得。服务端 SHALL 将该请求呈现给使用者（界面对话框显示请求方 UUID、短名与来源 IP），并保持请求挂起等待决定（上限 120 秒）。同时 SHALL 仅处理一条待决请求，其余请求 MUST 得到明确的「忙」响应。使用者**同意**时，响应 SHALL 返回本机配置：UUID、短名（可空）、服务端口与**长期 token**；**拒绝**、超时或忙时 MUST NOT 返回任何配置。发起方在获得同意后 SHALL 自动把对端写入本机配置的 `[[peer]]` 段：uuid / short_name / port / token 取自响应，**address 取发现到的来源 IP**（防止多网卡下不可达）；同一 UUID 重配 SHALL 为幂等覆盖。免 token 端点在有决定之前 MUST NOT 向请求方暴露任何本机配置信息。

#### Scenario: 同意后自动写入并可连通
- **WHEN** 对端在对话框中点击同意
- **THEN** 发起方配置中出现该设备的 `[[peer]]` 条目（含长期 token），随后以 `agent-bridge hello <该设备>` 可连通

#### Scenario: 拒绝不写入
- **WHEN** 对端点击拒绝
- **THEN** 发起方得到明确回执、配置不发生变化

#### Scenario: 超时回执
- **WHEN** 请求挂起 120 秒内无人决定
- **THEN** 发起方收到超时回执，配置不发生变化

#### Scenario: 忙时明确拒绝
- **WHEN** 已有一条待决请求未决，又来一条新请求
- **THEN** 新请求得到「忙」的明确响应，不改动待决请求

#### Scenario: 幂等重配对
- **WHEN** 对已配对的同一 UUID 再次发起配对并获同意
- **THEN** 既有 `[[peer]]` 条目被更新覆盖而非重复添加

#### Scenario: 免 token 端点不泄露信息
- **WHEN** 在无人决定之前观察 `/pair/request` 的响应或错误内容
- **THEN** 其中不含任何本机配置信息（UUID、短名、token 等），仅含请求状态

### Requirement: 已配对设备列表与在线状态
应用 SHALL 展示已配对设备列表（短名及其冲突标记、UUID、地址、在线状态），并每 5 秒刷新一次在线状态：以对端 token 调用 `/hello`（单机 2 秒超时），状态 SHALL 区分：**在线**（hello 成功）、**在线但凭据失效**（404 拒绝——token 被重置或轮换，提示重新配对）、**离线**（连接失败或超时）。短名冲突标记 SHALL 沿用「多设备配置与短名寻址」的同一规则（冲突各方标记无效）。

#### Scenario: 三态呈现
- **WHEN** 对端在线 / 对端离线 / 对端运行但 token 已失效 三种情形下查看列表
- **THEN** 分别显示「在线」「离线」「在线但凭据失效（需重新配对）」

#### Scenario: 周期刷新
- **WHEN** 对端在列表可见期间上下线
- **THEN** 5 秒内列表状态随之更新
