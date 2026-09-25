# -*- coding: utf-8 -*-
"""agent-bridge 自举模块：入口与程序主体共用的最小自举能力（唯一副本）。

只服务本工具，仅用标准库，且**不得依赖本仓库之外的任何文件**。提供：

- ``TOOL_ROOT``：工具根目录（含 ``run.py`` 与 ``bridge.local.md`` 的目录）的统一定位；
- ``soften_console``：控制台编码降级与行缓冲；
- ``fail``：带前缀的失败退出；``run_passthrough``：子进程直通执行（退出码透传）。
"""

import os
import subprocess
import sys

# 工具根：本模块位于 <工具根>/src/agent_bridge/，向上两级即工具根。
# 与调用时的工作目录无关——所有路径（token 文档等）一律以此为准。
TOOL_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def soften_console():
    """统一控制台行为（入口启动时调用一次）：

    - **编码降级**：``errors="replace"``——Windows 默认 cp936 控制台遇到不可编码
      字符会抛 UnicodeEncodeError 并中断脚本；
    - **行缓冲**：Python 的 stdout 在非 tty（管道 / 重定向）下是块缓冲，而入口
      派生的子进程直接写文件描述符——不设行缓冲会出现"入口自己的提示行挤到子进程
      输出之后"的错序。经远程通道（管道捕获）触发时，日志错序会直接毁掉远程诊断。
    """
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(errors="replace", line_buffering=True)
        except (AttributeError, ValueError):
            # 流被替换为非文本流，或已 detach —— 无需处理
            pass


def fail(message, label="run", code=2):
    """打印 ``[<label>] <message>`` 到 stderr 并以 ``code`` 退出。"""
    sys.stderr.write("[{0}] {1}\n".format(label, message))
    sys.exit(code)


def run_passthrough(argv, label="run", env=None):
    """执行子进程并以完全相同的退出码退出当前进程。

    中断语义：Ctrl+C 由终端发给**整个前台进程组**，子进程会收到同一个 SIGINT
    并自行收尾——故此处不急于退出，而是继续等待子进程，否则父进程先走会留下
    "提示符已回来、任务仍在跑"的假象。再按一次 Ctrl+C 则强制退出。
    """
    try:
        proc = subprocess.Popen(argv, env=env)
    except OSError as exc:
        fail("无法执行 {0}：{1}".format(argv[0], exc), label, 2)
    try:
        sys.exit(proc.wait())
    except KeyboardInterrupt:
        sys.stderr.write("[{0}] 已请求中断，等待子进程收尾"
                         "（再按一次 Ctrl+C 强制退出）\n".format(label))
        try:
            sys.exit(proc.wait())
        except KeyboardInterrupt:
            sys.exit(130)
