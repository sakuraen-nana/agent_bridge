# 实现-规格逐条对照复核（tasks §2.6）

复核对象：本变更首版实现（`run.py`、`src/agent_bridge/`）对照 `specs/agent-bridge/spec.md`（10 条需求 / 32 条场景）。
复核方式：逐条走查规格的 requirement 与 scenario，给出实现位置与验证方式；**未覆盖即缺陷**。
复核结论：**10/10 条需求均有实现与验证；无未覆盖项**。

## 逐条对照

| 规格需求 | 实现位置 | 验证 |
| --- | --- | --- |
| 服务启动与 token 生命周期 | `src/agent_bridge/server.py`：`BRIDGE_PORT`/`BRIDGE_VERSION` 常量（L32-33）、`main()` 端口预检与绑定（L546-581）、`TOKEN = secrets.token_urlsafe(24)`（L565）、`build_banner()`（L515） | 手工：`python3 run.py` 启动，可见 token/端口/用户/工作目录/IP/启动时刻；二次启动 token 变化、旧 token 立即 404；端口占用时非零退出（`main()` L558-561） |
| 统一 token 认证 | `_authorized()`（L172，`hmac.compare_digest`）、`_reject_404()`（L184，统一 404、无 Server/Date 特征头、不解析请求体）、`_log_rejected()`（L235） | 用例：`AuthTest.test_missing_or_wrong_token_uniform_404`（三端点 × 缺失/错误 token）、`test_valid_token_accepted` |
| 请求留痕与本地回显 | `_open_block`/`_kv`/`_kv_lines`（L156-169，┌/│/└ 分块）、exec 实时回显（`_run_command_stream` L431 `_log` 先于 `_send_event`）、download 仅记路径（L472+）、hello 完整响应（L290+） | 用例：全套用例运行期间控制台可见结构化留痕（含被拒请求与 exec 输出同步）；手工：观察一次 exec 的 ┌/│/└ 分块 |
| Hello 问候 API | `_handle_hello()`（L290） | 用例：`HelloTest.test_hello_fields_match_environment`（字段与运行环境一致） |
| Exec 命令执行 API | `_handle_exec()`（L317，参数校验 400）、`_run_command_stream()`（L369：`shell=True`、stdout+stderr 合并、NDJSON 输出事件、超时/断开终止、`PYTHONIOENCODING` 注入）、`_decode_child_output()`（L136）、`_kill_process_tree()`（L107） | 用例：`ExecTest` 全部 8 项——流式输出与退出码、非零退出、超时 `timed_out:true`、cwd 基于启动目录、长静默不误杀、断开杀子进程、UTF-8 编码、参数校验 |
| Download 文件下载 API | `_handle_download()`（L472：分块流式、Content-Length、路径不存在/是目录两类错误） | 用例：`DownloadTest` 4 项——二进制逐字节一致与 Content-Length、相对路径基于启动目录、缺失 404、目录 400 |
| Token 文档契约 | `src/agent_bridge/client.py`：`parse_token_doc()`（L53，多组后者覆盖）、`load_token_doc()`（L69，缺失/未填时退出码 2 与指引）、`bootstrap.TOOL_ROOT`（自举模块，与调用工作目录无关）；模板 `bridge.local.md.example`；`.gitignore` 排除 `bridge.local.md` | 用例：`ParseTokenDocTest` 5 项、`HelloTest.test_missing_token_doc_exit_2`；手工：`git check-ignore -v bridge.local.md` 命中 |
| Client 扫描与调用 | `cmd_scan()`（L183，并发 TCP 扫描 + hello 验证分组，目标列表始终含 127.0.0.1）、`cmd_hello()`（L242）、`cmd_exec()`（L263，流式转发 + 退出码透传）、`cmd_download()`（L317|L327）、`resolve_target()`（L91，参数覆盖文档缺省）、`_exit_connect_error()`（L131，退出码 3）、404 处置提示（L43） | 用例：`ScanTest` 2 项 + `ScanNoServerTest`、`HelloTest.test_stale_token_hint_and_exit_4`、`ExecTest.test_exit_code_passthrough`、`DownloadTest.test_default_name_and_out_option` |
| 统一 Python 入口 | `run.py`：内联版本检测（L31）、自定位（L37-38）、子命令分发（L55-）、`python -m` 子进程分发与退出码透传、信息行仅走 stderr | 用例：`EntryStaticTest` 3 项、`EntryPy2ParseGuardTest` 3 项（含静态守卫与其局限说明）、`EntryRuntimeTest` 3 项（入口与直调模块 stdout 一致、hello、退出码透传） |
| 自包含与独立部署 | 全仓库无工具目录之外的依赖：`bootstrap.py` 自带自举（`fail`/`run_passthrough`/`soften_console` 三项，为入口实际所需的最小集）、`run.py` 只导入标准库与包内模块 | 用例：`ToolTree` 夹具把工具目录整体拷入临时目录运行（`EntryRuntimeTest` 全部用例即在副本上运行，等价于"搬迁后可用"）；手工：`grep` 断言无仓库外路径依赖 |

## 场景覆盖抽查（易漏项）

- "长静默不被误判为断开"：由 `BridgeHandler.timeout` 覆盖为 2s 的用例复现原缺陷路径（socket.timeout 继续等待 vs EOF 终止），通过。
- "客户端断开不遗留进程"：裸 socket 中途关闭 → 子进程被整组终止（用例以 marker 文件不出现为证）。
- "任意工作目录下调用"：入口在 `/tmp` 下以绝对路径运行，token 文档仍按工具根解析（`EntryStaticTest`、`EntryRuntimeTest`）。
- "部署于项目目录之外"：`ToolTree` 副本位于系统临时目录，与被操作目录无包含关系，全部入口用例通过。

## 已知遗留（记入 tasks §8 跟进项，不在本变更处理）

- `server.py` 子进程 stdout 管道未显式关闭，解释器退出前会报 `ResourceWarning`（不影响行为与端口/进程清理）；属既有实现细节，修复应另立变更。
