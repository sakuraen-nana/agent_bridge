## Purpose

定义局域网远程执行桥（agent-bridge）的行为契约：被控端服务器的 token 生命周期与认证、hello / exec / download 三类 API 的输入输出与错误行为，token 文档的字段契约，以及 agent 侧客户端的扫描与调用行为。该能力是 agent 跨机器自主执行开发测试动作（拉仓库、触发验收、推提交、取会话包）的受控通道；作为独立工具，它可在任意被控机上部署于被控项目目录之外，充当类 ssh 的远程操作通道。

## ADDED Requirements

### Requirement: 服务启动与 token 生命周期
被控端服务器 SHALL 以 Python 标准库实现并在 Linux 与 Windows 上行为一致；每次启动 SHALL 即时生成随机 token（密码学安全随机源，≥24 字节熵）并醒目打印到控制台（同时输出监听端口、运行用户、工作目录、本机局域网 IP 列表与调用示例）。token SHALL 仅存于进程内存，服务重启后旧 token MUST 失效；服务 MUST NOT 将 token 写入文件、日志或任何持久化渠道。服务器 SHALL 监听固定端口 37777 并绑定全部网络接口；端口被占用时 MUST 输出明确错误并以非零退出码退出（MUST NOT 自动更换端口）。

#### Scenario: 启动输出可直接抄录
- **WHEN** 用户在被控机启动服务器
- **THEN** 控制台出现本次随机 token、端口 37777、运行用户、工作目录、局域网 IP 列表与调用示例，且文件系统与日志中不存在该 token

#### Scenario: 端口被占用时明确失败
- **WHEN** 端口 37777 已被其他进程监听时启动服务器
- **THEN** 服务器输出端口占用错误并以非零退出码退出，不尝试其他端口

### Requirement: 统一 token 认证
所有端点（含 hello）SHALL 要求请求携带正确的 token（URL 查询参数 `?token=`）；token 校验 SHALL 使用恒定时间比较。校验失败或缺失时服务 MUST 不产生业务响应（统一 404 并关闭连接），且对请求方的响应 MUST NOT 区分"token 错误"与"路径不存在"。被拒请求的请求体仅可限量读取用于本地显示，MUST NOT 被解析或执行。

#### Scenario: 无 token 请求被丢弃
- **WHEN** 任何端点收到缺失或不正确 token 的请求
- **THEN** 服务返回 404 并关闭连接，不执行任何业务逻辑，不泄露服务器能力信息；本地控制台同时留痕该请求（时间、来源、路径、token 状态与请求体预览）

#### Scenario: 正确 token 正常访问
- **WHEN** 请求携带当前有效的 token
- **THEN** 请求进入对应端点的正常处理流程

### Requirement: 请求留痕与本地回显
服务器 SHALL 在本地控制台以结构化、面向人类可读的排版实时留痕所有请求（含认证被拒者）：请求编号、时间、来源地址、方法、路径与查询串（token 一律脱敏显示）。有效 hello 请求 SHALL 完整显示响应 JSON；有效 exec 请求 SHALL 显示全部执行参数（command / cwd / timeout_seconds），并将子进程输出实时同步显示于控制台（不迟于客户端收到），结束后显示退出码、耗时与是否超时；有效 download 请求仅需显示目标文件的完整绝对路径与发送结果，不显示文件内容。被拒请求 SHALL 显示 token 状态与请求体预览。多个命令并发时，控制台输出允许交错。

#### Scenario: 被拒请求本地留痕
- **WHEN** 任一端点收到缺失或不匹配 token 的请求
- **THEN** 控制台出现该请求的时间、来源、路径、token 状态与请求体预览，且对请求方的响应仍为统一 404

#### Scenario: exec 输出本地同步可见
- **WHEN** 一条有效 exec 命令持续产出输出
- **THEN** 服务器控制台实时显示相同输出（不迟于客户端收到），结束后显示退出码与耗时

#### Scenario: download 仅显示目标路径
- **WHEN** 一条有效 download 请求被处理
- **THEN** 控制台显示该文件的完整绝对路径与发送结果，不显示文件内容

### Requirement: Hello 问候 API
`POST /hello` SHALL 返回 JSON：服务版本、主机名、运行用户名、平台与系统版本、服务器工作目录、本机局域网 IP 列表、服务启动时刻。该端点同样受统一 token 认证约束。

