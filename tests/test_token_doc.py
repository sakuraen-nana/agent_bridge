# -*- coding: utf-8 -*-
"""token 文档解析（共享约定）：注释与空行忽略、多组同名字段后者覆盖。"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import _support  # noqa: F401,E402  （导入即把 src/ 加入 sys.path）

from agent_bridge import client  # noqa: E402


class ParseTokenDocTest(unittest.TestCase):
    def test_fields_and_whitespace(self):
        fields = client.parse_token_doc("token:   abc123  \nhost: 192.168.1.10\nupdated: 2026-09-25\n")
        self.assertEqual(fields["token"], "abc123")
        self.assertEqual(fields["host"], "192.168.1.10")

    def test_comments_and_blank_lines_ignored(self):
        fields = client.parse_token_doc("# 注释行\n\n   \ntoken: t1\n")
        self.assertEqual(fields, {"token": "t1"})

    def test_inline_hash_belongs_to_value(self):
        # 不支持行内注释：值中出现的 # 属于值本身
        fields = client.parse_token_doc("token: ab#cd\n")
        self.assertEqual(fields["token"], "ab#cd")

    def test_multi_group_last_wins(self):
        doc = ("token: first\nhost: 10.0.0.1\n"
               "# 第二台被控机\n"
               "token: second\nhost: 10.0.0.2\n")
        fields = client.parse_token_doc(doc)
        self.assertEqual(fields["token"], "second")
        self.assertEqual(fields["host"], "10.0.0.2")

    def test_keys_are_lowercased(self):
        fields = client.parse_token_doc("TOKEN: t2\nHost: 10.1.1.1\n")
        self.assertEqual(fields["token"], "t2")
        self.assertEqual(fields["host"], "10.1.1.1")


if __name__ == "__main__":
    unittest.main()
