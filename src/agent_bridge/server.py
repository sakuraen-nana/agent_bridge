#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""agent-bridge 被控端服务器。

局域网内受 token 保护的远程命令执行 / 文件下载通道。仅 Python 标准库，
Linux / Windows 通用；配合包内 client.py（agent 侧）与仓库根 run.py 入口使用。

安全模型：持 token 者获得与服务器运行用户等同的命令执行权（等价 SSH）；
token 明文随 URL 传输，仅限用户自己的可信局域网使用；不用时请退出本进程。
"""

from __future__ import annotations

import getpass
import hmac
import itertools
import json
import locale
import os
import platform
import secrets
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.parse
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

BRIDGE_PORT = 37777  # 固定端口（已确认不可更换，占用即失败，绝不自动换端口）
BRIDGE_VERSION = "agent-bridge/0.1.0"
DEFAULT_TIMEOUT_SECONDS = 1800
DOWNLOAD_CHUNK_SIZE = 1024 * 1024

# 服务器启动时刻的工作目录：exec 的 cwd 相对路径与 download 的相对路径均以此为基准
START_DIR = os.path.abspath(os.getcwd())
STARTED_AT = datetime.now().isoformat(timespec="seconds")

# 启动时生成，仅存本进程内存（main() 中赋值）；重启即轮换，绝不落盘
TOKEN = ""

# ---- 请求留痕（2026-09-24 二次需求变更）--------------------------------------
# 所有请求（含被拒）在服务器控制台结构化显示；token 一律脱敏；
# 被拒请求体仅限量预览、不解析不执行。并发时控制台输出允许交错。
MAX_BODY_BYTES = 10 * 1024 * 1024
_BODY_PREVIEW_BYTES = 2048

_REQ_SEQ = itertools.count(1)


def _log(text: str = "") -> None:
    print(text, flush=True)


def _ts() -> str:
    return datetime.now().strftime("%H:%M:%S")


def _mask_query(query: str) -> str:
    if not query:
        return "（无查询参数）"
    parts = []
    for key, value in urllib.parse.parse_qsl(query, keep_blank_values=True):
        if key.lower() == "token":
            shown = value[:6] + "…" if len(value) > 6 else value
            parts.append(f"token={shown}（{len(value)} 字符）")
        else:
            parts.append(f"{key}={value}")
    return "&".join(parts)


def _block_close(req_id: int, text: str) -> None:
    _log(f"└── #{req_id} {text}")


def _current_user() -> str:
    try:
        return getpass.getuser()
    except Exception:
        return os.environ.get("USERNAME") or os.environ.get("USER") or "unknown"


def get_lan_ips() -> list:
    """收集本机非回环 IPv4 地址。

    UDP connect 不会实际发包，仅让内核选定路由源地址；再辅以主机名解析。
    """
    ips = set()
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
            s.connect(("8.8.8.8", 80))
            ips.add(s.getsockname()[0])
    except OSError:
        pass
    try:
        for info in socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET):
            ip = info[4][0]
            if ip and not ip.startswith("127."):
                ips.add(ip)
    except OSError:
        pass
    return sorted(ips)


def _kill_process_tree(proc: subprocess.Popen) -> None:
    """尽力终止进程及其全部子进程，保证超时 / 断开时不遗留孤儿。"""
    try:
        if proc.poll() is not None:
            return
    except OSError:
        pass
    if os.name == "posix":
        try:
            # 独立进程组（Popen start_new_session）内整组 SIGKILL，覆盖 sh 派生的子进程
            os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            return
        except OSError:
            pass
    else:
        try:
            subprocess.run(
                ["taskkill", "/F", "/T", "/PID", str(proc.pid)],
                capture_output=True, timeout=10,
            )
            return
        except Exception:
            pass
    try:
        proc.kill()
    except OSError:
        pass


def _decode_child_output(raw: bytes) -> str:
    """子进程输出解码：优先 UTF-8，不可解时回退平台 locale（errors=replace）。

    依据：dart / git / 经 PYTHONIOENCODING 注入的 python 子进程在管道下输出
    UTF-8；cmd.exe 内建命令输出平台本地编码（Windows 为 GBK）。UTF-8 优先
    可同时正确覆盖两类来源（GBK 字节几乎不可能构成合法 UTF-8 序列）。
    """
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError:
        return raw.decode(locale.getpreferredencoding(False), errors="replace")


class BridgeHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = BRIDGE_VERSION
    sys_version = ""  # 不向未授权方泄露 Python 版本
    timeout = 60      # 连接后长期不发请求则回收线程（不影响 exec 长静默期，见 watch_disconnect）

    # ---- 请求留痕排版 ----------------------------------------------------
    def _open_block(self, tail: str = "") -> None:
        split = urllib.parse.urlsplit(self.path)
        line = (f"\n┌── #{self.req_id} {self.command} {split.path} ── {_ts()}"
                f" ── 来源 {self.client_address[0]}:{self.client_address[1]}")
        _log(line + (f" ── {tail}" if tail else ""))
        _log(f"│   查询串: {_mask_query(split.query)}")

    def _kv(self, key: str, value: str) -> None:
        _log(f"│   {key}: {value}")

    def _kv_lines(self, key: str, text: str) -> None:
        _log(f"│   {key}:")
        for line in text.splitlines() or ["（空）"]:
            _log(f"│     {line}")

    # ---- 统一 token 认证中间层 ----------------------------------------
    def _authorized(self) -> bool:
        """校验 ?token= 查询参数，恒定时间比较；不解析、不记录请求体。"""
        query = urllib.parse.urlsplit(self.path).query
        provided = urllib.parse.parse_qs(query).get("token", [""])[0]
        if not provided:
            return False
        return hmac.compare_digest(provided.encode("utf-8"), TOKEN.encode("utf-8"))

    # 覆盖默认日志：任何请求内容（含 token）不得进入任何日志渠道
    def log_message(self, format, *args):  # noqa: A002
        pass

    def _reject_404(self) -> None:
        """统一 404 并关闭连接。

        token 错误与路径不存在使用完全相同的响应（状态码、响应体、无特征头），
        不解析请求体、不写任何日志，不向未授权方泄露服务器能力信息。
        """
        body = b'{"error":"not found"}'
        self.send_response_only(404)  # 不经 send_response，避免附加 Server/Date 特征头
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.close_connection = True
        self.end_headers()
        self.wfile.write(body)

    def _send_json(self, status: int, obj: dict) -> None:
        body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.close_connection = True
        self.end_headers()
        self.wfile.write(body)

    def _read_body_bytes(self) -> bytes:
        """读取请求体（至多 MAX_BODY_BYTES），供解析与留痕预览共用。"""
        try:
            length = int(self.headers.get("Content-Length") or 0)
        except ValueError:
            length = 0
        self._body_oversize = length > MAX_BODY_BYTES
        if length <= 0:
            return b""
        return self.rfile.read(min(length, MAX_BODY_BYTES))

    def _read_json_body(self):
        """解析已读入的 JSON 请求体（仅在认证通过后调用）。返回 (obj, err)。"""
        raw = self._body
        if self._body_oversize:
            return None, f"请求体超过 {MAX_BODY_BYTES // (1024 * 1024)}MB 上限"
        if not raw:
            return None, "请求体为空"
        try:
            data = json.loads(raw.decode("utf-8"))
        except (UnicodeDecodeError, ValueError) as exc:
            return None, f"请求体不是合法 JSON: {exc}"
        if not isinstance(data, dict):
            return None, "请求体必须是 JSON 对象"
        return data, None

    def _log_rejected(self) -> None:
        """被拒请求留痕：token 状态 + 请求体限量预览（不解析不执行）。"""
        query = urllib.parse.urlsplit(self.path).query
        values = urllib.parse.parse_qs(query).get("token", [])
        if values:
            token_note = f"{values[0][:6]}…（{len(values[0])} 字符，与当前 token 不匹配）"
        else:
            token_note = "（缺失）"
        self._kv("token", token_note)
        if self._body:
            preview = self._body[:_BODY_PREVIEW_BYTES].decode("utf-8", "replace")
            more = "（仅预览，未解析未执行）"
            self._kv_lines("请求体预览 " + more, preview)
        else:
            self._kv("请求体", "（无）")

    # ---- 路由 ----------------------------------------------------------
    def do_POST(self):
        self.req_id = next(_REQ_SEQ)
        self._body = b""
        self._body_oversize = False
        self._open_block()
        self._body = self._read_body_bytes()
        if not self._authorized():
            self._log_rejected()
            _block_close(self.req_id, "已丢弃 · 统一 404（token 无效或缺失）")
            self._reject_404()
            return
        route = urllib.parse.urlsplit(self.path).path
        if route == "/hello":
            self._handle_hello()
        elif route == "/exec":
            self._handle_exec()
        elif route == "/download":
            self._handle_download()
        else:
            self._kv("请求体预览", self._body[:_BODY_PREVIEW_BYTES].decode("utf-8", "replace")
                     or "（无）")
            _block_close(self.req_id, "404 · token 有效但路径不存在")
            self._reject_404()

    def do_GET(self):
        self.req_id = next(_REQ_SEQ)
        self._body = b""
        self._body_oversize = False
        self._open_block()
        if not self._authorized():
            self._log_rejected()
            _block_close(self.req_id, "已丢弃 · 统一 404（token 无效或缺失）")
            self._reject_404()
            return
        _block_close(self.req_id, "404 · 本服务仅提供 POST 端点")
        self._reject_404()

    # ---- POST /hello ----------------------------------------------------
    def _handle_hello(self):
        payload = {
            "version": BRIDGE_VERSION,
            "hostname": socket.gethostname(),
            "user": _current_user(),
            "system": platform.system(),
            "release": platform.release(),
            "platform": platform.platform(),
            "cwd": START_DIR,
            "lan_ips": get_lan_ips(),
            "started_at": STARTED_AT,
        }
        self._send_json(200, payload)
        self._kv_lines("响应 200", json.dumps(payload, ensure_ascii=False, indent=2))
        _block_close(self.req_id, "hello 完成")

    # ---- POST /exec -----------------------------------------------------
    def _send_event(self, obj: dict) -> None:
        """以一个 HTTP chunk 发送一条 NDJSON 事件并立即刷出。"""
        payload = (json.dumps(obj, ensure_ascii=False) + "\n").encode("utf-8")
        self.wfile.write(f"{len(payload):X}\r\n".encode("ascii") + payload + b"\r\n")
        self.wfile.flush()

    def _end_chunked(self) -> None:
        self.wfile.write(b"0\r\n\r\n")
        self.wfile.flush()

    def _handle_exec(self):
        data, err = self._read_json_body()
        if err:
            self._kv("请求体预览", self._body[:_BODY_PREVIEW_BYTES].decode("utf-8", "replace")
                     or "（无）")
            _block_close(self.req_id, f"已拒绝 · 400 {err}")
            self._send_json(400, {"error": err})
            return
        command = data.get("command")
        if not isinstance(command, str) or not command.strip():
            _block_close(self.req_id, "已拒绝 · 400 缺少必填字段 command（非空字符串）")
            self._send_json(400, {"error": "缺少必填字段 command（非空字符串）"})
            return

        cwd = START_DIR
        raw_cwd = data.get("cwd")
        if raw_cwd is not None:
            if not isinstance(raw_cwd, str) or not raw_cwd.strip():
                _block_close(self.req_id, "已拒绝 · 400 cwd 必须是非空字符串")
                self._send_json(400, {"error": "cwd 必须是非空字符串"})
                return
            cwd = os.path.abspath(os.path.join(START_DIR, raw_cwd))
            if not os.path.isdir(cwd):
                _block_close(self.req_id, f"已拒绝 · 400 cwd 不存在或不是目录: {cwd}")
                self._send_json(400, {"error": f"cwd 不存在或不是目录: {cwd}"})
                return

        timeout = DEFAULT_TIMEOUT_SECONDS
        raw_timeout = data.get("timeout_seconds")
        if raw_timeout is not None:
            if not isinstance(raw_timeout, (int, float)) or raw_timeout <= 0:
                _block_close(self.req_id, "已拒绝 · 400 timeout_seconds 必须是正数")
                self._send_json(400, {"error": "timeout_seconds 必须是正数"})
                return
            timeout = raw_timeout

        # 参数留痕：完整显示全部输入参数
        self._kv("command", command)
        self._kv("cwd", cwd if raw_cwd is not None else f"{cwd}（未提供 → 服务器启动目录）")
        self._kv("timeout_seconds", f"{timeout}" + ("（缺省）" if raw_timeout is None else ""))
        _log("│   ── 实时输出（stdout+stderr 合并）──")

        # 流式响应头：HTTP/1.1 chunked，逐事件推送
        self.send_response(200)
        self.send_header("Content-Type", "application/x-ndjson; charset=utf-8")
        self.send_header("Transfer-Encoding", "chunked")
        self.send_header("Connection", "close")
        self.close_connection = True
        self.end_headers()

        self._run_command_stream(command, cwd, timeout)

    def _run_command_stream(self, command: str, cwd: str, timeout: float) -> None:
        proc = None
        timed_out = threading.Event()
        finished = threading.Event()

        def kill_tree():
            if proc is not None:
                _kill_process_tree(proc)

        def on_timeout():
            timed_out.set()
            kill_tree()

        def watch_disconnect():
            # 客户端提前断开时 recv 返回 EOF（或连接复位）→ 终止子进程。
            # 注意：handler 的 60s socket 超时会让 recv 周期性抛 socket.timeout，
            # 那是命令的静默期（编译/下载常达数分钟）而非断开，必须继续等待——
            # 否则长静默命令会被误杀（已修复的回归缺陷）。
            try:
                while not finished.is_set():
                    try:
                        data = self.connection.recv(1)
                    except socket.timeout:
                        continue
                    if data == b"":
                        break
            except OSError:
                pass
            if not finished.is_set():
                kill_tree()

        popen_kwargs = {}
        if os.name == "posix":
            popen_kwargs["start_new_session"] = True  # 独立进程组，便于整组终止
        # 注入 PYTHONIOENCODING：python 子进程（验收触发器等）在管道 stdout 下
        # 也输出 UTF-8，避免非 ASCII 字符（如 flutter 的 ✓）触发 GBK 编码崩溃
        child_env = {**os.environ, "PYTHONIOENCODING": "utf-8"}
        t0 = time.monotonic()
        try:
            proc = subprocess.Popen(
                command,
                shell=True,                 # POSIX → /bin/sh；Windows → cmd.exe
                cwd=cwd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,   # stdout 与 stderr 合并为一路
                env=child_env,
                **popen_kwargs,
            )
        except OSError as exc:
            finished.set()
            _block_close(self.req_id, f"启动失败 · {exc}")
            try:
                self._send_event({"type": "exit", "code": 127,
                                  "duration_ms": 0, "error": str(exc)})
                self._end_chunked()
            except OSError:
                pass
            return

        timer = threading.Timer(timeout, on_timeout)
        timer.daemon = True
        timer.start()
        watcher = threading.Thread(target=watch_disconnect, daemon=True)
        watcher.start()

        client_gone = False
        try:
            for raw_line in proc.stdout:
                text = _decode_child_output(raw_line)
                _log(text.rstrip("\n"))               # 本地同步回显（先于发送，本地不缺行）
                self._send_event({"type": "output", "data": text})
        except (BrokenPipeError, ConnectionResetError, OSError):
            client_gone = True
        finally:
            finished.set()
            timer.cancel()

        if client_gone:
            kill_tree()
        try:
            proc.wait()  # 无论哪条路径都收尸，不留僵尸
        except OSError:
            pass
        _log("│   ── 输出结束 ──")

        if client_gone:
            _block_close(self.req_id, "客户端断开 · 子进程已终止，无孤儿")
            return  # 客户端已断开，不再尝试写响应

        duration_ms = int((time.monotonic() - t0) * 1000)
        exit_event = {"type": "exit", "code": proc.returncode, "duration_ms": duration_ms}
        if timed_out.is_set():
            exit_event["timed_out"] = True
        summary = (f"执行完成 · exit {proc.returncode} · 耗时 {duration_ms}ms"
                   + (" · 已超时终止" if timed_out.is_set() else ""))
        try:
            self._send_event(exit_event)
            self._end_chunked()
            _block_close(self.req_id, summary)
        except (BrokenPipeError, ConnectionResetError, OSError):
            _block_close(self.req_id, summary + "（收尾时客户端已断开）")

    # ---- POST /download ---------------------------------------------------
    def _handle_download(self):
        data, err = self._read_json_body()
        if err:
            _block_close(self.req_id, f"已拒绝 · 400 {err}")
            self._send_json(400, {"error": err})
            return
        path = data.get("path")
        if not isinstance(path, str) or not path.strip():
            _block_close(self.req_id, "已拒绝 · 400 缺少必填字段 path（非空字符串）")
            self._send_json(400, {"error": "缺少必填字段 path（非空字符串）"})
            return
        target = os.path.abspath(os.path.join(START_DIR, path))
        # 文件指令留痕：仅显示目标文件完整绝对路径（不显示内容）
        self._kv("目标文件", target)
        if not os.path.exists(target):
            _block_close(self.req_id, "已拒绝 · 404 文件不存在")
            self._send_json(404, {"error": "文件不存在", "path": target})
            return
        if not os.path.isfile(target):
            _block_close(self.req_id, "已拒绝 · 400 路径是目录而非文件")
            self._send_json(400, {"error": "路径是目录而非文件", "path": target})
            return

        size = os.path.getsize(target)
        self.send_response(200)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(size))
        self.send_header("Connection", "close")
        self.close_connection = True
        self.end_headers()
        try:
            with open(target, "rb") as fh:
                while True:
                    chunk = fh.read(DOWNLOAD_CHUNK_SIZE)
                    if not chunk:
                        break
                    self.wfile.write(chunk)
                    self.wfile.flush()
            _block_close(self.req_id, f"下载完成 · 已发送 {size} 字节")
        except (BrokenPipeError, ConnectionResetError, OSError):
            _block_close(self.req_id, "传输中断 · 客户端断开（无子进程，直接结束）")


def build_banner(token: str) -> str:
    ips = get_lan_ips()
    ip_list = ", ".join(ips) if ips else "（未检测到，请以 ipconfig / ip addr 输出为准）"
    call_ip = ips[0] if ips else "127.0.0.1"
    lines = [
        "=" * 66,
        "  agent-bridge 被控端服务器已启动（仅限可信局域网使用）",
        "-" * 66,
        f"  Token    : {token}",
        f"  端口     : {BRIDGE_PORT}（绑定 0.0.0.0）",
        f"  运行用户 : {_current_user()}",
        f"  工作目录 : {START_DIR}",
        f"  局域网 IP: {ip_list}",
        f"  启动时刻 : {STARTED_AT}",
        f"  版本     : {BRIDGE_VERSION}",
        "  请求留痕 : 所有请求（含被拒）与命令输出实时显示于本控制台",
        "-" * 66,
        "  调用示例（在 agent / 开发机上执行；入口在工具根目录）:",
        f'    curl -X POST "http://{call_ip}:{BRIDGE_PORT}/hello?token={token}"',
        "    python3 run.py scan",
        f"    python3 run.py hello --host {call_ip} --token {token}",
        f'    python3 run.py exec "git pull" --host {call_ip} --token {token}',
        f"    python3 run.py download <远程文件> --host {call_ip} --token {token}",
        "-" * 66,
        "  ⚠ token 仅存于本进程内存：重启即轮换、旧 token 立即失效；",
        "    请勿将 token 写入文件、日志或任何同步渠道。",
        "=" * 66,
    ]
    return "\n".join(lines)


def main() -> None:
    global TOKEN
    for stream in (sys.stdout, sys.stderr):
        try:
            stream.reconfigure(errors="replace")  # 防 Windows 控制台编码崩溃
        except Exception:
            pass

    # 端口预检：Windows 的 SO_REUSEADDR 可能允许重复绑定，先显式探测回环
    probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    probe.settimeout(1.0)
    try:
        if probe.connect_ex(("127.0.0.1", BRIDGE_PORT)) == 0:
            print(f"[错误] 端口 {BRIDGE_PORT} 已被占用（本机已有进程监听）。", file=sys.stderr)
            print("agent-bridge 使用固定端口，不自动更换；请先停止已运行的实例。", file=sys.stderr)
            sys.exit(1)
    finally:
        probe.close()

    TOKEN = secrets.token_urlsafe(24)  # 密码学安全随机源，24 字节熵；仅存内存

    try:
        server = ThreadingHTTPServer(("0.0.0.0", BRIDGE_PORT), BridgeHandler)
    except OSError as exc:
        print(f"[错误] 无法绑定 0.0.0.0:{BRIDGE_PORT}: {exc}", file=sys.stderr)
        print("agent-bridge 使用固定端口，不自动更换；请先释放该端口。", file=sys.stderr)
        sys.exit(1)
    server.daemon_threads = True  # 并发请求互不阻塞；主进程退出时回收全部线程

    print(build_banner(TOKEN), flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n[agent-bridge] 收到中断，服务器已停止。", flush=True)
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