#### Scenario: 问候返回服务器基本信息
- **WHEN** 携带正确 token 调用 `POST /hello`
- **THEN** 响应为包含上述字段的 JSON，其中用户名与工作目录与运行服务器的用户及启动目录一致

### Requirement: Exec 命令执行 API
`POST /exec` SHALL 接受 JSON 请求体：`command`（必填，整条 shell 命令字符串）、`cwd`（可选，工作目录）、`timeout_seconds`（可选，超时秒数，缺省 1800）。命令 SHALL 以运行服务器的用户身份执行：POSIX 使用 `/bin/sh`，Windows 使用 cmd.exe；工作目录缺省为服务器启动时的工作目录，`cwd` 相对路径基于该目录解释。响应 SHALL 以 chunked 流式返回 NDJSON 事件：输出以 `{"type":"output","data":…}` 逐段推送（stdout 与 stderr 合并；解码 UTF-8 优先、不可解时回退平台本地编码，均 errors=replace），结束时以 `{"type":"exit","code":<退出码>,"duration_ms":<耗时>}` 收尾。命令超时 MUST 终止子进程并以 `timed_out:true` 的 exit 事件收尾；客户端在流式期间断开 MUST 终止子进程。命令之间 MUST NOT 强制互斥（并发语义由调用方负责）。

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
- **THEN** 以服务器启动时的工作目录为基准解释执行

#### Scenario: 长静默命令保持执行
- **WHEN** 命令执行期间长时间无任何输出（超过连接层内部超时阈值），且客户端保持连接
- **THEN** 子进程继续执行至结束，服务 MUST NOT 将静默期误判为客户端断开而终止命令

### Requirement: Download 文件下载 API
`POST /download` SHALL 接受 JSON 请求体 `{"path": "<文件路径>"}`（相对路径基于服务器工作目录解释），以 `application/octet-stream` 分块流式返回该文件的完整内容，并正确设置 Content-Length。路径不存在或指向目录时 MUST 返回明确的 JSON 错误（区分"不存在"与"是目录"）。

#### Scenario: 下载文件内容完整
- **WHEN** 携带正确 token 请求一个存在的文件路径
- **THEN** 响应体为该文件的完整字节内容且 Content-Length 与文件大小一致

#### Scenario: 下载路径错误明确报错
- **WHEN** 请求的路径不存在或为目录
- **THEN** 返回携带具体原因的 JSON 错误，而非空内容或半途截断

### Requirement: Token 文档契约
工具 SHALL 在仓库根提供 `bridge.local.md.example` 模板（字段：`token:` 必填、`host:` 选填记录最近一次服务器 IP、`updated:` 填写日期，含填写说明）。`bridge.local.md` MUST 通过 .gitignore 排除在 git 之外，且 MUST NOT 进入任何同步渠道。client SHALL 缺省从工具自身目录（与调用时的工作目录无关）读取该文件的 token 与缺省 host；文档 SHALL 允许含多组同名 `token` / `host` 字段（供一台开发机记录多台被控机），解析语义为后者覆盖前者。

#### Scenario: git 状态不含 token 文档
- **WHEN** 用户按模板创建并填写 bridge.local.md 后执行 `git status`
- **THEN** 该文件不出现在任何待提交列表中

#### Scenario: 模板字段齐全
- **WHEN** 查看仓库中的 bridge.local.md.example
- **THEN** 包含 token / host / updated 字段与填写说明

#### Scenario: 多组 token 按后者覆盖解析
- **WHEN** token 文档含多组同名字段（如先记 184 的 token/host，后记另一台被控机的）
- **THEN** 缺省取最后一组；调用方可用 `--host` / `--token` 参数覆盖以指向其他组

### Requirement: Client 扫描与调用
agent 侧客户端 SHALL 提供四个子命令：`scan`（并发 TCP 连接扫描指定 CIDR，缺省本机主网段 /24 与端口 37777；对开放者以 token 文档中的 token 调用 hello，将结果分为"已确认的 bridge 服务器"与"未知服务"两组输出）、`hello`（校验服务器并打印基本信息，host 缺省取文档 host 字段）、`exec`（流式转发命令输出到本地控制台，并以被控端命令的退出码作为自身退出码）、`download`（下载远程文件到本地，缺省保存为当前目录同名文件）。token 与 host SHALL 缺省读 token 文档、命令行参数可覆盖。

