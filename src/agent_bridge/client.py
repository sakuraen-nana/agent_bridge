#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""agent-bridge 客户端（agent 侧）。

四个子命令：scan / hello / exec / download。token 与 host 缺省从工具根目录
（含 run.py 的目录）的 bridge.local.md 读取（模板见同目录 bridge.local.md.example），
命令行参数可覆盖；与调用时的工作目录无关。仅 Python 标准库。

退出码约定：
  0   成功（exec 时 = 远端命令退出码；scan 时 = 至少发现一台已确认服务器）
  1   业务失败（download 路径错误 / 下载不完整；scan 未发现已确认服务器）
  2   本地配置或用法错误（token 文档缺失、token 未填写等）
  3   网络连接失败或流式响应中断
  4   token 被服务器拒绝（404）——按标准处置提示更新 token 文档后重试
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import http.client
import ipaddress
import json
import os
import socket
import sys
import time
import urllib.parse

BRIDGE_PORT = 37777
SCAN_TIMEOUT = 0.3      # scan 的 TCP connect 超时
SCAN_WORKERS = 100      # scan 并发线程数
HTTP_TIMEOUT = 10       # 常规请求的连接/读取超时
DOWNLOAD_CHUNK_SIZE = 1024 * 1024

from .bootstrap import TOOL_ROOT

TOKEN_DOC_PATH = os.path.join(TOOL_ROOT, "bridge.local.md")
TEMPLATE_NAME = "bridge.local.md.example"

# ---- 共享约定（3.2）：token 失效标准处置提示 --------------------------------
# hello / exec / download 收到统一 404 拒绝时输出的固定文案。
TOKEN_STALE_HINT = (
    "[错误] 服务器返回 404：token 无效或已失效。\n"
    "agent-bridge 服务器每次重启都会生成新 token，旧 token 立即失效。\n"
    "请在被控机控制台抄录最新 token，更新工具根目录下的 bridge.local.md\n"
    "的 token 字段后重试（host 字段可一并核对）。"
)


# ---- token 文档解析（共享约定，3.2）----------------------------------------
# 启动横幅把主机写作「局域网 IP: …」，故该行需按其键名别名映射到 host，
# 使"整段粘贴控制台输出"直接可用。
_KEY_ALIASES = {"局域网 ip": "host"}


def _pick_banner_host(value: str) -> str:
    """从横幅的「局域网 IP」行取值：优先 RFC1918 私网地址，否则取第一个合法地址。

    横幅在多网卡机器上会列出多个逗号分隔的地址（可能含 VPN 地址），故需挑选。
    私网判别刻意手写区间而**不用** ``ipaddress.is_private``：后者的判定集合随
    Python 版本演进（且把 127/8、169.254/16、198.18/15 等非 LAN 地址也算作私网），
    而本工具承诺目标机任意 Python 3.7+ 行为一致。

    返回空串表示该行不含任何可解析地址（横幅未检测到地址时会打印整句说明，
    照搬为 host 只会得到难以归因的连接失败）。
    """
    candidates = []
    for part in value.split(","):
        part = part.strip()
        try:
            if isinstance(ipaddress.ip_address(part), ipaddress.IPv4Address):
                candidates.append(part)
        except ValueError:
            continue
    if not candidates:
        return ""
    for addr in candidates:
        first, second = int(addr.split(".")[0]), int(addr.split(".")[1])
        if first == 10 or (first == 172 and 16 <= second <= 31) or (first == 192 and second == 168):
            return addr
    return candidates[0]


def parse_token_doc(text: str) -> dict:
    """解析 token 文档内容，返回 {key: value}（键小写）。

    约定：识别 `key: value` 行，并识别**被控端启动横幅的行形态**（整段粘贴即可用）——
    横幅的 Token 行映射为 token，「局域网 IP」行映射为 host（多地址时私网优先）。
    以 # 开头的整行注释与空行忽略；键与值两侧空白不计；不支持行内注释（值中出现的
    # 属于值本身）；**值为空的行不覆盖已有值**（否则模板里待填的空字段会清掉粘贴进来
    的取值）；同名键按行序后者覆盖前者。
    """
    result = {}
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or ":" not in stripped:
            continue
        raw_key, _, value = stripped.partition(":")
        raw_key = raw_key.strip().lower()
        key = _KEY_ALIASES.get(raw_key, raw_key)
        value = value.strip()
        if not value:  # 空值不覆盖：模板中的待填字段与粘贴内容可安全并存
            continue
        if key == "host" and raw_key != "host":
            # 来自横幅的「局域网 IP」行：必须挑得出地址，否则该行无效
            value = _pick_banner_host(value)
            if not value:
                continue
        result[key] = value
    return result


