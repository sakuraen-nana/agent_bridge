## Why

局域网远程执行桥（agent-bridge：被控端 HTTP 服务 + agent 侧客户端）是通用工具——它不依赖任何具体项目的业务逻辑，却需要被部署到被控机上独立运行。使其成为独立仓库与独立交付物，工具就可以部署在被控项目目录**之外**：被控项目怎么切分支、切提交都不影响通道可用性，工具自身的文件也不会出现在被控项目的工作树里造成干扰；同时它可跨项目复用，充当类 ssh 的远程操作通道。

## What Changes

- 本仓库实现该工具的首版（此前无任何实现），采用 src 包布局：`run.py`（单一入口：自定位、可被 Python 2 解析、自带自举与解释器检测）+ `src/agent_bridge/{__init__.py, server.py, client.py, bootstrap.py}` + `tests/`
- **行为范围冻结**：server / client 的外部可观察行为以本仓库 `openspec/specs/agent-bridge/spec.md` 为准（token 生命周期与恒定时间认证、hello / exec / download 三类 API、请求留痕与控制台回显、超时与断开处理、NDJSON 流式输出、编码自适配）；本变更只实现规格所定义的行为，不夹带任何改进（改进另立变更）
- **自包含**：不依赖仓库之外的任何文件（含宿主项目可能提供的共享自举/工具模块）；工具目录整体拷到任意路径即可运行
- **Token 文档机制**：`bridge.local.md.example` 模板入库 + `client` 缺省从工具自身目录读取 `bridge.local.md`（多组 `token/host` 后者覆盖的解析语义保留），`.gitignore` 排除正式文件
- **测试**：`tests/` 下的标准库 unittest 套件（回环全链路、认证负路径、超时终止、客户端断开、长静默、输出编码、入口转发与 Py2 可解析性静态守卫）
- 文档与仓库约定：`README.md`（工具自身：启动、Windows 防火墙、token 填写、agent 使用流程、安全声明、控制台输出说明）、`AGENTS.md`（本仓库工作约定，当前为 0 字节空文件）、`openspec/config.yaml` 的 context（当前为空模板）、`.gitignore`（本仓库当前没有）、脚手架（`.agents/`、`.claude/`）纳入提交
- 运行时零第三方依赖：仅 Python 标准库，目标机任意 Python 3.7+ 可运行
- **仓库定位**：本仓库是独立、纯净的工具软件——机器清单、部署拓扑、凭据位置、与任何使用方项目的集成细节均不在本仓库记录（见 `AGENTS.md`「仓库定位」）

## Capabilities

### New Capabilities

- `agent-bridge`: 局域网远程执行桥的能力契约——服务启动与 token 生命周期、统一 token 认证、请求留痕与本地回显、hello / exec / download 三类 API、token 文档契约、client 扫描与调用、统一 Python 入口、自包含与独立部署

### Modified Capabilities

（无——本仓库此前没有任何能力规格）

## Impact

- 新增：本仓库的 `run.py`、`src/agent_bridge/`、`tests/`、`README.md`、`.gitignore`、`openspec/` 规格体系；填充现有空文件 `AGENTS.md` 与 `openspec/config.yaml`
- 修改：无对既有代码的修改（本仓库除脚手架外无内容）
- 外部依赖：无新增（仅标准库；目标机需已有 Python 3.7+）。分发通道即本仓库（GitHub）
- 完成判据含实跑验证：须在 Linux 与 Windows 两种被控环境各自完成一次真实运行验证（含从另一台机器经本工具发起远程操作），验证细节由使用方按其部署环境记录
- 风险声明：工具授予持 token 者以服务运行身份执行任意命令（等价 SSH）；token 文档 `bridge.local.md` 为敏感物，不入库、不进任何同步渠道
