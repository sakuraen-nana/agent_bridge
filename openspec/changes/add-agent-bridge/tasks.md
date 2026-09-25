# Tasks: add-agent-bridge

> 实施顺序按节顺序：脚手架（1）→ 包与入口实现（2）→ 测试（3）→ 客户端侧部署验证（4）→
> 被控机侧部署验证（5，需一台 Windows 被控机可达）→ 收尾（6）。
> 行为范围冻结见 design D5：只实现规格定义的行为，不做任何"顺手改进"，改进点记入 §8。
> 文档边界见 design D8：本仓库不记录使用方的机器与部署信息，下方需要指代环境处一律用通用角色（"客户端侧""被控机"）。

## 1. 仓库脚手架与约定落档

- [x] 1.1 新增 `.gitignore`（`bridge.local.md`、`__pycache__/`、`*.pyc`、`.venv/` 等）；验证：`git check-ignore -v bridge.local.md` 命中 `.gitignore:2`，按模板创建正式文件后 `git status` 不显示它 —— 证据：提交 `d4ede47`
- [x] 1.2 填写 `AGENTS.md`（design D8：仓库定位"独立通用工具、只维护通用事实"、OpenSpec 工作流、单一 Python 入口约定、提交约定（委派 git-commit skill）、文档写法"仓库自足"）；验证：文件非空且覆盖上述各点，**不含任何具体机器名、部署路径与使用方项目名称**（`grep -nEi "hermes|DESKTOP-TI3AMEE|se77-ws02|192\.168\.31\.|file_forest|File Forest|/data/|sakuraen" AGENTS.md README.md` 零命中；LICENSE 的版权署名为许可信息的作者项，不在守卫范围），克隆仓库的读者无需外部信息即可据此工作 —— 证据：提交 `d4ede47`，全仓 grep 零命中（除 LICENSE 作者项）
- [x] 1.3 填写 `openspec/config.yaml` 的 context（语言 Chinese / SHALL-MUST 保留英文、技术栈 Python 标准库、部署模型（通用描述：可独立部署于被控项目目录之外）、行为范围冻结）；验证：`openspec context --json` 正常，context 非空且不含具体机器名、使用方项目名称 —— 证据：提交 `27155ce`，`openspec validate add-agent-bridge --strict --no-interactive` 通过
- [x] 1.4 新增 `README.md`（工具自身文档：启动方法（两平台）、Windows 防火墙首次授权提示、token 填写流程、agent 使用流程、安全声明、控制台输出说明；使用模型用通用描述）；验证：按 README 步骤在任意工作目录跑通一次回环（启动服务 → 另一终端 scan/hello）—— 见 §4.2 实跑；`grep -nEi "hermes|DESKTOP-TI3AMEE|se77-ws02|192\.168\.31\.|file_forest|File Forest|/data/|sakuraen" README.md` 零命中 —— 证据：提交 `3cd1e89`
- [x] 1.5 首个提交：脚手架（`.agents/`、`.claude/`、`openspec/`、`AGENTS.md`、`.gitignore`、`README.md`）入库；验证：`git log` 有该提交且 `git status` 干净（除忽略项）—— 证据：提交 `d4ede47`（脚手架）、`27155ce`（变更制品）、`3cd1e89`（README）

## 2. 包与入口实现（按规格实现首版）

- [x] 2.1 新增 `src/agent_bridge/__init__.py` 与 `bootstrap.py`（design D2/D3：统一工具根定位——包目录向上两级、行缓冲设置、日志与失败辅助、控制台处理）；验证：包内其余模块与入口可 import，`python3 -c "from agent_bridge import bootstrap; print(bootstrap.TOOL_ROOT)"` 输出仓库根 —— 证据：提交 `9eaae90`，实测输出 `/data/se77-repo/agent_bridge`
- [x] 2.2 实现被控端 `server.py`（逐条对照 `spec.md` 的 requirement/scenario：固定端口 37777 与占用即失败、token 生成与内存驻留、恒定时间认证与统一 404、留痕排版、hello/exec/download 行为、超时与断开终止、长静默不误杀、编码自适配）；验证：`python3 run.py server` 启动输出齐全，本机 curl 三类 API 均符合 spec 场景 —— 证据：提交 `9eaae90`；启动横幅含 token/端口/用户/工作目录/IP/启动时刻；三类 API 由 §3 用例与 §4.2 实跑覆盖
- [x] 2.3 实现 agent 侧 `client.py`（四个子命令、退出码约定 0/1/2/3/4、token 文档多组后者覆盖解析、404 轮换提示文案）；验证：对 2.2 起的实例跑通 scan/hello/exec（成功与失败）/download 且退出码符合约定 —— 证据：提交 `9eaae90`；实测退出码 0（成功）、3（远端命令退出码透传）、4（错 token）、2（未知子命令）
- [x] 2.4 新增入口 `run.py`（design D2/D4：内联解释器版本检测且文件保持可被 Python 2 解析、自定位、`python -m agent_bridge.<主体>` 子进程分发与退出码透传、信息行只走 stderr）；验证：任意工作目录下经入口与直调模块的输出逐字节一致（`diff <(python3 run.py exec "echo hi") <(PYTHONPATH=src python3 -m agent_bridge.client exec "echo hi")`），`python2 run.py`（本机无 Py2，以静态检查替代，见 3.6）给出明确提示而非语法错误 —— 证据：提交 `9eaae90`；stdout 逐字节一致（实测 diff 无输出）
- [x] 2.5 新增 `bridge.local.md.example` 模板至仓库根（字段与说明含"不得进入任何同步渠道"警示）；验证：`git status` 追踪模板、不追踪正式文件（与 1.1 规则联动）—— 证据：提交 `9eaae90`；`git status` 显示模板被追踪、`bridge.local.md` 被忽略
- [x] 2.6 实现-规格逐条对照复核：逐条走查 `spec.md` 的每条 requirement 与 scenario，列出实现位置与验证方式；发现未覆盖即缺陷（先修实现或按流程改规格）；验证：复核清单产出并随提交入库 —— 证据：`openspec/changes/add-agent-bridge/implementation-review.md`（10/10 条需求覆盖，无未覆盖项；提交 `27155ce`）