def load_token_doc() -> dict:
    """读取 token 文档；缺失或未填 token 时输出清晰指引并退出（码 2）。"""
    if not os.path.isfile(TOKEN_DOC_PATH):
        print(
            f"[错误] 未找到 token 文档: {TOKEN_DOC_PATH}\n"
            f"请复制工具根目录下的 {TEMPLATE_NAME} 为 bridge.local.md，\n"
            f"并把被控机控制台输出的 token 填入 token: 字段（host 建议一并填写）。",
            file=sys.stderr,
        )
        sys.exit(2)
    with open(TOKEN_DOC_PATH, "r", encoding="utf-8") as fh:
        fields = parse_token_doc(fh.read())
    if not fields.get("token"):
        print(
            f"[错误] token 文档 {TOKEN_DOC_PATH} 中未填写 token 字段。\n"
            f"请在被控机控制台抄录最新 token，填入 token: 字段。",
            file=sys.stderr,
        )
        sys.exit(2)
    return fields


def resolve_target(args) -> tuple:
    """返回 (host, token)：命令行参数覆盖文档缺省值。"""
    fields = load_token_doc()
    host = getattr(args, "host", None) or fields.get("host", "")
    token = getattr(args, "token", None) or fields.get("token", "")
    if not host:
        print(
            "[错误] 未指定 host：请用 --host 参数，或在 token 文档的 host: 字段填写被控机 IP。",
            file=sys.stderr,
        )
        sys.exit(2)
    return host, token


# ---- HTTP 公共逻辑 ----------------------------------------------------------
def bridge_post(host, token, route, body=None, *, connect_timeout=HTTP_TIMEOUT,
                read_timeout=None):
    """POST 到 bridge 服务器，返回 (conn, resp)。

    connect_timeout 用于建立连接（快速发现 host 不可达）；read_timeout=None
    表示读响应不限时（exec 长静默场景必需），传数值则限时（scan/hello 防呆）。
    """
    conn = http.client.HTTPConnection(host, BRIDGE_PORT, timeout=connect_timeout)
    try:
        conn.connect()
        if conn.sock is not None:
            conn.sock.settimeout(read_timeout)
        conn.request(
            "POST",
            f"{route}?token={urllib.parse.quote(token, safe='')}",
            body=json.dumps(body) if body is not None else b"",
            headers={"Content-Type": "application/json", "Connection": "close"},
        )
        resp = conn.getresponse()
    except Exception:
        conn.close()
        raise
    return conn, resp


def _exit_connect_error(host: str, exc: Exception):
    print(
        f"[错误] 无法连接 {host}:{BRIDGE_PORT} —— {exc.__class__.__name__}: {exc}\n"
        f"请确认被控机已启动服务器（run.py server），host 正确且同处局域网。",
        file=sys.stderr,
    )
    sys.exit(3)


def _read_error_body(resp) -> str:
    detail = resp.read().decode("utf-8", "replace")
    try:
        return json.loads(detail).get("error", detail)
    except ValueError:
        return detail


# ---- 子命令：scan -----------------------------------------------------------
def _local_primary_ip():
    """UDP connect 取路由源地址（不实际发包）；失败返回 None。"""
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
            s.connect(("8.8.8.8", 80))
            return s.getsockname()[0]
    except OSError:
        return None


def _tcp_probe(ip: str) -> bool:
    try:
        with socket.create_connection((ip, BRIDGE_PORT), timeout=SCAN_TIMEOUT):
            return True
    except OSError:
        return False


def _probe_bridge(ip: str, token: str):
    """对开放端口者调 hello 验证 token；返回 (是否确认的 bridge, 信息)。"""
    try:
        conn, resp = bridge_post(ip, token, "/hello", {}, connect_timeout=2, read_timeout=3)
    except OSError:
        return False, None
    try:
        if resp.status != 200:
            return False, None
        return True, json.loads(resp.read().decode("utf-8"))
    except (OSError, ValueError):
        return False, None
    finally:
        conn.close()


