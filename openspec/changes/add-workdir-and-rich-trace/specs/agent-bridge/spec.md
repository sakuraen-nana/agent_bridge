## MODIFIED Requirements

### Requirement: 服务启动与 token 生命周期
被控端服务器 SHALL 以 Python 标准库实现并在 Linux 与 Windows 上行为一致；每次启动 SHALL 即时生成随机 token（密码学安全随机源，≥24 字节熵）并醒目打印到控制台（同时输出监听端口、运行用户、工作目录、本机局域网 IP 列表与调用示例）。token SHALL 仅存于进程内存，服务重启后旧 token MUST 失效；服务 MUST NOT 将 token 写入文件、日志或任何持久化渠道。服务器 SHALL 监听固定端口 37777 并绑定全部网络接口；端口被占用时 MUST 输出明确错误并以非零退出码退出（MUST NOT 自动更换端口）。

服务器 SHALL 接受启动参数 `--workdir <路径>`，以其指定服务器的**默认工作目录**——即服务器全部相对路径的基准（见 Hello / Exec / Download 各 API 与启动横幅「工作目录」行）。未提供该参数时，默认工作目录 SHALL 为服务器进程启动时的工作目录（与既有一致）。启动横幅的工作目录行 SHALL 标注该目录的来源——由 `--workdir` 指定，或未指定而取启动时的工作目录。提供的路径无效时——不存在、存在但不是目录、存在且是目录但不可访问——服务器 MUST NOT 启动：MUST 输出指明具体原因的错误并以非零退出码退出，MUST NOT 退回默认目录、MUST NOT 改用其他目录继续启动。

#### Scenario: 启动输出可直接抄录
- **WHEN** 用户在被控机启动服务器
- **THEN** 控制台出现本次随机 token、端口 37777、运行用户、工作目录、局域网 IP 列表与调用示例，且文件系统与日志中不存在该 token

#### Scenario: 端口被占用时明确失败
- **WHEN** 端口 37777 已被其他进程监听时启动服务器
- **THEN** 服务器输出端口占用错误并以非零退出码退出，不尝试其他端口

#### Scenario: 以启动参数指定工作目录
- **WHEN** 以 `--workdir` 指向一个有效目录启动服务器
- **THEN** 启动横幅「工作目录」显示该目录并标注其来自启动参数，hello 响应的 `cwd` 字段为同一目录，且 exec 未提供 `cwd` 时以该目录为工作目录执行

#### Scenario: 工作目录无效时拒绝启动
- **WHEN** 以 `--workdir` 指向不存在、不是目录或不可访问的路径启动服务器
- **THEN** 服务器输出指明具体原因的错误并以非零退出码退出，服务不启动、不监听端口、不退回默认目录

#### Scenario: 未提供启动参数时行为不变
- **WHEN** 启动服务器时不提供 `--workdir`
- **THEN** 默认工作目录为服务器进程启动时的工作目录，横幅工作目录行标注其未由启动参数指定，行为与既有版本一致

### Requirement: 请求留痕与本地回显
服务器 SHALL 在本地控制台以分节式、面向人类可读的排版实时留痕所有请求（含认证被拒者）。每次请求 SHALL 以请求行开场（请求编号、时间、来源地址、方法、路径），查询串单独显示且 token 一律脱敏；其后各类信息 SHALL 按小节分节显示——小节以标题行起首、以空行与其他小节分隔，缩进与对齐保持一致。

对**通过 token 认证**的请求，请求参数与响应内容均 SHALL 完整显示：

- 参数 SHALL 完整显示：exec 显示 `command` / `cwd` / `timeout_seconds` 全部执行参数；download 显示调用方传入的原始 `path` 与解析后的完整绝对路径；hello 显示其请求信息。
- 响应 SHALL 完整显示：hello 的响应按字段显示；exec 的子进程输出（stdout 与 stderr 合并）实时同步显示于控制台（不迟于客户端收到），结束后显示退出码、耗时与是否超时；download 显示发送结果，MUST NOT 显示文件内容。
- 通过认证但因参数或路径无效被 400 / 404 拒绝的请求，其请求参数 SHALL 完整显示，与成功请求同等，并标明拒绝原因。
- 一次性整块显示的文本（如请求参数、响应 JSON）单段超过 64KB 时 MAY 截断，但 MUST 显式标注已省略的字节数，MUST NOT 静默省略；exec 的实时输出流不适用该上限，按原样实时转发。

