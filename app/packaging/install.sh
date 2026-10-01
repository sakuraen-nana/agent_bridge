#!/bin/sh
# agent-bridge 用户级 CLI 安装（tar.gz 与 AppImage 两种布局皆适用；幂等）。
# 把 agent-bridge 命令软链到 ~/.local/bin；不写系统目录、不需要 root。
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
TARGET="${BIN_DIR}/agent-bridge"

mkdir -p "${BIN_DIR}"

if [ -x "${HERE}/bin/agent-bridge" ]; then
    # tar.gz 布局：直接软链真实 CLI 二进制
    ln -sf "${HERE}/bin/agent-bridge" "${TARGET}"
elif [ -f "${HERE}/usr/bin/agent-bridge" ] && [ -x "${HERE}/AppRun" ]; then
    # 已解包的 AppDir 布局
    ln -sf "${HERE}/usr/bin/agent-bridge" "${TARGET}"
else
    # AppImage 布局：生成 wrapper（借 AppRun 的 CLI 转发）
    APPIMAGE="$(ls "${HERE}"/agent-bridge-*.AppImage 2>/dev/null | head -n 1 || true)"
    if [ -z "${APPIMAGE}" ]; then
        echo "未找到 agent-bridge 可执行文件（tar.gz 或 AppImage 布局）" >&2
        exit 1
    fi
    WRAPPER="${HERE}/agent-bridge-cli-wrapper.sh"
    printf '#!/bin/sh\nexec "%s" "$@"\n' "${APPIMAGE}" > "${WRAPPER}"
    chmod +x "${WRAPPER}"
    ln -sf "${WRAPPER}" "${TARGET}"
fi

echo "已安装: ${TARGET}"
case ":${PATH}:" in
    *":${BIN_DIR}:"*) echo "PATH 已包含 ${BIN_DIR}，可直接使用 agent-bridge 命令" ;;
    *) echo "提示: 请把 ${BIN_DIR} 加入 PATH（例如在 ~/.bashrc 中 export PATH=\"${BIN_DIR}:\$PATH\"）" ;;
esac