def cmd_scan(args):
    fields = load_token_doc()
    token = args.token or fields.get("token", "")

    if args.cidr:
        try:
            net = ipaddress.ip_network(args.cidr, strict=False)
        except ValueError:
            print(f"[错误] 无效网段: {args.cidr}（示例格式 192.168.1.0/24）", file=sys.stderr)
            sys.exit(2)
        if net.version != 4:
            print("[错误] 本工具仅支持 IPv4 网段扫描", file=sys.stderr)
            sys.exit(2)
    else:
        local_ip = _local_primary_ip()
        if not local_ip:
            print("[错误] 未检测到本机局域网地址，请用 --cidr 指定扫描网段", file=sys.stderr)
            sys.exit(2)
        net = ipaddress.ip_network(f"{local_ip}/24", strict=False)
    targets = [str(h) for h in net.hosts()]
    targets.append("127.0.0.1")  # 始终覆盖本机回环，便于定位本机实例
    seen = set()
    targets = [t for t in targets if not (t in seen or seen.add(t))]

    t0 = time.monotonic()
    open_hosts = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=SCAN_WORKERS) as pool:
        futures = {pool.submit(_tcp_probe, ip): ip for ip in targets}
        for future in concurrent.futures.as_completed(futures):
            if future.result():
                open_hosts.append(futures[future])
    elapsed = time.monotonic() - t0
    print(f"[agent-bridge] 扫描 {net}: {BRIDGE_PORT}（{len(targets)} 个地址），"
          f"端口开放 {len(open_hosts)} 个，耗时 {elapsed:.1f}s")

    confirmed, unknown = [], []
    for ip in sorted(open_hosts):
        ok, info = _probe_bridge(ip, token)
        (confirmed if ok else unknown).append((ip, info))

    print()
    if confirmed:
        print("已确认的 bridge 服务器:")
        for ip, info in confirmed:
            print(f"  - {ip}")
            print(f"      主机名: {info.get('hostname')}  运行用户: {info.get('user')}")
            print(f"      工作目录: {info.get('cwd')}")
            print(f"      版本: {info.get('version')}  启动时刻: {info.get('started_at')}")
    else:
        print("未发现已确认的 bridge 服务器"
              "（若被控机已在运行，多半是 token 不匹配或网段不对）")
    if unknown:
        print("\n未知服务（端口开放但 token 未通过）:")
        for ip, _ in unknown:
            print(f"  - {ip}")
    sys.exit(0 if confirmed else 1)


# ---- 子命令：hello ----------------------------------------------------------
def cmd_hello(args):
    host, token = resolve_target(args)
    try:
        conn, resp = bridge_post(host, token, "/hello", {}, read_timeout=HTTP_TIMEOUT)
    except OSError as exc:
        _exit_connect_error(host, exc)
    try:
        if resp.status == 404:
            print(TOKEN_STALE_HINT, file=sys.stderr)
            sys.exit(4)
        if resp.status != 200:
            print(f"[错误] 服务器返回 HTTP {resp.status}: {_read_error_body(resp)}",
                  file=sys.stderr)
            sys.exit(1)
        payload = json.loads(resp.read().decode("utf-8"))
    finally:
        conn.close()
    print(json.dumps(payload, ensure_ascii=False, indent=2))


# ---- 子命令：exec -----------------------------------------------------------
def cmd_exec(args):
    host, token = resolve_target(args)
    command = " ".join(args.command).strip()
    if not command:
        print('[错误] 缺少要执行的命令（用法: client.py exec "<整条命令>"）',
              file=sys.stderr)
        sys.exit(2)
    body = {"command": command}
    if args.cwd:
        body["cwd"] = args.cwd
    if args.timeout is not None:
        body["timeout_seconds"] = args.timeout
    try:
        conn, resp = bridge_post(host, token, "/exec", body)  # 读不限时：长命令静默期合法
    except OSError as exc:
        _exit_connect_error(host, exc)
    exit_code = 1
    try:
        if resp.status == 404:
            print(TOKEN_STALE_HINT, file=sys.stderr)
            sys.exit(4)
        if resp.status != 200:
            print(f"[错误] 服务器返回 HTTP {resp.status}: {_read_error_body(resp)}",
                  file=sys.stderr)
            sys.exit(1)
        try:
            for raw in resp:  # 逐行读取 chunked NDJSON，实时转发
                line = raw.decode("utf-8").strip()
                if not line:
                    continue
                try:
                    event = json.loads(line)
                except ValueError:
                    sys.stdout.write(line + "\n")  # 非事件行按原文透传
                    continue
                if event.get("type") == "output":
                    sys.stdout.write(event.get("data", ""))
                    sys.stdout.flush()
                elif event.get("type") == "exit":
                    if event.get("timed_out"):
                        limit = args.timeout if args.timeout is not None else 1800
                        print(f"\n[agent-bridge] 远端命令超时被终止"
                              f"（timeout_seconds={limit}）", file=sys.stderr)
                    exit_code = int(event.get("code", 1))
        except (OSError, http.client.HTTPException) as exc:
            print(f"\n[错误] 流式响应中断 —— {exc.__class__.__name__}: {exc}",
                  file=sys.stderr)
            sys.exit(3)
    finally:
        conn.close()
    sys.exit(exit_code)


