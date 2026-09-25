# -*- coding: utf-8 -*-
"""agent-bridge：局域网远程执行桥（被控端服务器 + agent 侧客户端）。

包内模块：
- ``server``：被控端（受 token 保护的 hello / exec / download 服务）；
- ``client``：agent 侧（scan / hello / exec / download 四个子命令）；
- ``bootstrap``：入口与主体共用的最小自举能力（工具根定位、控制台、子进程直通）。

对外的统一入口是仓库根的 ``run.py``；行为契约见 ``openspec/specs/agent-bridge/spec.md``。
"""
