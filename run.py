#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""run.py — agent-bridge 统一入口（跨平台）。

本工具独立、自包含：只要求目标机有任意可用的 Python 3（>=3.7），零第三方依赖。
入口只做四件事：

  1. 解释器检测：版本须 >=3.7；本文件刻意不使用 f-string 等 Python 2 无法解析的
     语法，误用 Python 2 启动时输出明确提示而非语法错误；
  2. 依赖环境检测：程序主体（src/agent_bridge/）仅用标准库、零第三方依赖，
     不需要 venv、pip 或联网步骤；
  3. 识别操作系统；
  4. 分发给程序主体（经 ``python -m`` 子进程，退出码透传）：
       python run.py                              # 被控端：启动服务器
       python run.py server [参数...]             # 同上
       python run.py scan|hello|exec|download ... # agent 侧：客户端子命令
       python run.py client <子命令> ...           # 同上（显式形式）

入口自定位（依据自身文件位置），可在任意工作目录下调用。
"""

from __future__ import print_function

import os
import platform
import sys

# 1) 解释器检测。**必须内联在模块顶层**：本文件可被 Python 2 解析，故此检查在
#    Python 2 下同样可达；而 src/agent_bridge/ 下的模块使用 Python 3 语法，只能
#    在版本检测通过之后再导入。
if sys.version_info < (3, 7):
    sys.stderr.write(
        "[run] 需要 Python 3.7 及以上，当前为 {0}。"
        "请改用 python3（Windows 可用 py -3）运行。\n".format(sys.version.split()[0]))
    sys.exit(2)

ENTRY_DIR = os.path.dirname(os.path.abspath(__file__))
SRC_DIR = os.path.join(ENTRY_DIR, "src")
sys.path.insert(0, SRC_DIR)

import agent_bridge.bootstrap as bootstrap  # noqa: E402  （须在版本检测之后导入，见上）

LABEL = "run"
CLIENT_SUBCOMMANDS = ("scan", "hello", "exec", "download")

USAGE = """\
用法（入口位于工具根目录，可在任意工作目录下调用）:
  python run.py                              # 被控端：启动服务器
  python run.py server                       # 同上
  python run.py scan|hello|exec|download ... # agent 侧：客户端子命令
  python run.py client <子命令> ...           # 同上（显式形式）
"""


def main(argv):
    bootstrap.soften_console()

    if not os.path.isdir(os.path.join(SRC_DIR, "agent_bridge")):
        bootstrap.fail("未找到程序主体：{0} 不存在（期望含 agent_bridge 包）；"
                       "请确认工具目录完整检出。".format(SRC_DIR), LABEL, 2)

    args = list(argv[1:])
    if args and args[0] in ("-h", "--help", "help"):
        print(USAGE, end="")
        return 0
    if not args or args[0] == "server":
        module, rest = "agent_bridge.server", args[1:]
    elif args[0] == "client":
        module, rest = "agent_bridge.client", args[1:]
    elif args[0] in CLIENT_SUBCOMMANDS:
        module, rest = "agent_bridge.client", args
    else:
        print(USAGE, end="", file=sys.stderr)
        bootstrap.fail("未知子命令: {0}".format(args[0]), LABEL, 2)

    # 2) 依赖环境检测：主体仅用标准库，无需 venv。信息行走 stderr：
    #    stdout 必须与直调程序主体逐字节一致（管道/机器可读场景不被污染）。
    sys.stderr.write("[{0}] 平台: {1} | Python: {2} | 依赖检测: 仅标准库，无需 venv\n".format(
        LABEL, platform.system(), sys.version.split()[0]))

    # 3) 识别 OS 与主体启动（-m 需要 src/ 在 PYTHONPATH 中）
    env = dict(os.environ)
    env["PYTHONPATH"] = SRC_DIR + os.pathsep + env.get("PYTHONPATH", "")
    bootstrap.run_passthrough([sys.executable, "-m", module] + rest, LABEL, env=env)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
