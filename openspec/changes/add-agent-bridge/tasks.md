# Tasks: add-agent-bridge

> 实施顺序按节顺序：脚手架（1）→ 包与入口实现（2）→ 测试（3）→ 客户端侧部署验证（4）→
> 被控机侧部署验证（5，需一台 Windows 被控机可达）→ 收尾（6）。
> 行为范围冻结见 design D5：只实现规格定义的行为，不做任何"顺手改进"，改进点记入 §8。
> 文档边界见 design D8：本仓库不记录使用方的机器与部署信息，下方需要指代环境处一律用通用角色（"客户端侧""被控机"）。

## 1. 仓库脚手架与约定落档

- [ ] 1.1 新增 `.gitignore`（`bridge.local.md`、`__pycache__/`、`*.pyc`、`.venv/` 等）；验证：按模板创建 `bridge.local.md` 后 `git status` 不显示它，`git check-ignore -v bridge.local.md` 命中规则
- [ ] 1.2 填写 `AGENTS.md`（design D8：仓库定位"独立通用工具、只维护通用事实"、OpenSpec 工作流、单一 Python 入口约定、提交约定（委派 git-commit skill）、文档写法"仓库自足"）；验证：文件非空且覆盖上述各点，**不含任何具体机器名、部署路径与使用方项目名称**（`grep -nEi "hermes|DESKTOP-TI3AMEE|se77-ws02|192\.168\.|file_forest|File Forest|/data/" AGENTS.md` 零命中），克隆仓库的读者无需外部信息即可据此工作
- [ ] 1.3 填写 `openspec/config.yaml` 的 context（语言 Chinese / SHALL-MUST 保留英文、技术栈 Python 标准库、部署模型（通用描述：可独立部署于被控项目目录之外）、行为范围冻结）；验证：`openspec context --json` 正常，context 非空且不含具体机器名、使用方项目名称
- [ ] 1.4 新增 `README.md`（工具自身文档：启动方法（两平台）、Windows 防火墙首次授权提示、token 填写流程、agent 使用流程、安全声明、控制台输出说明；使用模型用通用描述）；验证：按 README 步骤在任意工作目录跑通一次回环（启动服务 → 另一终端 scan/hello），且 `grep -nEi "hermes|DESKTOP-TI3AMEE|se77-ws02|192\.168\.|file_forest|File Forest|/data/" README.md` 零命中
- [ ] 1.5 首个提交：脚手架（`.agents/`、`.claude/`、`openspec/`、`AGENTS.md`、`.gitignore`、`README.md`）入库；验证：`git log` 有该提交且 `git status` 干净（除忽略项）

## 2. 包与入口实现（按规格实现首版）

- [ ] 2.1 新增 `src/agent_bridge/__init__.py` 与 `bootstrap.py`（design D2/D3：统一工具根定位——包目录向上两级、行缓冲设置、日志与失败辅助、控制台处理）；验证：包内其余模块与入口可 import，`python3 -c "from agent_bridge import bootstrap; print(bootstrap.TOOL_ROOT)"` 输出仓库根
- [ ] 2.2 实现被控端 `server.py`（逐条对照 `spec.md` 的 requirement/scenario：固定端口 37777 与占用即失败、token 生成与内存驻留、恒定时间认证与统一 404、留痕排版、hello/exec/download 行为、超时与断开终止、长静默不误杀、编码自适配）；验证：`python3 run.py server` 启动输出齐全，本机 curl 三类 API 均符合 spec 场景
- [ ] 2.3 实现 agent 侧 `client.py`（四个子命令、退出码约定 0/1/2/3/4、token 文档多组后者覆盖解析、404 轮换提示文案）；验证：对 2.2 起的实例跑通 scan/hello/exec（成功与失败）/download 且退出码符合约定
- [ ] 2.4 新增入口 `run.py`（design D2/D4：内联解释器版本检测且文件保持可被 Python 2 解析、自定位、`python -m agent_bridge.<主体>` 子进程分发与退出码透传、信息行只走 stderr）；验证：任意工作目录下经入口与直调模块的输出逐字节一致（`diff <(python3 run.py exec "echo hi") <(PYTHONPATH=src python3 -m agent_bridge.client exec "echo hi")`），`python2 run.py`（若无 py2 则以静态检查替代）给出明确提示而非语法错误
- [ ] 2.5 新增 `bridge.local.md.example` 模板至仓库根（字段与说明含"不得进入任何同步渠道"警示）；验证：`git status` 追踪模板、不追踪正式文件（与 1.1 规则联动）
- [ ] 2.6 实现-规格逐条对照复核：逐条走查 `spec.md` 的每条 requirement 与 scenario，列出实现位置与验证方式；发现未覆盖即缺陷（先修实现或按流程改规格）；验证：复核清单产出并随提交入库（可置于变更目录或文档附录）