## 3. 测试（stdlib unittest，design D6）

- [x] 3.1 `tests/` 骨架与夹具：进程内起服务实例（端口覆盖为临时端口）、临时目录 token 文档；token 文档解析用例（含多组同名 token/host 后者覆盖、注释与空行）；验证：`python3 -m unittest discover -s tests` 相应用例通过 —— 证据：提交 `aa7736d`
- [x] 3.2 认证与 hello 用例组：无 token / 错 token → 统一 404 且无业务响应；有效 token → hello 字段与运行环境一致；验证：用例通过 —— 证据：`AuthTest` 2 项、`HelloTest` 3 项
- [x] 3.3 exec 用例组：流式输出、非零退出码、超时（`timed_out:true`）、客户端断开不遗留子进程、长静默（>60s 阈值）不误杀、输出编码（UTF-8 与平台本地编码回退）；验证：用例通过（长静默用例以覆盖 socket 超时阈值实现，不实跑 90 秒）—— 证据：`ExecTest` 8 项（含断开用例以裸 socket 模拟，marker 文件不出现为证）
- [x] 3.4 download 用例组：文本与二进制文件逐字节一致（哈希比对）、Content-Length 正确、路径不存在与指向目录两类错误区分；验证：用例通过 —— 证据：`DownloadTest` 4 项
- [x] 3.5 client 用例组：对回环实例的 scan（已确认/未知服务分组）、hello、exec 退出码透传、download 默认落点与 `--out`；验证：用例通过 —— 证据：`ScanTest` 2 项、`ScanNoServerTest` 1 项、`ExecTest`/`HelloTest`/`DownloadTest` 相关项
- [x] 3.6 入口用例组：经 `run.py` 与直调模块的 stdout 一致与退出码透传、任意工作目录调用、未知子命令提示、解释器版本不足报错；入口文件的"Python 2 可解析"静态扫描守卫（design D7，局限记入用例注释）；验证：用例通过 —— 证据：`EntryStaticTest` 3 项、`EntryPy2ParseGuardTest` 3 项、`EntryRuntimeTest` 3 项
- [x] 3.7 测试一站式跑通并在 README 记明命令；验证：`python3 -m unittest discover -s tests -v` 全绿，退出码 0 —— 证据：`Ran 39 tests in 23.7s · OK`（提交 `aa7736d`；帮助文案微调后复跑仍 39/39）

## 4. 客户端侧部署验证（开发机，运行客户端的一侧）

- [x] 4.1 把本仓库克隆/拷入任意非项目目录，复制模板为 `bridge.local.md`；验证：`python3 run.py --help` 在任意工作目录可用，token 文档解析命中该目录 —— 证据：克隆到 `/tmp/agent-bridge-deploy`（与被操作目录无包含关系），`cd /` 下以绝对路径运行 `--help` 正常；token 文档位于该副本根并被正确解析
- [x] 4.2 回环全链路实跑：启动服务 → scan 定位 → hello → exec（成功/失败/超时各一条）→ download 哈希一致 → 无 token 请求 404；验证：命令与输出留作证据 —— 证据（全部经部署副本、cwd 任意）：scan 命中 127.0.0.1（工作目录显示为副本目录）；hello `agent-bridge/0.1.0`；exec `echo` 退出码 0、`exit 3` 退出码 3；download README 哈希与源文件一致（`ac87e58…`）；错 token 退出码 4。超时一项由 §3.3 用例覆盖（回环实跑略）

