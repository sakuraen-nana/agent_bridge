# -*- coding: utf-8 -*-
"""测试共用夹具（非测试模块，不被 discover 收集）。

- ``TestServer``：进程内起服务实例——把端口常量覆盖为临时端口（缺省 0=自动分配），
  仅绑回环，避免依赖产品的固定端口；用毕还原被覆盖的模块全局。
- ``ToolTree``：把工具目录整体拷入临时目录（run.py + src/ + 模板 + 伪造的
  token 文档），供入口级子进程测试使用——不触碰工作区，也不触碰真实凭据文件。
- HTTP 小工具：直接以 ``http.client`` 发请求，便于做认证负路径与流式解析。
"""

import contextlib
import http.client
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time

TESTS_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(TESTS_DIR)
SRC_DIR = os.path.join(REPO_ROOT, "src")
if SRC_DIR not in sys.path:
    sys.path.insert(0, SRC_DIR)

from agent_bridge import server as server_mod  # noqa: E402


class TestServer:
    """进程内服务实例（上下文管理器）。"""

    def __init__(self, token="test-token-123", handler_timeout=None, port=0, host="127.0.0.1",
                 work_dir=None):
        self.token = token
        self._saved = (server_mod.BRIDGE_PORT, server_mod.TOKEN, server_mod.BridgeHandler.timeout,
                       server_mod.WORK_DIR, server_mod.WORK_DIR_FROM_ARG)
        server_mod.BRIDGE_PORT = port
        server_mod.TOKEN = token
        if work_dir is not None:
            # 模拟 --workdir：默认工作目录由夹具指定，用例无需真的走启动参数
            server_mod.WORK_DIR = work_dir
            server_mod.WORK_DIR_FROM_ARG = True
        if handler_timeout is not None:
            # 覆盖连接层 socket 超时阈值：长静默行为无需实跑 60 秒即可验证
            server_mod.BridgeHandler.timeout = handler_timeout
        self.httpd = server_mod.ThreadingHTTPServer((host, port), server_mod.BridgeHandler)
        self.httpd.daemon_threads = True
        self.host, self.port = self.httpd.server_address[0], self.httpd.server_address[1]
        self._thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self._thread.start()

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()
        (server_mod.BRIDGE_PORT, server_mod.TOKEN, server_mod.BridgeHandler.timeout,
         server_mod.WORK_DIR, server_mod.WORK_DIR_FROM_ARG) = self._saved

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
        return False


@contextlib.contextmanager
def capture_console():
    """捕获服务端控制台留痕（``_log`` 走 print，故重定向 stdout 即可）。"""
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        yield out


def wait_until(predicate, timeout=3.0, interval=0.02):
    """轮询直到 predicate() 为真（超时返回最后一次结果）。

    留痕由服务端线程写出，与响应完成之间**没有先后保证**（例如 hello 先发响应、
    再打印响应小节）——断言捕获到的留痕前须等其收尾行出现，否则读到的可能是
    还没写完的半截缓冲。
    """
    deadline = time.monotonic() + timeout
    while True:
        if predicate():
            return True
        if time.monotonic() >= deadline:
            return False
        time.sleep(interval)


def await_trace(out, blocks=1, timeout=3.0):
    """等到（并返回）捕获到的留痕写完 ``blocks`` 个请求块。"""
    wait_until(lambda: out.getvalue().count("└──") >= blocks, timeout=timeout)
    return out.getvalue()


def port_is_free(port, host="127.0.0.1"):
    import socket
    sock = socket.socket()
    sock.settimeout(0.5)
    try:
        return sock.connect_ex((host, port)) != 0
    finally:
        sock.close()


def request(port, method, path, token=None, body=None, host="127.0.0.1", timeout=60):
    """发一个请求，返回 ``(status, headers, payload_bytes)``。"""
    if token is not None:
        path = path + ("&" if "?" in path else "?") + "token=" + token
    conn = http.client.HTTPConnection(host, port, timeout=timeout)
    try:
        data = None
        headers = {}
        if body is not None:
            data = json.dumps(body).encode("utf-8")
            headers["Content-Type"] = "application/json"
        conn.request(method, path, body=data, headers=headers)
        resp = conn.getresponse()
        return resp.status, dict(resp.getheaders()), resp.read()
    finally:
        conn.close()


def exec_events(port, token, command, cwd=None, timeout_seconds=None, host="127.0.0.1", timeout=60):
    """执行一条 exec 请求，返回 ``(status, [事件...])``（NDJSON 逐行解析）。"""
    body = {"command": command}
    if cwd is not None:
        body["cwd"] = cwd
    if timeout_seconds is not None:
        body["timeout_seconds"] = timeout_seconds
    status, _, payload = request(port, "POST", "/exec", token, body, host=host, timeout=timeout)
    events = []
    for line in payload.decode("utf-8", "replace").splitlines():
        if line.strip():
            events.append(json.loads(line))
    return status, events


def output_text(events):
    """把 output 事件拼成完整输出文本。"""
    return "".join(e.get("data", "") for e in events if e.get("type") == "output")


def exit_event(events):
    """取 exit 事件（无则 None）。"""
    for event in reversed(events):
        if event.get("type") == "exit":
            return event
    return None


class ToolTree:
    """工具目录的临时副本（整目录搬迁场景，供入口级测试）。"""

    def __init__(self, token=None, host="127.0.0.1"):
        self.root = tempfile.mkdtemp(prefix="agent-bridge-tool-")
        shutil.copy2(os.path.join(REPO_ROOT, "run.py"), self.root)
        shutil.copytree(os.path.join(SRC_DIR, "agent_bridge"),
                        os.path.join(self.root, "src", "agent_bridge"))
        shutil.copy2(os.path.join(REPO_ROOT, "bridge.local.md.example"), self.root)
        if token is not None:
            with open(os.path.join(self.root, "bridge.local.md"), "w", encoding="utf-8") as fh:
                fh.write("token: {0}\nhost: {1}\nupdated: 2026-09-25\n".format(token, host))

    def run_entry(self, *args, cwd=None):
        """以子进程运行副本中的 run.py。"""
        return subprocess.run(
            [sys.executable, os.path.join(self.root, "run.py")] + list(args),
            cwd=cwd or self.root, capture_output=True, text=True, timeout=120)

    def run_module(self, module, *args, cwd=None):
        """以子进程直调包内模块（PYTHONPATH 指向副本的 src/）。"""
        env = dict(os.environ)
        env["PYTHONPATH"] = os.path.join(self.root, "src")
        return subprocess.run(
            [sys.executable, "-m", module] + list(args),
            cwd=cwd or self.root, env=env, capture_output=True, text=True, timeout=120)

    def cleanup(self):
        shutil.rmtree(self.root, ignore_errors=True)