对**未通过 token 认证**的请求，SHALL 维持既有行为：仅显示 token 状态与限量截断的请求体预览，MUST NOT 解析或执行请求体。

多个命令并发时，控制台输出允许交错。

#### Scenario: 被拒请求本地留痕
- **WHEN** 任一端点收到缺失或不匹配 token 的请求
- **THEN** 控制台显示该请求的时间、来源、路径、token 状态与限量截断的请求体预览，请求体未被解析亦未被执行，且对请求方的响应仍为统一 404

#### Scenario: exec 输出本地同步可见
- **WHEN** 一条有效 exec 命令持续产出输出
- **THEN** 服务器控制台实时显示相同输出（不迟于客户端收到），结束后显示退出码与耗时

#### Scenario: download 仅显示目标路径
- **WHEN** 一条有效 download 请求被处理
- **THEN** 控制台显示调用方传入的原始 `path`、解析后的完整绝对路径与发送结果，但不显示文件内容

#### Scenario: 留痕按小节分节排版
- **WHEN** 任一请求被留痕（无论认证通过与否）
- **THEN** 该请求的留痕以请求行开场，其后按小节显示：小节标题行起首、空行分隔、缩进对齐一致，参数与结果各自成节

#### Scenario: exec 请求参数完整显示
- **WHEN** 一条有效 exec 请求被处理
- **THEN** 控制台完整显示 `command` / `cwd` / `timeout_seconds` 三个参数（含缺省取值的标注）

#### Scenario: 认证通过但被拒的请求完整显示参数
- **WHEN** 一条通过 token 认证、但因参数或路径无效被 400 / 404 拒绝的请求
- **THEN** 控制台完整显示其请求参数（与成功请求同等），并标明拒绝原因，不使用截断预览

#### Scenario: 超长内容显式标注省略
- **WHEN** 一段一次性整块显示的文本（如请求参数或响应 JSON）超过 64KB
- **THEN** 控制台打印已省略的字节数，不静默丢弃内容

### Requirement: Hello 问候 API
`POST /hello` SHALL 返回 JSON：服务版本、主机名、运行用户名、平台与系统版本、服务器的默认工作目录、本机局域网 IP 列表、服务启动时刻。该端点同样受统一 token 认证约束。

#### Scenario: 问候返回服务器基本信息
- **WHEN** 携带正确 token 调用 `POST /hello`
- **THEN** 响应为包含上述字段的 JSON，其中用户名与运行服务器的用户一致，工作目录为服务器的默认工作目录（启动参数指定，未指定时为启动时的工作目录）

### Requirement: Exec 命令执行 API
`POST /exec` SHALL 接受 JSON 请求体：`command`（必填，整条 shell 命令字符串）、`cwd`（可选，工作目录）、`timeout_seconds`（可选，超时秒数，缺省 1800）。命令 SHALL 以运行服务器的用户身份执行：POSIX 使用 `/bin/sh`，Windows 使用 cmd.exe；工作目录缺省为服务器的默认工作目录（`--workdir` 指定，未指定时为服务器启动时的工作目录），`cwd` 相对路径基于该目录解释。响应 SHALL 以 chunked 流式返回 NDJSON 事件：输出以 `{"type":"output","data":…}` 逐段推送（stdout 与 stderr 合并；解码 UTF-8 优先、不可解时回退平台本地编码，均 errors=replace），结束时以 `{"type":"exit","code":<退出码>,"duration_ms":<耗时>}` 收尾。命令超时 MUST 终止子进程并以 `timed_out:true` 的 exit 事件收尾；客户端在流式期间断开 MUST 终止子进程。命令之间 MUST NOT 强制互斥（并发语义由调用方负责）。

#### Scenario: 长命令流式返回
- **WHEN** 执行一条持续产出输出的长命令
- **THEN** 客户端在命令运行期间即陆续收到 output 事件，命令结束后收到携带真实退出码的 exit 事件

#### Scenario: 超时终止
- **WHEN** 命令运行超过 timeout_seconds
- **THEN** 子进程被终止，响应以 exit 事件收尾且 timed_out 为 true

