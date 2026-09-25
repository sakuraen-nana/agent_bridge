# -*- coding: utf-8 -*-
"""统一入口 run.py 的行为契约。

- 静态组：用法、未知子命令、任意工作目录调用、入口文件的"可被 Python 2 解析"守卫；
- 运行时组：入口与直调模块的输出一致、退出码透传——需要真实固定端口 37777
  （客户端按产品端口连接，入口不接受端口覆盖），故带空闲探测，端口被占用即跳过。
- 全部用例都在**工具目录的临时副本**上运行（ToolTree），顺带覆盖"工具目录可整体
  搬迁"这一规格场景，且不触碰工作区与真实凭据文件。
"""

import ast
import os
import re
import sys
import tempfile
import tokenize
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _support as support  # noqa: E402

ENTRY_PATH = os.path.join(support.REPO_ROOT, "run.py")


def _entry_source():
    with open(ENTRY_PATH, "r", encoding="utf-8") as fh:
        return fh.read()


class EntryStaticTest(unittest.TestCase):
    """不需要服务端。"""

    def setUp(self):
        self.tree = support.ToolTree()
        self.addCleanup(self.tree.cleanup)

    def test_help_exits_zero(self):
        result = self.tree.run_entry("--help")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("run.py", result.stdout)

    def test_unknown_subcommand_exits_2_with_usage(self):
        result = self.tree.run_entry("frobnicate")
        self.assertEqual(result.returncode, 2)
        self.assertIn("未知子命令", result.stderr)
        self.assertIn("用法", result.stderr)

    def test_callable_from_any_working_directory(self):
        result = self.tree.run_entry("--help", cwd=tempfile.gettempdir())
        self.assertEqual(result.returncode, 0, result.stderr)


class EntryPy2ParseGuardTest(unittest.TestCase):
    """入口文件"可被 Python 2 解析"的静态近似守卫（design D7）。

    **局限**：本机不保证有 Python 2 解释器，无法真机解析校验。这里用 AST 节点
    检查（Python 3 专有语法）＋ tokenize 检查（数字下划线）作近似替代；新增入口
    代码时仍需人工留意该约定。
    """

    def test_no_python3_only_syntax(self):
        tree = ast.parse(_entry_source())
        offenders = []
        for node in ast.walk(tree):
            if isinstance(node, ast.JoinedStr):
                offenders.append((node.lineno, "f-string"))
            elif isinstance(node, ast.NamedExpr):
                offenders.append((node.lineno, "海象运算符 :="))
            elif isinstance(node, ast.Nonlocal):
                offenders.append((node.lineno, "nonlocal"))
            elif isinstance(node, (ast.AsyncFunctionDef, ast.Await, ast.AsyncFor, ast.AsyncWith)):
                offenders.append((node.lineno, "async/await"))
            elif isinstance(node, ast.YieldFrom):
                offenders.append((node.lineno, "yield from"))
            elif isinstance(node, ast.MatMult):
                offenders.append((node.lineno, "矩阵乘 @"))
            elif isinstance(node, ast.AnnAssign):
                offenders.append((node.lineno, "变量注解"))
            elif getattr(node, "returns", None) is not None:
                offenders.append((node.lineno, "函数返回注解 ->"))
            elif isinstance(node, (ast.FunctionDef, ast.Lambda)) and getattr(node.args, "kwonlyargs", None):
                offenders.append((node.lineno, "关键字-only 参数"))
        self.assertEqual(offenders, [], "入口文件出现 Python 3 专有语法：{0}".format(offenders))

    def test_no_numeric_underscores(self):
        with open(ENTRY_PATH, "rb") as fh:
            tokens = list(tokenize.tokenize(fh.readline))
        offenders = [(tok.start[0], tok.string) for tok in tokens
                     if tok.type == tokenize.NUMBER and "_" in tok.string]
        self.assertEqual(offenders, [], "入口文件出现带下划线的数字字面量（Python 3.6+）：{0}".format(offenders))

    def test_version_guard_and_future_import_present(self):
        source = _entry_source()
        self.assertIn("from __future__ import print_function", source)
        self.assertRegex(source, r"sys\.version_info\s*<\s*\(3,\s*7\)")
        # 版本检测必须在包导入之前：包内模块使用 Python 3 语法，Py2 下导入即抛
        # SyntaxError，那样就来不及打印版本提示了
        self.assertLess(source.index("sys.version_info"),
                        source.index("import agent_bridge.bootstrap"))


class EntryRuntimeTest(unittest.TestCase):
    """入口与直调模块等价、退出码透传（需真实固定端口，占用即跳过）。"""

    def setUp(self):
        if not support.port_is_free(37777):
            self.skipTest("固定端口 37777 被占用，跳过入口级运行时验证")
        self.srv = support.TestServer(port=37777)
        self.addCleanup(self.srv.close)
        self.tree = support.ToolTree(token=self.srv.token, host="127.0.0.1")
        self.addCleanup(self.tree.cleanup)

    def test_entry_matches_direct_module(self):
        cwd = tempfile.gettempdir()
        entry = self.tree.run_entry("exec", "echo 一致性检查", cwd=cwd)
        direct = self.tree.run_module("agent_bridge.client", "exec", "echo 一致性检查", cwd=cwd)
        self.assertEqual(entry.returncode, 0, entry.stderr)
        self.assertEqual(direct.returncode, 0, direct.stderr)
        self.assertEqual(entry.stdout, direct.stdout)   # stdout 逐字节一致
        self.assertIn("一致性检查", entry.stdout)

    def test_hello_via_entry(self):
        result = self.tree.run_entry("hello", cwd=tempfile.gettempdir())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('"version"', result.stdout)

    def test_exit_code_passthrough_via_entry(self):
        result = self.tree.run_entry("exec", "exit 5", cwd=tempfile.gettempdir())
        self.assertEqual(result.returncode, 5)


class EntryServerArgTest(unittest.TestCase):
    """被控端启动参数经入口原样转发。

    不需占用产品端口：无效目录在绑定端口**之前**就退出，因此这里只走静态与失败路径。
    """

    def setUp(self):
        self.tree = support.ToolTree()
        self.addCleanup(self.tree.cleanup)

    def test_workdir_forwarded_to_subject(self):
        bogus = os.path.join(tempfile.gettempdir(), "ab-no-such-dir-xyz")
        entry = self.tree.run_entry("server", "--workdir", bogus)
        direct = self.tree.run_module("agent_bridge.server", "--workdir", bogus)
        self.assertEqual(entry.returncode, 2, entry.stderr)
        self.assertEqual(direct.returncode, 2, direct.stderr)
        self.assertEqual(entry.stdout, direct.stdout)  # 入口不污染 stdout
        self.assertIn("路径不存在", entry.stderr)       # 参数确实到达了程序主体

    def test_workdir_without_server_subcommand_is_unknown(self):
        result = self.tree.run_entry("--workdir", tempfile.gettempdir())
        self.assertEqual(result.returncode, 2)
        self.assertIn("未知子命令", result.stderr)


if __name__ == "__main__":
    unittest.main()
