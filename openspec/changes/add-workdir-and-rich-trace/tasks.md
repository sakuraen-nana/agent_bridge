# Tasks: add-workdir-and-rich-trace

> 实施顺序：被控端参数（1）→ 入口转发（2）→ 留痕排版（3）→ 完整性上限（4）→ 测试（5）→
> 两平台实跑（6）→ 文档与收尾（7）。
> 决策依据见 design.md（D1 参数在主体解析、D3 目录校验口径、D4 `WORK_DIR` 改名、
> D5 退出码 2、D6 分节排版、D7 上限只作用于「块」）；行为范围以差异规格为准，不做规格外改动。
> 文档边界按既有约定：本仓库不记录使用方的机器名、IP 与部署路径，需指代环境处用通用角色。

## 1. 被控端启动参数 --workdir

- [x] 1.1 `server.py`：模块级 `START_DIR` 更名为 `WORK_DIR`（仍为模块级，导入时取启动 cwd），同步全部引用点；验证：全仓 `grep -rn "START_DIR"` 零残留，改动后既有用例全绿 —— 证据：提交 `f40907d`；`grep -rn START_DIR src/ tests/` 零命中，全量用例通过
- [x] 1.2 `server.py`：`main()` 接收 argv 并以 argparse 解析 `--workdir`；校验口径为「存在 + 是目录 + 可列出」（`isdir` 后再 `listdir` 探测，捕获 `OSError`），无效时向 stderr 输出指明具体原因的错误并以退出码 2 退出；验证：三类无效路径（不存在 / 不是目录 / POSIX 下 `chmod 000`）分别实跑，均得退出码 2、可读错误，且端口未被监听 —— 证据：提交 `f40907d`；三类实跑分别输出「路径不存在」「不是目录」「目录不可访问（Permission denied）」并退出码 2，端口未被监听（失败发生在绑定之前）。**实施发现**：以 root 运行时 `chmod 000` 构造不出「不可访问」（root 绕过 DAC），该例改用非特权用户端到端复验通过；用例据此按平台与 euid 跳过
- [x] 1.3 校验通过后将 `WORK_DIR` 覆盖为解析后的绝对路径（相对路径以启动时 cwd 为基准），且横幅工作目录行标注其来源（来自启动参数）；验证：横幅「工作目录」、hello 的 `cwd`、exec 未提供 cwd 时的工作目录三处一致且都等于 `--workdir` 所指目录，横幅该行可见来源标注 —— 证据：`--workdir <临时目录>` 启动后横幅为「工作目录 : <该目录>（来自 --workdir）」；`exec "pwd"` 输出该目录；留痕 `cwd = <该目录>（未提供 → 服务器默认工作目录）`；hello 响应 `cwd` 同值。相对路径一例由用例 `test_relative_workdir_resolves_against_startup_cwd` 覆盖
- [x] 1.4 未提供 `--workdir` 时行为不变；验证：与既有版本逐项比对启动输出，工作目录为启动时 cwd 且横幅标注「未由启动参数指定」，其余（token / 端口 / IP / 调用示例）形态不变 —— 证据：不传参数实跑，横幅为「工作目录 : <启动时目录>（未指定 --workdir，取启动时目录）」；用例 `test_without_option_keeps_startup_dir` 断言 `WORK_DIR` 与来源标志均未被改动

## 2. 入口转发

- [x] 2.1 `run.py`：确认 `server` 之后的参数原样到达程序主体（现有 `rest = args[1:]` 已满足，需用例锁定该契约）；验证：`python run.py server --workdir <路径>` 生效且退出码与 `python -m agent_bridge.server --workdir <路径>` 一致 —— 证据：分发逻辑无需改动；无效目录下两者 stdout 逐字节一致、退出码同为 2（用例 `EntryServerArgTest.test_workdir_forwarded_to_subject`）；有效目录下 `run.py server --workdir <目录>` 实跑生效
- [x] 2.2 `run.py`：确认不带 `server` 的 `--workdir` 报「未知子命令」并以码 2 退出，行为不变且提示可读；验证：实跑 `python run.py --workdir <路径>`，输出用法提示与退出码 2 —— 证据：实跑输出「[run] 未知子命令: --workdir」+ 用法提示，退出码 2；用例 `test_workdir_without_server_subcommand_is_unknown` 锁定

## 3. 留痕分节排版与参数完整显示

