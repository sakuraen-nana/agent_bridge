# -*- coding: utf-8 -*-
"""被控端启动参数 --workdir 的行为契约（对照 spec「服务启动与 token 生命周期」）。

覆盖：目录校验口径（存在 + 是目录 + 可列出）、拒绝启动的退出码与错误信息、
相对路径以启动时 cwd 为基准、不传参数时行为不变、横幅标注工作目录来源。

main() 的用例在进程内运行：把 BRIDGE_PORT 覆盖为临时端口（0 = 自动分配）并让
serve_forever 空转返回——因此既不触碰产品的固定端口 37777，也不会真的对外服务。
"""

import contextlib
import io
import os
import shutil
import stat
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _support as support  # noqa: E402

from agent_bridge import server  # noqa: E402

# 「存在但不可访问」的目录需要 POSIX 且**非 root**：root 绕过 DAC，chmod 000 也读得进去。
# Windows 与该子情形另见 tasks.md 的待用户验收清单。
UNREADABLE_TESTABLE = (os.name == "posix" and hasattr(os, "geteuid") and os.geteuid() != 0)
UNREADABLE_REASON = "需 POSIX 且非 root（root 绕过权限检查，构造不出不可访问目录）"


@contextlib.contextmanager
def _run_main(argv):
    """进程内运行 main()，返回 (退出码, stdout, stderr)；期间覆盖的模块全局用后还原。"""
    saved = (server.BRIDGE_PORT, server.WORK_DIR, server.WORK_DIR_FROM_ARG, server.TOKEN)
    server.BRIDGE_PORT = 0  # 自动分配端口，不使用产品端口
    out, err = io.StringIO(), io.StringIO()
    try:
        with mock.patch.object(server.ThreadingHTTPServer, "serve_forever",
                               lambda self, *args, **kwargs: None):
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = 0
                try:
                    server.main(list(argv))
                except SystemExit as exc:
                    code = exc.code if exc.code is not None else 0
        yield code, out.getvalue(), err.getvalue()
    finally:
        (server.BRIDGE_PORT, server.WORK_DIR, server.WORK_DIR_FROM_ARG, server.TOKEN) = saved


def _banner_workdir_line(banner):
    return [line for line in banner.splitlines() if "工作目录" in line][0]


class CheckWorkdirTest(unittest.TestCase):
    """目录校验口径：存在 + 是目录 + 可列出（design D3）。"""

    def test_usable_dir_passes(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIsNone(server._check_workdir(d))

    def test_missing_path_rejected(self):
        self.assertEqual(server._check_workdir(os.path.join(tempfile.gettempdir(),
                                                            "ab-no-such-dir-xyz")), "路径不存在")

    def test_file_rejected(self):
        fd, path = tempfile.mkstemp(prefix="ab-file-")
        os.close(fd)
        self.addCleanup(os.unlink, path)
        self.assertEqual(server._check_workdir(path), "不是目录")

    @unittest.skipUnless(UNREADABLE_TESTABLE, UNREADABLE_REASON)
    def test_unreadable_dir_rejected(self):
        d = tempfile.mkdtemp(prefix="ab-noread-")
        self.addCleanup(shutil.rmtree, d, ignore_errors=True)
        os.chmod(d, 0)
        try:
            self.assertIn("不可访问", server._check_workdir(d))
        finally:
            os.chmod(d, stat.S_IRWXU)  # 先恢复权限，否则临时目录自身删不掉


class MainWorkdirTest(unittest.TestCase):
    """main() 对 --workdir 的处理：生效、拒绝启动、横幅来源标注。"""

    def test_valid_workdir_takes_effect_and_banner_annotated(self):
        with tempfile.TemporaryDirectory() as d:
            with _run_main(["--workdir", d]) as (code, out, err):
                self.assertEqual(code, 0, err)
                self.assertEqual(server.WORK_DIR, os.path.abspath(d))
                self.assertTrue(server.WORK_DIR_FROM_ARG)
                line = _banner_workdir_line(out)
                self.assertIn(os.path.abspath(d), line)
                self.assertIn("来自 --workdir", line)

    def test_relative_workdir_resolves_against_startup_cwd(self):
        with tempfile.TemporaryDirectory() as d:
            try:
                rel = os.path.relpath(d, os.getcwd())
            except ValueError:  # Windows 跨盘符无法计算相对路径
                self.skipTest("临时目录与当前目录不在同一盘符")
            with _run_main(["--workdir", rel]) as (code, out, err):
                self.assertEqual(code, 0, err)
                self.assertEqual(server.WORK_DIR, os.path.abspath(d))

    def test_missing_dir_refuses_to_start(self):
        with _run_main(["--workdir", os.path.join(tempfile.gettempdir(), "ab-nope-xyz")]) \
                as (code, out, err):
            self.assertEqual(code, 2)
            self.assertIn("路径不存在", err)
            self.assertNotIn("已启动", out)  # 未进入启动流程

    def test_file_path_refuses_to_start(self):
        fd, path = tempfile.mkstemp(prefix="ab-file-")
        os.close(fd)
        self.addCleanup(os.unlink, path)
        with _run_main(["--workdir", path]) as (code, out, err):
            self.assertEqual(code, 2)
            self.assertIn("不是目录", err)

    @unittest.skipUnless(UNREADABLE_TESTABLE, UNREADABLE_REASON)
    def test_unreadable_dir_refuses_to_start(self):
        d = tempfile.mkdtemp(prefix="ab-noread-")
        self.addCleanup(shutil.rmtree, d, ignore_errors=True)
        os.chmod(d, 0)
        try:
            with _run_main(["--workdir", d]) as (code, out, err):
                self.assertEqual(code, 2)
                self.assertIn("不可访问", err)
        finally:
            os.chmod(d, stat.S_IRWXU)

    def test_unknown_option_exits_2(self):
        with _run_main(["--nope"]) as (code, out, err):
            self.assertEqual(code, 2)
            self.assertIn("unrecognized arguments", err)

    def test_without_option_keeps_startup_dir(self):
        before = os.path.abspath(os.getcwd())
        with _run_main([]) as (code, out, err):
            self.assertEqual(code, 0, err)
            self.assertEqual(server.WORK_DIR, before)
            self.assertFalse(server.WORK_DIR_FROM_ARG)
            self.assertIn("未指定 --workdir", _banner_workdir_line(out))


if __name__ == "__main__":
    unittest.main()
