# -*- coding: utf-8 -*-
"""客户端行为契约（对照 spec）：四个子命令、退出码约定、token 文档缺省与覆盖。

服务端在进程内以临时端口运行，客户端模块的 BRIDGE_PORT / TOKEN_DOC_PATH 被
临时改写指向它；token 文档写在临时目录，不触碰工作区与真实凭据文件。
"""

import contextlib
import hashlib
import io
import os
import shutil
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _support as support  # noqa: E402

from agent_bridge import client  # noqa: E402


class ClientTestBase(unittest.TestCase):
    def setUp(self):
        self.srv = support.TestServer()
        self.addCleanup(self.srv.close)
        self.tmp = tempfile.mkdtemp(prefix="ab-client-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.doc = os.path.join(self.tmp, "bridge.local.md")
        for patcher in (mock.patch.object(client, "BRIDGE_PORT", self.srv.port),
                        mock.patch.object(client, "TOKEN_DOC_PATH", self.doc)):
            patcher.start()
            self.addCleanup(patcher.stop)

    def write_doc(self, token, host="127.0.0.1"):
        with open(self.doc, "w", encoding="utf-8") as fh:
            fh.write("token: {0}\nhost: {1}\n".format(token, host))

    def run_client(self, *args, cwd=None):
        """以给定参数调用客户端 main()，返回 (退出码, stdout, stderr)。"""
        out, err = io.StringIO(), io.StringIO()
        old_cwd = os.getcwd()
        if cwd:
            os.chdir(cwd)
        try:
            with mock.patch.object(sys, "argv", ["run.py"] + list(args)), \
                 contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                try:
                    client.main()
                    code = 0
                except SystemExit as exc:
                    code = exc.code if isinstance(exc.code, int) else 0
        finally:
            os.chdir(old_cwd)
        return code, out.getvalue(), err.getvalue()


class PastedBannerDocTest(ClientTestBase):
    """把启动横幅整段粘进 token 文档后，client 无需 --host 即可用（端到端）。"""

    def write_banner(self, ip_line, extra=""):
        banner = (
            "  agent-bridge 被控端服务器已启动（仅限可信局域网使用）\n"
            "------------------------------------------------------------------\n"
            "  Token    : {token}\n"
            "  端口     : 37777（绑定 0.0.0.0）\n"
            "  局域网 IP: {ip_line}\n"
            "  版本     : agent-bridge/0.1.0\n"
            "------------------------------------------------------------------\n"
        ).format(token=self.srv.token, ip_line=ip_line)
        with open(self.doc, "w", encoding="utf-8") as fh:
            fh.write(banner + extra)

    def test_pasted_banner_usable_without_host_arg(self):
        self.write_banner("127.0.0.1")
        code, out, err = self.run_client("hello")
        self.assertEqual(code, 0, err)
        self.assertIn('"version"', out)

    def test_trailing_host_line_overrides_banner(self):
        # 横幅列的是不可达地址，末尾另写一行 host: 覆盖后即可用
        self.write_banner("203.0.113.7", extra="host: 127.0.0.1\n")
        code, out, err = self.run_client("hello")
        self.assertEqual(code, 0, err)
        self.assertIn('"version"', out)


class HelloTest(ClientTestBase):
    def test_hello_ok(self):
        self.write_doc(self.srv.token)
        code, out, _ = self.run_client("hello")
        self.assertEqual(code, 0)
        self.assertIn('"version"', out)

    def test_stale_token_hint_and_exit_4(self):
        self.write_doc("wrong-token")
        code, _, err = self.run_client("hello")
        self.assertEqual(code, 4)
        self.assertIn("404", err)
        self.assertIn("bridge.local.md", err)

    def test_missing_token_doc_exit_2(self):
        code, _, err = self.run_client("hello")
        self.assertEqual(code, 2)
        self.assertIn("未找到 token 文档", err)


class ExecTest(ClientTestBase):
    def test_success_output(self):
        self.write_doc(self.srv.token)
        code, out, _ = self.run_client("exec", "echo 来自远端")
        self.assertEqual(code, 0)
        self.assertIn("来自远端", out)

    def test_exit_code_passthrough(self):
        self.write_doc(self.srv.token)
        code, _, _ = self.run_client("exec", "exit 7")
        self.assertEqual(code, 7)


class DownloadTest(ClientTestBase):
    def test_default_name_and_out_option(self):
        payload = b"download-payload-123"
        fd, target = tempfile.mkstemp(prefix="ab-remote-")
        with os.fdopen(fd, "wb") as fh:
            fh.write(payload)
        self.addCleanup(os.unlink, target)
        self.write_doc(self.srv.token)

        # 缺省保存为当前目录同名文件
        code, _, _ = self.run_client("download", target, cwd=self.tmp)
        self.assertEqual(code, 0)
        default_path = os.path.join(self.tmp, os.path.basename(target))
        with open(default_path, "rb") as fh:
            self.assertEqual(hashlib.sha256(fh.read()).hexdigest(),
                             hashlib.sha256(payload).hexdigest())

        # --out 指定路径
        out_path = os.path.join(self.tmp, "renamed.bin")
        code, _, _ = self.run_client("download", target, "--out", out_path)
        self.assertEqual(code, 0)
        with open(out_path, "rb") as fh:
            self.assertEqual(fh.read(), payload)


class ScanTest(ClientTestBase):
    """scan 用例（服务端在跑）。注意：scan 的目标列表**始终追加 127.0.0.1**
    （便于定位本机实例），故只要回环上有本实例，扫描就必然命中它。"""

    def test_scan_finds_confirmed_server(self):
        self.write_doc(self.srv.token)
        code, out, _ = self.run_client("scan", "--cidr", "127.0.0.1/32")
        self.assertEqual(code, 0)
        self.assertIn("已确认的 bridge 服务器", out)
        self.assertIn("127.0.0.1", out)

    def test_scan_unknown_service_grouped(self):
        # 端口开放但 token 不匹配 → 归入"未知服务"，整体视为未确认（退出码 1）
        self.write_doc("wrong-token", host="127.0.0.1")
        code, out, _ = self.run_client("scan", "--cidr", "127.0.0.1/32")
        self.assertEqual(code, 1)
        self.assertIn("未知服务", out)


class ScanNoServerTest(unittest.TestCase):
    """无服务器时的 scan：无任何已确认结果 → 业务失败（退出码 1）。"""

    def setUp(self):
        # 取一个空闲但**不监听**的端口：扫描端口全程无人应答
        import socket
        probe = socket.socket()
        probe.bind(("127.0.0.1", 0))
        free_port = probe.getsockname()[1]
        probe.close()
        self.tmp = tempfile.mkdtemp(prefix="ab-scan-")
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        doc = os.path.join(self.tmp, "bridge.local.md")
        with open(doc, "w", encoding="utf-8") as fh:
            fh.write("token: unused-token\nhost: 127.0.0.1\n")
        for patcher in (mock.patch.object(client, "BRIDGE_PORT", free_port),
                        mock.patch.object(client, "TOKEN_DOC_PATH", doc)):
            patcher.start()
            self.addCleanup(patcher.stop)

    def test_no_result_exits_1(self):
        out, err = io.StringIO(), io.StringIO()
        with mock.patch.object(sys, "argv", ["run.py", "scan", "--cidr", "127.0.0.1/32"]), \
             contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                client.main()
                code = 0
            except SystemExit as exc:
                code = exc.code if isinstance(exc.code, int) else 0
        self.assertEqual(code, 1)


if __name__ == "__main__":
    unittest.main()