- [x] 3.1 新增 `_section(title)`（标题行 + 空行分隔）并将同节字段按最长键名对齐；请求行、结尾行与 `┌ │ └` 骨架保持不变；验证：实跑三类 API，控制台呈现小节标题、空行分隔与对齐字段 —— 证据：提交 `f40907d`；实跑留痕呈现 `▸ 请求参数` / `▸ 实时输出（stdout+stderr 合并）` / `▸ 响应` 小节，空行分隔；用例 `test_sections_use_titles_and_aligned_fields` 断言同节内等号落在同一列（中文键按两列计宽）
- [x] 3.2 exec 请求：`command` / `cwd` / `timeout_seconds` 三个参数完整显示于「请求参数」小节；验证：实跑后控制台可见三项完整值（含缺省标注） —— 证据：留痕为 `command = <命令原文>`、`cwd = <原值> → <解析后绝对路径>`（未提供时 `<默认工作目录>（未提供 → 服务器默认工作目录）`）、`timeout_seconds = 1800（缺省）`
- [x] 3.3 download 请求：显示调用方传入的原始 `path` 与解析后的完整绝对路径，随后显示发送结果；验证：实跑 `download run.py` 后两值同时出现，且不显示文件内容 —— 证据：留痕为 `path = hello.txt` 与 `解析后路径 = <绝对路径>`；用例 `test_download_shows_raw_and_resolved_path` 另断言不含目标文件首行内容
- [x] 3.4 hello 请求：响应按字段完整显示于「响应」小节；验证：实跑后响应各字段可读呈现 —— 证据：`▸ 响应` 下逐字段显示 version / hostname / user / system / release / platform / cwd / lan_ips / started_at，字段对齐
- [x] 3.5 通过认证但被 400 / 404 拒绝的请求：完整显示其请求参数（不再使用截断预览）并标明拒绝原因；验证：实跑「cwd 不存在」「download 指向目录」「未知路径」三类，参数完整可见 —— 证据：三类实跑留痕均完整显示参数（如 `command = echo x` 与 `cwd = /tmp/nope-xyz → …（不存在或不是目录）`）并收尾于 `已拒绝 · 400|404 …`；用例 `test_authenticated_rejection_shows_full_params`
- [x] 3.6 未通过认证的请求路径保持既有语义（token 状态 + 2048 字节截断预览、不解析不执行）；验证：错 token 与无 token 各实跑一次，控制台仅见截断预览，响应仍为统一 404 —— 证据：错 token 实跑留痕仅 `token: wrong-…（11 字符，与当前 token 不匹配）` 与 `请求体预览（仅预览，未解析未执行）`，无 `▸ 请求参数` 小节；客户端退出码 4；用例 `test_unauthenticated_keeps_truncated_preview`

## 4. 完整性上限

- [x] 4.1 新增单段文本上限（64KB，按 UTF-8 字节计，截断点回退到字符边界，尾部标注「已省略 N 字节」）并应用于请求参数块与响应块；验证：构造超限请求体实跑，输出出现省略标注、中文字符未被截碎、服务行为不受影响 —— 证据：70011 字节请求体实跑，留痕尾部为 `……已省略 4475 字节（单段上限 64KB）`（差额精确 = 70011 − 65536），服务仍按 404 正常响应；`_truncate_text('中'×100, 10)` 返回保留 3 字、省略 291 字节 → 多字节字符未被截碎；用例 `test_oversize_block_annotates_omitted_bytes`
- [x] 4.2 exec 的实时输出流不适用该上限；验证：实跑一条产出 >64KB 输出的命令，控制台与客户端收到的内容均完整、无省略标注 —— 证据：`seq 1 20000` 实跑，客户端收到 748943 字节；控制台留痕 20000 行一行不少、无省略标注；用例 `test_exec_output_stream_not_subject_to_block_limit`（POSIX）

## 5. 测试（stdlib unittest）

- [x] 5.1 启动参数用例组：`--workdir` 有效时三处工作目录一致且横幅带来源标注、未提供时行为不变且标注为取启动时目录、无效目录（不存在 / 不是目录 / 不可访问）拒绝启动且退出码非零；「不可访问」子情形仅 POSIX 以 `chmod 000` 构造并在用例注释写明原因（Windows 上该子情形列入 §6 待用户验收清单）；验证：用例通过 —— 证据：提交 `1547022`；`tests/test_workdir.py` 共 11 项（校验口径 4 项 + main 行为 7 项）。**实施细化**：该子情形还需「非 root」这一条件（root 绕过 DAC，`chmod 000` 目录照样可读），用例据平台与 euid 跳过并在 skip 文案写明；该路径另以非特权用户端到端复验通过
- [x] 5.2 入口转发用例：`run.py server --workdir <路径>` 与直调主体的 stdout 逐字节一致、退出码一致；验证：用例通过 —— 证据：`EntryServerArgTest` 2 项（转发一致性、无 `server` 时的未知子命令）
- [x] 5.3 留痕用例组：分节标题出现、download 同时显示原始与解析路径、认证通过后被 400 拒绝的请求参数完整、超限时出现省略标注、未认证请求仍为截断预览；验证：用例通过 —— 证据：`TraceFormatTest` 7 项（含 hello 响应逐字段、exec 输出流不受限）
- [x] 5.4 夹具与既有用例适配：`_support.TestServer` 的保存/还原元组加入 `WORK_DIR`；`tests/test_server.py` 中 `server.START_DIR` 引用改名；验证：无 `START_DIR` 残留且用例通过 —— 证据：提交 `1547022`；夹具另提供 `work_dir` 覆盖与留痕捕获器；`grep -rn START_DIR tests/` 零命中
- [x] 5.5 全量回归；验证：`python3 -m unittest discover -s tests` 全绿、退出码 0，用例总数不少于改动前 —— 证据：`Ran 59 tests in 29.4s · OK (skipped=2)`、退出码 0（改动前基线 39 项；2 项 skip 即 5.1 说明的「不可访问」子情形）

