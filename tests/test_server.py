# -*- coding: utf-8 -*-
"""被控端行为契约（对照 spec）：统一认证、hello、exec、download 与生命期收尾。

用例在进程内以临时端口起服务实例，不依赖产品的固定端口 37777（唯一例外是入口级
测试 test_entry.py，那里需要真实端口且带空闲探测）。
涉及 POSIX shell 语义的用例（sleep/touch）在 Windows 上跳过。
"""

import hashlib
import json
import os
import socket
import sys
import tempfile
import time
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _support as support  # noqa: E402

from agent_bridge import server  # noqa: E402

POSIX_ONLY = unittest.skipIf(os.name == "nt", "该用例依赖 POSIX shell（sleep/touch）")


class AuthTest(unittest.TestCase):
    def test_missing_or_wrong_token_uniform_404(self):
        with support.TestServer() as srv:
            cases = (("/hello", {}),
                     ("/exec", {"command": "echo x"}),
                     ("/download", {"path": "run.py"}))
            for path, body in cases:
                for token in (None, "wrong-token"):
                    status, header, payload = support.request(srv.port, "POST", path, token, body)
                    self.assertEqual(status, 404, (path, token))
                    # 统一响应：不区分"token 错"与"路径不存在"，不泄露服务器信息
                    self.assertEqual(payload, b'{"error":"not found"}')
                    self.assertNotIn("Server", header)
                    self.assertNotIn("Date", header)

    def test_valid_token_accepted(self):
        with support.TestServer() as srv:
            status, _, _ = support.request(srv.port, "POST", "/hello", srv.token, {})
            self.assertEqual(status, 200)


class HelloTest(unittest.TestCase):
    def test_hello_fields_match_environment(self):
        with support.TestServer() as srv:
            status, _, payload = support.request(srv.port, "POST", "/hello", srv.token, {})
            self.assertEqual(status, 200)
            data = json.loads(payload)
            self.assertEqual(data["version"], server.BRIDGE_VERSION)
            self.assertEqual(data["cwd"], server.WORK_DIR)
            self.assertEqual(data["user"], server._current_user())
            for key in ("hostname", "system", "release", "platform", "lan_ips", "started_at"):
                self.assertIn(key, data)
            self.assertEqual(data["started_at"], server.STARTED_AT)


