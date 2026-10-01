#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""agent-bridge Linux 打包脚本（仅 Python 标准库）。

产物（输出到 app/dist/，均带版本号与 SHA256 校验清单）：
  - agent-bridge_<版本>_amd64.deb            安装包（CLI 直接进 /usr/bin）
  - agent-bridge-<版本>-x86_64.AppImage     自包含单文件（AppRun 支持 CLI 转发）
  - agent-bridge-<版本>-linux-x86_64.tar.gz 便携包（含 install.sh）
  - SHA256SUMS

版本来源：app/rust/Cargo.toml 的 package.version（全仓唯一之新应用版本源）。

用法：
  python3 app/packaging/build-linux.py [--skip-flutter] [--skip-appimage]
环境：
  - AppImage 需要 appimagetool：优先 PATH，其次环境变量 $APPIMAGETOOL 指向其文件；
    缺失时明确报错（见下文提示），deb 与 tar.gz 仍照常产出。
"""

import argparse
import hashlib
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
from pathlib import Path

APP = Path(__file__).resolve().parents[1]          # app/
ROOT = APP.parent                                   # 仓库根
DIST = APP / "dist"
PACKAGING = APP / "packaging"
ICONS = PACKAGING / "icons"
BUNDLE = APP / "build" / "linux" / "x64" / "release" / "bundle"
CLI_RELEASE = APP / "rust" / "target" / "release" / "agent-bridge"
CARGO_TOML = APP / "rust" / "Cargo.toml"


def log(message: str) -> None:
    print(f"[packaging] {message}", flush=True)


def fail(message: str) -> "NoReturn":  # noqa: F821 - 文档字符串友好
    print(f"[packaging] ERROR: {message}", file=sys.stderr, flush=True)
    sys.exit(1)


def version() -> str:
    text = CARGO_TOML.read_text(encoding="utf-8")
    match = re.search(r'^version = "([^"]+)"', text, flags=re.MULTILINE)
    if not match:
        fail(f"未能从 {CARGO_TOML} 解析 package.version")
    return match.group(1)


def run(cmd, cwd=None) -> None:
    log("$ " + " ".join(str(part) for part in cmd))
    subprocess.run([str(part) for part in cmd], cwd=cwd, check=True)


def copy_tree(src: Path, dst: Path) -> None:
    if dst.exists():
        shutil.rmtree(dst)
    shutil.copytree(src, dst, symlinks=True)


def make_executable(path: Path) -> None:
    mode = path.stat().st_mode
    path.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def desktop_entry(exec_path: str) -> str:
    return (
        "[Desktop Entry]\n"
        "Type=Application\n"
        "Name=agent-bridge\n"
        "Comment=局域网远程执行桥（远程命令执行与文件下载）\n"
        f"Exec={exec_path}\n"
        "Icon=agent-bridge\n"
        "Terminal=false\n"
        "Categories=Utility;Network;\n"
    )


def app_run_script() -> str:
    return (
        "#!/bin/sh\n"
        "# AppRun：首个参数为 CLI 子命令或 CLI 级选项时转发给内嵌 CLI，否则启动图形界面。\n"
        'HERE="$(dirname "$(readlink -f "$0")")"\n'
        'case "$1" in\n'
        "  peers|hello|exec|download|token|-h|--help|-V|--version)\n"
        '    exec "$HERE/usr/bin/agent-bridge" "$@" ;;\n'
        "esac\n"
        'exec "$HERE/usr/lib/agent-bridge/agent_bridge_app" "$@"\n'
    )


def build_deb(ver: str, workdir: Path) -> Path:
    root = workdir / "deb-root"
    copy_tree(BUNDLE, root / "usr/lib/agent-bridge")
    (root / "usr/bin").mkdir(parents=True, exist_ok=True)
    shutil.copy2(CLI_RELEASE, root / "usr/bin/agent-bridge")
    make_executable(root / "usr/bin/agent-bridge")
    apps = root / "usr/share/applications"
    apps.mkdir(parents=True, exist_ok=True)
    (apps / "agent-bridge.desktop").write_text(
        desktop_entry("/usr/lib/agent-bridge/agent_bridge_app"), encoding="utf-8"
    )
    icons = root / "usr/share/icons/hicolor/256x256/apps"
    icons.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ICONS / "agent-bridge.png", icons / "agent-bridge.png")

    control_dir = root / "DEBIAN"
    control_dir.mkdir(parents=True, exist_ok=True)
    (control_dir / "control").write_text(
        "Package: agent-bridge\n"
        f"Version: {ver}\n"
        "Architecture: amd64\n"
        "Maintainer: agent-bridge maintainers\n"
        "Section: utils\n"
        "Priority: optional\n"
        "Depends: libgtk-3-0 | libgtk-3-0t64, libglib2.0-0 | libglib2.0-0t64\n"
        "Description: LAN remote execution bridge (desktop app)\n"
        " Provides the agent-bridge GUI and the `agent-bridge` client command\n"
        " on PATH; devices pair over the LAN to run commands and fetch files.\n",
        encoding="utf-8",
    )

    deb_path = DIST / f"agent-bridge_{ver}_amd64.deb"
    run(["dpkg-deb", "--build", "--root-owner-group", str(root), str(deb_path)])
    return deb_path


def build_tarball(ver: str, workdir: Path) -> Path:
    stage = workdir / f"agent-bridge-{ver}-linux-x86_64"
    copy_tree(BUNDLE, stage / "app")
    (stage / "bin").mkdir(parents=True, exist_ok=True)
    shutil.copy2(CLI_RELEASE, stage / "bin/agent-bridge")
    make_executable(stage / "bin/agent-bridge")
    shutil.copy2(PACKAGING / "install.sh", stage / "install.sh")
    make_executable(stage / "install.sh")

    tar_path = DIST / f"agent-bridge-{ver}-linux-x86_64.tar.gz"
    with tarfile.open(tar_path, "w:gz") as archive:
        archive.add(stage, arcname=stage.name)
    return tar_path


def find_appimagetool() -> "str | None":
    from shutil import which

    found = which("appimagetool")
    if found:
        return found
    override = os.environ.get("APPIMAGETOOL", "").strip()
    if override and Path(override).exists():
        return override
    return None


def build_appimage(ver: str, workdir: Path) -> "Path | None":
    tool = find_appimagetool()
    if tool is None:
        print(
            "[packaging] ERROR: 未找到 appimagetool —— 跳过 AppImage。\n"
            "  获取方式（任一）：\n"
            "    1) 发行版/包管理器安装 appimagetool；\n"
            "    2) 从 GitHub Releases 下载后：\n"
            "       curl -LO https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage\n"
            "       chmod +x appimagetool-x86_64.AppImage\n"
            "       ./appimagetool-x86_64.AppImage --appimage-extract   # 无 FUSE 时\n"
            "       export APPIMAGETOOL=$PWD/squashfs-root/AppRun\n"
            "  另：若网络受限导致 appimagetool 无法自动下载 type2 runtime，可手动下载后：\n"
            "     curl -LO https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64\n"
            "     export APPIMAGE_RUNTIME=$PWD/runtime-x86_64\n"
            "  本次已产出的 deb 与 tar.gz 不受影响。",
            file=sys.stderr,
            flush=True,
        )
        return None

    appdir = workdir / "agent-bridge.AppDir"
    copy_tree(BUNDLE, appdir / "usr/lib/agent-bridge")
    (appdir / "usr/bin").mkdir(parents=True, exist_ok=True)
    shutil.copy2(CLI_RELEASE, appdir / "usr/bin/agent-bridge")
    make_executable(appdir / "usr/bin/agent-bridge")
    (appdir / "AppRun").write_text(app_run_script(), encoding="utf-8")
    make_executable(appdir / "AppRun")
    (appdir / "agent-bridge.desktop").write_text(
        desktop_entry("agent_bridge_app"), encoding="utf-8"
    )
    shutil.copy2(ICONS / "agent-bridge.png", appdir / "agent-bridge.png")
    shutil.copy2(ICONS / "agent-bridge.png", appdir / ".DirIcon")
    shutil.copy2(PACKAGING / "install.sh", appdir / "install.sh")
    make_executable(appdir / "install.sh")

    out = DIST / f"agent-bridge-{ver}-x86_64.AppImage"
    env = dict(os.environ)
    env.setdefault("ARCH", "x86_64")
    cmd = [tool, "--no-appstream"]
    runtime = os.environ.get("APPIMAGE_RUNTIME", "").strip()
    if runtime:
        # 受限网络获取不了内置 runtime 时：从 type2-runtime Releases 下载后经此传入
        cmd += ["--runtime-file", runtime]
    cmd += [str(appdir), str(out)]
    log("$ " + " ".join(cmd))
    subprocess.run(cmd, check=True, env=env)
    return out


def write_checksums(artifacts: "list[Path]") -> Path:
    lines = []
    for path in sorted(artifacts):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        lines.append(f"{digest}  {path.name}")
    sums = DIST / "SHA256SUMS"
    sums.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return sums


def main() -> None:
    parser = argparse.ArgumentParser(description="agent-bridge Linux 打包")
    parser.add_argument("--skip-flutter", action="store_true", help="跳过 flutter build")
    parser.add_argument("--skip-appimage", action="store_true", help="跳过 AppImage")
    args = parser.parse_args()

    ver = version()
    log(f"版本（取自 {CARGO_TOML.relative_to(ROOT)}）: {ver}")
    DIST.mkdir(parents=True, exist_ok=True)

    if not args.skip_flutter:
        run(["flutter", "build", "linux", "--release"], cwd=APP)
    run(["cargo", "build", "--release", "--bin", "agent-bridge"], cwd=APP / "rust")

    if not BUNDLE.is_dir():
        fail(f"未找到应用束 {BUNDLE}（先构建或去掉 --skip-flutter）")
    if not CLI_RELEASE.is_file():
        fail(f"未找到 CLI 产物 {CLI_RELEASE}")

    workdir = DIST / "stage"
    if workdir.exists():
        shutil.rmtree(workdir)
    workdir.mkdir(parents=True)

    artifacts = []
    artifacts.append(build_deb(ver, workdir))
    log(f"deb 产出: {artifacts[-1]}")
    artifacts.append(build_tarball(ver, workdir))
    log(f"tar.gz 产出: {artifacts[-1]}")
    if not args.skip_appimage:
        appimage = build_appimage(ver, workdir)
        if appimage is not None:
            artifacts.append(appimage)
            log(f"AppImage 产出: {appimage}")
            # install.sh 随 AppImage 一并放到 dist（AppImage 布局的用户级安装）
            shutil.copy2(PACKAGING / "install.sh", DIST / "install.sh")
            make_executable(DIST / "install.sh")

    sums = write_checksums(artifacts)
    log(f"校验清单: {sums}")
    log("完成：" + "、".join(path.name for path in artifacts))


if __name__ == "__main__":
    main()
