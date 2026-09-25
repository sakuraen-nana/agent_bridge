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


_BANNER = """\
[run] 平台: Windows | Python: 3.12.7 | 依赖检测: 仅标准库，无需 venv
==================================================================
  agent-bridge 被控端服务器已启动（仅限可信局域网使用）
------------------------------------------------------------------
  Token    : pasted-token-value
  端口     : 37777（绑定 0.0.0.0）
  运行用户 : someone
  工作目录 : D:\\tools\\agent_bridge（未指定 --workdir，取启动时目录）
  局域网 IP: {ip_line}
  启动时刻 : 2026-09-26T01:41:02
  版本     : agent-bridge/0.1.0
------------------------------------------------------------------
  调用示例（在 agent / 开发机上执行；入口在工具根目录）:
    curl -X POST "http://example.test:37777/hello?token=pasted-token-value"
==================================================================
"""


def _banner(ip_line):
    return _BANNER.format(ip_line=ip_line)


class PastedBannerTest(unittest.TestCase):
    """整段粘贴启动横幅即可用（对照 spec「Token 文档契约」的横幅行形态）。"""

    def test_banner_yields_token_and_host(self):
        fields = client.parse_token_doc(_banner("192.168.7.21"))
        self.assertEqual(fields["token"], "pasted-token-value")
        self.assertEqual(fields["host"], "192.168.7.21")

    def test_private_address_preferred_over_cgnat(self):
        # VPN/CGNAT 段（100.64/10）不是 RFC1918，排在前面也不该被取
        fields = client.parse_token_doc(_banner("100.64.9.9, 192.168.7.21"))
        self.assertEqual(fields["host"], "192.168.7.21")

    def test_first_address_when_no_private(self):
        fields = client.parse_token_doc(_banner("100.64.9.9, 203.0.113.7"))
        self.assertEqual(fields["host"], "100.64.9.9")

    def test_line_without_address_is_ignored(self):
        # 横幅未检测到地址时会打印整句说明，照搬为 host 只会得到难归因的连接失败
        fields = client.parse_token_doc(_banner("（未检测到，请以 ipconfig / ip addr 输出为准）"))
        self.assertNotIn("host", fields)

    def test_manual_host_after_banner_wins(self):
        fields = client.parse_token_doc(_banner("192.168.7.21") + "\nhost: 10.1.2.3\n")
        self.assertEqual(fields["host"], "10.1.2.3")

    def test_call_example_line_does_not_yield_host(self):
        # 调用示例行不得成为 host 来源（规格明令）
        fields = client.parse_token_doc('    curl -X POST "http://10.9.9.9:37777/hello?token=t"\n')
        self.assertNotIn("host", fields)


class ValueSemanticsTest(unittest.TestCase):
    def test_empty_value_does_not_override(self):
        # 模板里留空的字段不得清掉粘贴进来的取值
        fields = client.parse_token_doc(_banner("192.168.7.21") + "\ntoken:\nhost:\nupdated:\n")
        self.assertEqual(fields["token"], "pasted-token-value")
        self.assertEqual(fields["host"], "192.168.7.21")

    def test_manual_host_accepts_hostname(self):
        # 手工 host 行不受"必须挑得出地址"约束，主机名照旧可用
        self.assertEqual(client.parse_token_doc("host: my-host.local\n")["host"], "my-host.local")


if __name__ == "__main__":
    unittest.main()