## 6. 两平台实跑验证（结论以真机为准）

- [x] 6.1 Linux 被控端实跑：以 `--workdir` 指定目录启动，从客户端侧验证 exec 缺省 cwd、exec 相对 cwd、download 相对路径均以该目录为基准；验证：命令与输出留证 —— 证据：以临时目录为 `--workdir` 起服务，客户端实跑 `exec "pwd"`（不带 `--cwd`）→ 输出该目录；`exec "…; pwd" --cwd .` → 同；`download hello.txt` → 取回内容逐字节一致；三类无效目录启动均退出码 2；留痕排版与 64KB 上下限见 §3 / §4 证据
- [ ] 6.2 Windows 被控端实跑：同上，另验证无效路径拒绝启动与留痕分节排版的实际呈现；验证：命令与输出留证 —— **未完成**：需被控机取到本次提交后重启服务，而 token 随重启轮换（安全设计），须由使用方在被控机控制台抄录新 token 后才能继续；步骤见 §6 待用户验收清单

### 待用户验收清单（需在被控机上人工操作）

- [ ] 6.3 在被控机上取到本次提交（`git pull --ff-only`）后重启服务：`python run.py server --workdir <项目目录>`；**预期**：横幅「工作目录」显示该目录并标注其来自启动参数，服务正常提供全部能力。**注意**：token 随重启轮换，需把控制台新 token 抄入客户端侧 token 文档后通道才恢复
- [ ] 6.4 在被控机以 `--workdir` 指向一个不可访问的目录启动（Windows 上可构造为无读取权限的目录）；**预期**：输出指明原因的错误并以非零码退出，端口不被监听
- [ ] 6.5 从客户端侧触发一次 exec 与一次 download，观察被控机控制台；**预期**：控制台按小节排版，参数与结果完整可读，download 同时显示原始 `path` 与绝对路径
- [ ] 6.6 在被控机另跑一次不带 `--workdir` 的启动（在项目目录下 `cd` 后启动）；**预期**：横幅标注「未指定 --workdir，取启动时目录」，行为与旧版一致

## 7. 文档与收尾

- [x] 7.1 `README.md`：补启动参数说明（`--workdir` 用法、必须在 `server` 之后、`run.py --workdir X` 不成立）与留痕形态说明；验证：照 README 步骤可复现，且守卫 grep（机器名 / IP / 部署路径）零命中 —— 证据：提交 `f2b67ca`；守卫 grep 全仓零命中（除 LICENSE 作者署名这一既有豁免项）
- [x] 7.2 分提交推送（实现、测试、文档各自成提交）；验证：`git log --oneline` 与远端一致、`git status` 干净 —— 证据：提交 `f40907d`（实现）/ `1547022`（测试）/ `f2b67ca`（文档）；**已推送**：`0e2b3d5..f2b67ca  main -> main`，`git status -sb` 显示与 `origin/main` 同步
- [x] 7.3 证据登记：勾选各项并附证据（提交哈希 / 命令 / 退出码 / 产物名）；未在目标平台实跑的不勾选 —— 证据：本次更新即登记；§6.2 与 §6.3–6.6 因需被控机操作与 token 轮换而保持未勾选

## 8. 跟进项（本变更不实现，记录于此）

- [ ] 8.1 其他启动参数需求（如 `--port` / `--bind`）——本变更只做 `--workdir`，需要时另立变更
- [ ] 8.2 运行期默认工作目录失效（可移动介质拔出、网络盘断开）——启动期校验无法覆盖，如需处理另立变更
- [ ] 8.3 留痕是否同时写入文件（当前仅控制台）——涉及凭据与内容落盘的安全权衡，如需处理另立变更