## 3. 测试（stdlib unittest，design D6）

- [ ] 3.1 `tests/` 骨架与夹具：进程内起服务实例（端口覆盖为临时端口）、临时目录 token 文档；token 文档解析用例（含多组同名 token/host 后者覆盖、注释与空行）；验证：`python3 -m unittest discover -s tests` 相应用例通过
- [ ] 3.2 认证与 hello 用例组：无 token / 错 token → 统一 404 且无业务响应；有效 token → hello 字段与运行环境一致；验证：用例通过
- [ ] 3.3 exec 用例组：流式输出、非零退出码、超时（`timed_out:true`）、客户端断开不遗留子进程、长静默（>60s 阈值）不误杀、输出编码（UTF-8 与平台本地编码回退）；验证：用例通过（长静默用例可缩短阈值参数实现，不实跑 90 秒）
- [ ] 3.4 download 用例组：文本与二进制文件逐字节一致（哈希比对）、Content-Length 正确、路径不存在与指向目录两类错误区分；验证：用例通过
- [ ] 3.5 client 用例组：对回环实例的 scan（已确认/未知服务分组）、hello、exec 退出码透传、download 默认落点与 `--out`；验证：用例通过
- [ ] 3.6 入口用例组：经 `run.py` 与直调模块的 stdout 一致与退出码透传、任意工作目录调用、未知子命令提示、解释器版本不足报错；入口文件的"Python 2 可解析"静态扫描守卫（design D7，局限记入用例注释）；验证：用例通过
- [ ] 3.7 测试一站式跑通并在 README 记明命令；验证：`python3 -m unittest discover -s tests -v` 全绿，退出码 0

## 4. 客户端侧部署验证（开发机，运行客户端的一侧）

- [ ] 4.1 把本仓库克隆/拷入任意非项目目录，复制模板为 `bridge.local.md`；验证：`python3 run.py --help` 在任意工作目录可用，token 文档解析命中该目录
- [ ] 4.2 回环全链路实跑：启动服务 → scan 定位 → hello → exec（成功/失败/超时各一条）→ download 哈希一致 → 无 token 请求 404；验证：命令与输出留作证据（提交信息或变更记录）

## 5. 被控机侧部署验证（一台 Windows 被控机；要求：部署于被控项目目录之外）

- [ ] 5.1 在被控机上把本仓库部署到被控项目目录之外的位置；验证：该目录不属于任何被控项目工作树（在被控项目内执行 `git status` 不含工具文件）；访问本仓库通道不通时走离线/局域网引导（design D9，记录实际所用通道）
- [ ] 5.2 在被控机启动 `python run.py server`，抄录本次 token 至客户端侧 token 文档（Windows 防火墙首次授权见 README）；验证：控制台输出 token/端口/工作目录等信息
- [ ] 5.3 从客户端侧经本工具完成一次真实跨机操作：`exec` 执行一条无害的真实命令，并 `download` 取回一个文件校验一致；验证：退出码 0、内容与预期一致

## 6. 收尾

- [ ] 6.1 分提交推送：实现、测试、文档各自成提交，推送至本仓库远端；验证：`git log --oneline` 与远端一致，`git status` 干净
- [ ] 6.2 证据登记：更新本变更 tasks 勾选与证据（提交哈希 / 命令 / 退出码 / 产物名）；验证：勾选项均附证据，未实跑验证的不勾选

## 7. 待用户验收清单（需在被控机上的人工操作）

- [ ] 7.1 在被控机上取得本仓库（常规克隆，或经离线/局域网引导接收），放到被控项目目录之外；**预期**：目录存在且含 `run.py` 与 `src/`
- [ ] 7.2 在被控机启动 `python run.py server`；**预期**：控制台出现 token / 端口 37777 / 运行用户 / 工作目录；若 Windows 防火墙弹窗请选择"允许（专用网络）"；随后把 token（与被控机 IP）填入客户端侧 token 文档
- [ ] 7.3（可选）确认被控机访问本仓库的通道是否可用（`git ls-remote <本仓库地址>`）；**预期**：可用则 §5.1 直接克隆，不可用则与 agent 约定离线/局域网引导通道

## 8. 跟进项（本变更不实现，记录于此）

- [ ] 8.1 实现中发现的任何行为改进点（鉴权升级、HTTPS、命令白名单、并发策略、工效增强等）——一律另立变更，不在首版实现中夹带
- [ ] 8.2 静态扫描守卫升级为 Python 2 真机校验（design D7 局限）——取决于是否有可用 Py2 环境
- [ ] 8.3 除首台被控机外是否还需其他机器部署（design D9 / Open Questions）——需要时同法克隆