## 5. 被控机侧部署验证（一台 Windows 被控机；要求：部署于被控项目目录之外）

- [ ] 5.1 在被控机上把本仓库部署到被控项目目录之外的位置；验证：该目录不属于任何被控项目工作树（在被控项目内执行 `git status` 不含工具文件）；访问本仓库通道不通时走离线/局域网引导（design D9，记录实际所用通道）
- [ ] 5.2 在被控机启动 `python run.py server`，抄录本次 token 至客户端侧 token 文档（Windows 防火墙首次授权见 README）；验证：控制台输出 token/端口/工作目录等信息
- [ ] 5.3 从客户端侧经本工具完成一次真实跨机操作：`exec` 执行一条无害的真实命令，并 `download` 取回一个文件校验一致；验证：退出码 0、内容与预期一致

> §5 三项**未勾选**：当前无可达的 Windows 被控机（原测试机不可达）。代码与测试部分（§1–§4）已完成并留证；本变更停在"待被控机验证"状态，不得据此宣告完成。

## 6. 收尾

- [x] 6.1 分提交推送：实现、测试、文档各自成提交，推送至本仓库远端；验证：`git log --oneline` 与远端一致，`git status` 干净 —— 证据：提交 `d4ede47`/`27155ce`/`9eaae90`/`aa7736d`/`3cd1e89`/`780f8e7`/`2cc9604`/`e0c7aa3`/`ec64035`/`aaed110`；**已推送**：`dd8884a..aaed110  main -> main`（`git status -sb` 显示与 `origin/main` 同步）
- [x] 6.2 证据登记：更新本变更 tasks 勾选与证据（提交哈希 / 命令 / 退出码 / 产物名）；验证：勾选项均附证据，未实跑验证的不勾选 —— 证据：本次更新即登记

> **远端通路为间歇性——以实际推送结果为准（2026-09-25 更正）**：当日先后两次 `git ls-remote`（SSH 与 HTTPS，各 25–30s）超时、TCP 直连 `github.com:443` 可建立但 TLS 后的 git 操作超时，据此曾记为"推送未执行"（提交 `e0c7aa3`）；随后重试 `git push origin main` **一次成功**（`dd8884a..aaed110`），再次 `ls-remote` 又超时。结论：本仓库远端（GitHub）**可达性不稳定**，提交与推送应"尽试尽推"，失败时重试或改期，**不得因单次失败判定通路不可用**；确实推不出去时，按 design D9 经离线/局域网交付。§5 被控机侧验证同样需要在被控机取得一份副本，本就依赖一条可用通道。

## 7. 待用户验收清单（需在被控机上的人工操作）

- [ ] 7.1 在被控机上取得本仓库（常规克隆，或经离线/局域网引导接收），放到被控项目目录之外；**预期**：目录存在且含 `run.py` 与 `src/`
- [ ] 7.2 在被控机启动 `python run.py server`；**预期**：控制台出现 token / 端口 37777 / 运行用户 / 工作目录；若 Windows 防火墙弹窗请选择"允许（专用网络）"；随后把 token（与被控机 IP）填入客户端侧 token 文档
- [ ] 7.3（可选）确认被控机访问本仓库的通道是否可用（`git ls-remote <本仓库地址>`）；**预期**：可用则 §5.1 直接克隆，不可用则与 agent 约定离线/局域网引导通道

## 8. 跟进项（本变更不实现，记录于此）

- [ ] 8.1 实现中发现的任何行为改进点（鉴权升级、HTTPS、命令白名单、并发策略、工效增强等）——一律另立变更，不在首版实现中夹带
- [ ] 8.2 静态扫描守卫升级为 Python 2 真机校验（design D7 局限）——取决于是否有可用 Py2 环境
- [ ] 8.3 除首台被控机外是否还需其他机器部署（design D9 / Open Questions）——需要时同法克隆
- [ ] 8.4 `server.py` 子进程 stdout 管道未显式关闭，解释器退出前报 `ResourceWarning`（不影响行为与进程/端口清理；见 implementation-review「已知遗留」）——修复另立变更
- [ ] 8.5 入口为"父进程监督 + 子进程服务"结构：直接 `kill` 父进程不会停止服务（需按端口反查真实 PID 或用 Ctrl+C 使整个前台进程组退出）——属既有设计，若需改变（如父进程转发信号）另立变更
- [ ] 8.6 把"无使用方信息"守卫从 `AGENTS.md` / `README.md` 扩展到全仓文档层（尤其 `.claude/skills/` 下的技能文件：本变更期间从外部拷贝技能时就带入过使用方的机器名示例，已修正）——新增技能或文档时应有可执行的检查手段，而非依赖人工复查