class ExecTest(unittest.TestCase):
    def test_stream_and_exit_code(self):
        with support.TestServer() as srv:
            status, events = support.exec_events(srv.port, srv.token, "printf 'line1\\nline2\\n'")
            self.assertEqual(status, 200)
            self.assertIn("line1", support.output_text(events))
            self.assertIn("line2", support.output_text(events))
            exit_ev = support.exit_event(events)
            self.assertEqual(exit_ev["code"], 0)
            self.assertNotIn("timed_out", exit_ev)
            self.assertIsInstance(exit_ev["duration_ms"], int)

    def test_nonzero_exit_code_passthrough(self):
        with support.TestServer() as srv:
            _, events = support.exec_events(srv.port, srv.token, "exit 7")
            self.assertEqual(support.exit_event(events)["code"], 7)

    @POSIX_ONLY
    def test_timeout_terminates_child(self):
        with support.TestServer() as srv:
            _, events = support.exec_events(srv.port, srv.token, "sleep 5", timeout_seconds=1)
            exit_ev = support.exit_event(events)
            self.assertTrue(exit_ev.get("timed_out"), "超时必须以 timed_out:true 收尾")
            self.assertNotEqual(exit_ev["code"], 0)

    @POSIX_ONLY
    def test_cwd_relative_to_server_start_dir(self):
        with support.TestServer() as srv:
            _, events = support.exec_events(srv.port, srv.token, "pwd", cwd=".")
            self.assertEqual(support.output_text(events).strip(), server.WORK_DIR)

    @POSIX_ONLY
    def test_long_silence_not_mistaken_for_disconnect(self):
        # 连接层 socket 超时阈值缩短到 2s；命令静默 4s 后仍有输出 → 必须完整执行
        with support.TestServer(handler_timeout=2) as srv:
            _, events = support.exec_events(srv.port, srv.token, "sleep 4 && echo 完成", timeout=30)
            self.assertIn("完成", support.output_text(events))
            exit_ev = support.exit_event(events)
            self.assertEqual(exit_ev["code"], 0)
            self.assertNotIn("timed_out", exit_ev)

    @POSIX_ONLY
    def test_client_disconnect_kills_child(self):
        # 用裸 socket 模拟"命令执行期间客户端断开"：发出请求后不读取响应、直接关闭
        # 连接（不能用 http.client.read()——它会等到首个 chunk 才返回，那时命令已跑完）
        with support.TestServer() as srv:
            marker = os.path.join(tempfile.mkdtemp(prefix="ab-disconnect-"), "marker")
            command = "sleep 3 && touch {0}".format(marker)
            body = json.dumps({"command": command}).encode("utf-8")
            head = (
                "POST /exec?token={token} HTTP/1.1\r\n"
                "Host: {host}\r\n"
                "Content-Type: application/json\r\n"
                "Content-Length: {length}\r\n"
                "Connection: close\r\n\r\n"
            ).format(token=srv.token, host=srv.host, length=len(body))
            sock = socket.create_connection((srv.host, srv.port), timeout=10)
            sock.sendall(head.encode("utf-8") + body)
            time.sleep(0.5)   # 让服务端把子进程起起来
            sock.close()      # 客户端断开
            time.sleep(4.5)   # 子进程若未被终止，3 秒后即已 touch 出 marker
            self.assertFalse(os.path.exists(marker), "客户端断开后子进程未被终止（有孤儿进程）")

    def test_child_output_encoding_utf8(self):
        # 子进程经 PYTHONIOENCODING=utf-8 注入，非 ASCII 输出不崩溃且正确回传
        with support.TestServer() as srv:
            command = '"{0}" -c "print(\'✓ 中文输出\')"'.format(sys.executable)
            _, events = support.exec_events(srv.port, srv.token, command)
            self.assertIn("✓ 中文输出", support.output_text(events))
            self.assertEqual(support.exit_event(events)["code"], 0)

    def test_missing_command_rejected(self):
        with support.TestServer() as srv:
            status, _, payload = support.request(srv.port, "POST", "/exec", srv.token, {})
            self.assertEqual(status, 400)
            self.assertIn("error", json.loads(payload))

    def test_bad_timeout_rejected(self):
        with support.TestServer() as srv:
            status, _, payload = support.request(
                srv.port, "POST", "/exec", srv.token, {"command": "echo x", "timeout_seconds": -1})
            self.assertEqual(status, 400)
            self.assertIn("error", json.loads(payload))


class DownloadTest(unittest.TestCase):
    def test_content_and_length_match(self):
        # 二进制逐字节一致（含非文本字节）+ Content-Length 正确；绝对路径入参
        payload_bytes = bytes(range(256)) * 64
        fd, target = tempfile.mkstemp(prefix="ab-dl-")
        with os.fdopen(fd, "wb") as fh:
            fh.write(payload_bytes)
        self.addCleanup(os.unlink, target)
        with support.TestServer() as srv:
            status, headers, payload = support.request(
                srv.port, "POST", "/download", srv.token, {"path": target})
            self.assertEqual(status, 200)
            self.assertEqual(int(headers["Content-Length"]), len(payload_bytes))
            self.assertEqual(hashlib.sha256(payload).hexdigest(),
                             hashlib.sha256(payload_bytes).hexdigest())

    def test_relative_path_resolves_against_work_dir(self):
        # 相对路径基于默认工作目录解释：在该目录下建临时文件，按文件名取回
        fd, target = tempfile.mkstemp(prefix="ab-rel-", dir=server.WORK_DIR)
        with os.fdopen(fd, "wb") as fh:
            fh.write(b"relative-path-content")
        self.addCleanup(os.unlink, target)
        with support.TestServer() as srv:
            status, _, payload = support.request(
                srv.port, "POST", "/download", srv.token,
                {"path": os.path.basename(target)})
            self.assertEqual(status, 200)
            self.assertEqual(payload, b"relative-path-content")

    def test_missing_path_reports_not_found(self):
        with support.TestServer() as srv:
            status, _, payload = support.request(
                srv.port, "POST", "/download", srv.token, {"path": "no/such/file.bin"})
            self.assertEqual(status, 404)
            self.assertEqual(json.loads(payload)["error"], "文件不存在")

    def test_directory_reports_distinct_error(self):
        target_dir = tempfile.mkdtemp(prefix="ab-dir-")
        self.addCleanup(lambda: os.rmdir(target_dir))
        with support.TestServer() as srv:
            status, _, payload = support.request(
                srv.port, "POST", "/download", srv.token, {"path": target_dir})
            self.assertEqual(status, 400)
            self.assertEqual(json.loads(payload)["error"], "路径是目录而非文件")