# ---- 子命令：download ---------------------------------------------------------
def cmd_download(args):
    host, token = resolve_target(args)
    try:
        conn, resp = bridge_post(host, token, "/download",
                                 {"path": args.remote_path}, read_timeout=HTTP_TIMEOUT)
    except OSError as exc:
        _exit_connect_error(host, exc)
    try:
        if resp.status != 200:
            err = _read_error_body(resp)
            if resp.status == 404 and err == "not found":
                print(TOKEN_STALE_HINT, file=sys.stderr)
                sys.exit(4)
            print(f"[错误] 下载失败（HTTP {resp.status}）: {err}", file=sys.stderr)
            sys.exit(1)
        out_path = args.out or os.path.basename(
            args.remote_path.replace("\\", "/").rstrip("/")) or "download.bin"
        expected = resp.getheader("Content-Length")
        received = 0
        digest = hashlib.sha256()
        try:
            with open(out_path, "wb") as fh:
                while True:
                    chunk = resp.read(DOWNLOAD_CHUNK_SIZE)
                    if not chunk:
                        break
                    fh.write(chunk)
                    digest.update(chunk)
                    received += len(chunk)
        except (OSError, http.client.HTTPException) as exc:
            print(f"\n[错误] 下载中断 —— {exc.__class__.__name__}: {exc}",
                  file=sys.stderr)
            sys.exit(3)
    finally:
        conn.close()
    if expected is not None and received != int(expected):
        print(f"[错误] 下载不完整：期望 {expected} 字节，实际收到 {received} 字节",
              file=sys.stderr)
        sys.exit(1)
    print(f"[agent-bridge] 已下载 {received} 字节 → {out_path}\n"
          f"  sha256: {digest.hexdigest()}")


# ---- CLI 骨架 ----------------------------------------------------------------
def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="run.py", description="agent-bridge 客户端（agent 侧，仅标准库）")
    sub = parser.add_subparsers(dest="subcommand", required=True)

    p_scan = sub.add_parser("scan", help="扫描本机网段，定位并确认 bridge 服务器")
    p_scan.add_argument("--cidr", help="扫描网段（如 192.168.1.0/24），缺省为本机主网段 /24")
    p_scan.add_argument("--token", help="验证用 token（缺省读 token 文档）")
    p_scan.set_defaults(func=cmd_scan)

    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--host", help="被控机 IP（缺省读 token 文档 host 字段）")
    common.add_argument("--token", help="访问 token（缺省读 token 文档 token 字段）")

    p_hello = sub.add_parser("hello", parents=[common],
                             help="校验服务器并打印基本信息")
    p_hello.set_defaults(func=cmd_hello)

    p_exec = sub.add_parser("exec", parents=[common],
                            help="远程执行 shell 命令（流式输出，退出码透传）")
    p_exec.add_argument("command", nargs="+", help="整条 shell 命令")
    p_exec.add_argument("--cwd", help="远端工作目录（相对路径基于服务器启动目录）")
    p_exec.add_argument("--timeout", type=float, help="超时秒数（缺省 1800）")
    p_exec.set_defaults(func=cmd_exec)

    p_dl = sub.add_parser("download", parents=[common], help="下载远端文件")
    p_dl.add_argument("remote_path", help="远端文件路径（相对路径基于服务器启动目录）")
    p_dl.add_argument("--out", help="本地保存路径（缺省当前目录同名文件）")
    p_dl.set_defaults(func=cmd_download)
    return parser


def main() -> None:
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(errors="replace")  # 防 Windows 控制台编码崩溃
        except Exception:
            pass
    args = build_parser().parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