#### Scenario: 扫描定位并确认服务器
- **WHEN** 被控机在局域网内运行且客户端执行 scan
- **THEN** 扫描在秒级完成，输出该被控机 IP 为"已确认的 bridge 服务器"（token 验证通过），端口开放但 token 不匹配的其他主机归入"未知服务"

#### Scenario: exec 退出码透传
- **WHEN** 通过 exec 执行一条失败（非零退出）的远程命令
- **THEN** 客户端以相同的非零码退出，输出内容与远端命令输出一致

#### Scenario: token 失效的处置提示
- **WHEN** hello / exec / download 收到 404 拒绝
- **THEN** 客户端提示 token 可能已因服务器重启轮换，指引更新 token 文档后重试

### Requirement: 统一 Python 入口
工具 SHALL 提供唯一的跨平台启动入口 `run.py`（位于仓库根，仅用 Python 标准库实现），MUST NOT 依赖任何 .sh/.bat 薄封装。入口 SHALL 仅依赖目标机上任意可用的 Python 3（≥3.7）即可运行；解释器版本或依赖环境不满足时 MUST 输出明确的缺失项与修复指引并以非零码退出（MUST NOT 产生未处理堆栈，且入口文件 SHALL 保持可被 Python 2 解析以便版本提示可达）；入口 SHALL 识别操作系统并启动相应程序主体。入口 SHALL 依据自身文件位置确定其程序主体与所需路径，在任意工作目录下调用均须得到一致结果，MUST NOT 要求调用者先切换到仓库根。入口 SHALL 提供主体分发：缺省或 `server` 启动被控端服务，`scan` / `hello` / `exec` / `download` 转发到客户端且退出码透传。工具主体仅使用标准库，SHALL 跳过虚拟环境的检测与创建（不在目标机上产生额外环境要求）。

#### Scenario: 仅凭任意 Python 3 启动被控端
- **WHEN** 在 Windows 或 Linux 目标机上用任意可用的 Python 3 运行 `python run.py`（无参数或 `server`）
- **THEN** 入口完成解释器检测、识别 OS 后启动被控端服务，行为与直接运行程序主体一致（含启动横幅与 token 输出）

#### Scenario: 客户端子命令经统一入口转发
- **WHEN** 运行 `python run.py scan|hello|exec|download ...`
- **THEN** 请求被转发到客户端对应子命令，输出与退出码与直接调用客户端模块完全一致

#### Scenario: 环境不满足时明确报错
- **WHEN** 解释器版本低于 3.7，或使用 Python 2 启动入口
- **THEN** 入口输出具体的缺失项与 Python 3 运行方式指引，并以非零码退出，不产生未处理堆栈

#### Scenario: 任意工作目录下调用
- **WHEN** 在工具目录之外的其他目录下用绝对路径或相对路径运行该入口
- **THEN** 入口仍正确确定程序主体与 token 文档路径并启动对应主体，输出与在工具目录下运行完全一致

### Requirement: 自包含与独立部署
工具 SHALL 自包含：除工具自身目录外 MUST NOT 依赖任何被控项目或宿主仓库的文件（含其自举/共享库）。被控端 SHALL 可部署于被控项目目录之外，从该位置提供全部能力；远程对项目目录的任意改动（切换分支、切换提交、写文件）MUST NOT 影响工具进程与通道可用性，工具的存在也 MUST NOT 使被控项目工作树出现额外文件或 git 冲突。

#### Scenario: 部署于项目目录之外运行
- **WHEN** 把工具目录放在被控机上与任何被控项目无包含关系的位置（如用户目录或系统工具目录）并启动服务
- **THEN** 服务正常提供全部能力，被控项目工作树内不含工具文件

#### Scenario: 远程改动项目目录不影响通道
- **WHEN** 经该通道在被控机上对某被控项目执行切换分支或切换到不含工具文件的提交
- **THEN** 服务器进程继续运行，通道保持可用（工具文件不在该项目工作树内，不被其 git 操作触及）

#### Scenario: 工具目录可整体搬迁
- **WHEN** 把工具目录整体拷贝到另一台机器或另一个路径后运行入口
- **THEN** 无需任何仓库级配置或额外环境（如虚拟环境）即可正常工作，token 文档按新目录解析

#### Scenario: 无外部文件依赖
- **WHEN** 目标机上除工具目录外没有任何被控项目的源码或共享库
- **THEN** 工具的全部子命令仍可运行，不读取工具目录之外的任何文件