class TraceFormatTest(unittest.TestCase):
    """控制台留痕：分节排版、认证通过者参数与响应完整显示、未认证者仅截断预览。"""

    def test_sections_use_titles_and_aligned_fields(self):
        with support.TestServer() as srv, support.capture_console() as out:
            support.request(srv.port, "POST", "/exec", srv.token, {"command": "echo t"})
        text = out.getvalue()
        self.assertIn("▸ 请求参数", text)
        self.assertIn("▸ 实时输出", text)
        field_lines = [ln for ln in text.splitlines() if ln.startswith("│       ") and " = " in ln]
        self.assertGreaterEqual(len(field_lines), 3, field_lines)
        # 同一小节内字段对齐：等号落在同一列（中文键按显示宽度计）
        self.assertEqual(len({ln.index(" = ") for ln in field_lines}), 1,
                         "字段未对齐: {0}".format(field_lines))

    def test_hello_response_shown_field_by_field(self):
        with support.TestServer() as srv, support.capture_console() as out:
            support.request(srv.port, "POST", "/hello", srv.token, {})
        text = out.getvalue()
        self.assertIn("▸ 响应", text)
        self.assertIn(server.BRIDGE_VERSION, text)
        self.assertIn(server.WORK_DIR, text)  # 工作目录字段与默认工作目录一致

    def test_download_shows_raw_and_resolved_path(self):
        with support.TestServer() as srv, support.capture_console() as out:
            support.request(srv.port, "POST", "/download", srv.token, {"path": "README.md"})
        text = out.getvalue()
        self.assertRegex(text, r"path\s+= README\.md")                     # 调用方传入的原始值
        self.assertIn(os.path.join(server.WORK_DIR, "README.md"), text)    # 解析后的绝对路径
        with open(os.path.join(server.WORK_DIR, "README.md"), encoding="utf-8") as fh:
            self.assertNotIn(fh.readline().strip(), text)                  # 不显示文件内容

    def test_authenticated_rejection_shows_full_params(self):
        with support.TestServer() as srv, support.capture_console() as out:
            status, _, _ = support.request(srv.port, "POST", "/exec", srv.token,
                                           {"command": "echo should-not-run",
                                            "cwd": "/ab-no-such-dir"})
        self.assertEqual(status, 400)
        text = out.getvalue()
        self.assertIn("▸ 请求参数", text)
        self.assertIn("echo should-not-run", text)   # 被拒请求的参数同样完整
        self.assertIn("/ab-no-such-dir", text)
        self.assertNotIn("请求体预览", text)          # 认证通过者不用截断预览

    def test_unauthenticated_keeps_truncated_preview(self):
        with support.TestServer() as srv, support.capture_console() as out:
            support.request(srv.port, "POST", "/exec", "wrong-token", {"command": "echo secret"})
        text = out.getvalue()
        self.assertIn("请求体预览", text)
        self.assertIn("未解析未执行", text)
        self.assertNotIn("▸ 请求参数", text)          # 未认证者不进入完整参数显示

    def test_oversize_block_annotates_omitted_bytes(self):
        body = {"pad": "A" * (server._BLOCK_LIMIT_BYTES + 1000)}
        with support.TestServer() as srv, support.capture_console() as out:
            support.request(srv.port, "POST", "/unknown-endpoint", srv.token, body)
        text = out.getvalue()
        self.assertIn("已省略", text)
        self.assertIn("单段上限 64KB", text)

    @POSIX_ONLY
    def test_exec_output_stream_not_subject_to_block_limit(self):
        with support.TestServer() as srv, support.capture_console() as out:
            status, events = support.exec_events(srv.port, srv.token, "seq 1 20000")
        self.assertEqual(status, 200)
        text = out.getvalue()
        self.assertNotIn("已省略", text)      # 实时输出流不受单段上限
        self.assertIn("\n20000\n", text)      # 末行完整回显
        self.assertEqual(support.output_text(events).count("\n"), 20000)


if __name__ == "__main__":
    unittest.main()