#### Scenario: 客户端断开不遗留进程
- **WHEN** 客户端在命令执行期间断开连接
- **THEN** 服务器终止该命令的子进程，不遗留仍在运行的孤儿进程

#### Scenario: 相对工作目录
- **WHEN** 请求仅提供相对路径形式的 cwd 或命令中的相对路径
- **THEN** 以服务器的默认工作目录为基准解释执行

#### Scenario: 长静默命令保持执行
- **WHEN** 命令执行期间长时间无任何输出（超过连接层内部超时阈值），且客户端保持连接
- **THEN** 子进程继续执行至结束，服务 MUST NOT 将静默期误判为客户端断开而终止命令

### Requirement: Download 文件下载 API
`POST /download` SHALL 接受 JSON 请求体 `{"path": "<文件路径>"}`（相对路径基于服务器的默认工作目录解释），以 `application/octet-stream` 分块流式返回该文件的完整内容，并正确设置 Content-Length。路径不存在或指向目录时 MUST 返回明确的 JSON 错误（区分"不存在"与"是目录"）。

#### Scenario: 下载文件内容完整
- **WHEN** 携带正确 token 请求一个存在的文件路径
- **THEN** 响应体为该文件的完整字节内容且 Content-Length 与文件大小一致

#### Scenario: 下载路径错误明确报错
- **WHEN** 请求的路径不存在或为目录
- **THEN** 返回携带具体原因的 JSON 错误，而非空内容或半途截断

### Requirement: 统一 Python 入口
工具 SHALL 提供唯一的跨平台启动入口 `run.py`（位于仓库根，仅用 Python 标准库实现），MUST NOT 依赖任何 .sh/.bat 薄封装。入口 SHALL 仅依赖目标机上任意可用的 Python 3（≥3.7）即可运行；解释器版本或依赖环境不满足时 MUST 输出明确的缺失项与修复指引并以非零码退出（MUST NOT 产生未处理堆栈，且入口文件 SHALL 保持可被 Python 2 解析以便版本提示可达）；入口 SHALL 识别操作系统并启动相应程序主体。入口 SHALL 依据自身文件位置确定其程序主体与所需路径，在任意工作目录下调用均须得到一致结果，MUST NOT 要求调用者先切换到仓库根。入口 SHALL 提供主体分发：缺省或 `server` 启动被控端服务，`scan` / `hello` / `exec` / `download` 转发到客户端且退出码透传。入口 SHALL 将紧随 `server` 的被控端启动参数（当前为 `--workdir <路径>`）原样转发至被控端程序主体，MUST NOT 改写、吞掉或重新解释这些参数。工具主体仅使用标准库，SHALL 跳过虚拟环境的检测与创建（不在目标机上产生额外环境要求）。

#### Scenario: 仅凭任意 Python 3 启动被控端
- **WHEN** 在 Windows 或 Linux 目标机上用任意可用的 Python 3 运行 `python run.py`（无参数或 `server`）
- **THEN** 入口完成解释器检测、识别 OS 后启动被控端服务，行为与直接运行程序主体一致（含启动横幅与 token 输出）

#### Scenario: 客户端子命令经统一入口转发
- **WHEN** 运行 `python run.py scan|hello|exec|download ...`
- **THEN** 请求被转发到客户端对应子命令，输出与退出码与直接调用客户端模块完全一致

#### Scenario: 被控端启动参数经入口转发
- **WHEN** 运行 `python run.py server --workdir <路径>`
- **THEN** 该参数原样到达被控端程序主体并按「服务启动与 token 生命周期」生效，退出码与直接运行程序主体完全一致

#### Scenario: 环境不满足时明确报错
- **WHEN** 解释器版本低于 3.7，或使用 Python 2 启动入口
- **THEN** 入口输出具体的缺失项与 Python 3 运行方式指引，并以非零码退出，不产生未处理堆栈

#### Scenario: 任意工作目录下调用
- **WHEN** 在工具目录之外的其他目录下用绝对路径或相对路径运行该入口
- **THEN** 入口仍正确确定程序主体与 token 文档路径并启动对应主体，输出与在工具目录下运行完全一致
