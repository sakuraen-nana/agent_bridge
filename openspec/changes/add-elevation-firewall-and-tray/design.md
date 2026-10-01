## Context

动机见 `proposal.md`；行为契约见本变更差异规格。现状与约束：

- 新应用已有服务端（变更 ②）、信息面板与 `app_init` 启动链路；数据目录解析在 `config.rs` 留了「环境注入」扩展点（变更 ① D5 预留）。
- 用户既定决策：**启动即请求管理员/root、全程以最高权限运行**（非「仅防火墙步骤提权」）。托盘常驻 + 可选开机自启亦为用户选定。
- 实施环境：Linux 开发机（root、无图形会话的日常状态；有 Xvfb 可造图形环境）；ufw 存在但未激活、firewalld 缺席、裸 nft/iptables 存在、pkexec 在位、无 autostart 目录。Windows 全部走用户验收。
- 幂等与凭据卫生是红线：防火墙只动自己的规则；剪贴板片段与会话 token 语义（重启失效）必须与规格一致。

## Goals / Non-Goals

**Goals:**

- 启动提权链（Windows 清单 / Linux pkexec 重启）＋受限模式退化路径，两路径都清晰可解释
- 防火墙检测/放行全分支可单测（命令执行注入），真机路径如实报告
- 托盘常驻/降级、开机自启、剪贴板片段四项在 Linux 侧端到端可验；Windows 侧全部进验收清单

**Non-Goals:**

- 不做「仅注册防火墙规则的独立提权助手」（已否）；不做无托盘环境下的守护进程化
- 不接管裸 nft/iptables 的规则管理（如实报告，手动放行）
- 配对与长期 token 分发仍属变更 ④（本变更片段固定为会话 token）
- 应用图标的最终设计（安装器物料属变更 ⑤；本变更只产托盘/占位图标）

## Decisions

### D1 提权形态：启动时检测 + pkexec 重启 + 受限模式退化

- `elevation.rs` 于桥接面 `app_init` 最前调用（早于服务端启动）：
  - 已是管理员（`is_elevated`，两端统一）→ `AlreadyAdmin`，继续；
  - 非管理员：Linux 且图形会话（`DISPLAY` 或 `WAYLAND_DISPLAY` 非空）且 `pkexec` 可用 → 构造 `pkexec env DISPLAY=… WAYLAND_DISPLAY=… XAUTHORITY=… XDG_RUNTIME_DIR=… DBUS_SESSION_BUS_ADDRESS=… XDG_CONFIG_HOME=<调用者家目录>/.config <本可执行文件>` 并**同步等待**其结束——pkexec 失败/取消（非零退出）→ 返回 `Restricted{reason}`（不退出，横幅提示）；成功则本实例 `std::process::exit(0)`（新实例已在运行）；
  - 其余情形 → `Restricted{reason}`。
- **受限模式不退出**：规格只要求「显著提示 + 防火墙步骤标记未执行」，保持其余功能可用；prominent 横幅由快照字段驱动（与配置损坏提示同形态、错误色）。
- 备选与取舍：仅防火墙步骤提权（用户已否）；「每次启动都强制成功否则退出」（在无 polkit/无图形的环境会把应用变成不可用，且取消弹窗的常见误操作代价过高）——记录为对用户决策的工程化解释，验收清单中请用户复核。
- 已知限制：Linux 以 root 运行 GUI 在部分 Wayland 合成器上有兼容问题（root 客户端受限）；记录并列入验收（X11/主流发行版可用；Wayland 问题后续变更处理）。

### D2 数据目录按调用者解析（兑现变更 ① 的注入点）

`config::data_dir_from` 扩展（Unix 分支）：`XDG_CONFIG_HOME` 优先；其次若 `euid == 0` 且存在 `SUDO_USER` / `PKEXEC_UID` → 由 `/etc/passwd` 解析其家目录 → `<home>/.config/agent-bridge`；否则 `HOME`。pkexec 重启路径同时显式传 `XDG_CONFIG_HOME`（双保险）。解析失败（用户不存在）回退现行为并记日志。

### D3 防火墙探测与放行（命令注入可测；只动自己的规则）

- 探测顺序与判据（全部 `LC_ALL=C`）：
  - Linux：`ufw status` 含 `Status: active` → ufw；否则 `firewall-cmd --state` = `running` → firewalld；否则 `Unsupported`（含裸 nft/iptables 情形）。
  - Windows：`netsh advfirewall show allprofiles state` 任一 profile `State ON` → Defender。
- 放行（幂等）：
  - ufw：`ufw status` 已含 `37777/tcp`（按实际端口）→ 跳过；否则 `ufw allow <port>/tcp`。
  - firewalld：`firewall-cmd --query-port=<port>/tcp` → `yes` 跳过；否则 `--permanent --add-port` + `--reload`。
  - Defender：`netsh advfirewall firewall show rule name=agent-bridge` 存在 → 跳过；否则 `add rule name=agent-bridge dir=in action=allow protocol=TCP localport=<port>`。
- 结构：`firewall::ensure(port, &dyn CommandRunner) -> FirewallReport { manager, active, applied, detail }`；生产 runner 用 `std::process::Command`，测试用注入 runner 回放各管理器输出 → 全分支单测（含已放行幂等、未激活跳过、Unsupported、命令不存在）。
- 执行时机：服务端启动成功后（端口已知），失败/跳过都只影响 `FirewallReport`，不阻断。**改动边界**：仅增/查上述规则，绝不 flush/删除他人规则。

### D4 托盘与窗口（tray_manager + window_manager；失败必降级）

- `main.dart`：`windowManager.ensureInitialized()` → `setPreventClose(true)`；`trayManager.setIcon(assets/tray_icon.png)`（32×32，PIL 生成入库）＋菜单（显示窗口 / 复制本机配置 / 退出）。
- `onWindowClose` → `windowManager.hide()`（首次提示「已最小化到托盘」气泡/通知，视插件能力）。退出菜单 → 停止服务端（`windowManager.destroy()` 触进程退出即停）。
- 托盘创建失败（无 host）→ catch：`setPreventClose(false)`，面板/日志提示「托盘不可用，关闭窗口将退出」——幽灵进程红线。
- 备选：libappindicator 自实现/各平台原生——否决（插件覆盖 Win/Linux 桌面为主，失败有降级路径）。

### D5 开机自启（状态=机制现状，不另存配置）

- `autostart.rs`：`status() -> {enabled, mechanism, detail}`、`enable()/disable()`：
  - Linux：`~/.config/autostart/agent-bridge.desktop`（`Exec=<提权启动命令>`：有 pkexec 则 `pkexec env ... <exe>`，否则直启并在 `Comment=` 注明需手动提权）；存在性即状态；写入用「临时文件+rename」幂等。
  - Windows：`reg add/delete HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v agent-bridge`；查询存在性即状态。
- 一律作用于**调用者用户**范围（HKCU / 用户家目录），不动系统级。
- GUI：「开机自启」开关（默认关）+ 状态说明行；失败给出原因（不进验收不可知项——Linux 可测）。

### D6 剪贴板片段（`share_payload`）

- Rust `api::share_payload() -> Result<String, AppError>`（在 `api/init.rs`，可访问进程级会话 token）：
  ```
  # agent-bridge 本机配置（token 为本次会话短期 token，应用重启后失效）
  [[peer]]
  uuid = "<本机 uuid>"
  short_name = "<本机短名>"      # 未设置则整行省略
  address = "<首选局域网地址>"
  port = <实际端口>
  token = "<会话 token>"
  ```
- 地址选择：`sysinfo` 接口列表 → 排除 `lo/docker/br-/veth/virbr*/tailscale*` 前缀接口 → 私网地址优先 → 取第一个；无 → `Err`（GUI 显示原因）。
- GUI：「复制本机配置」按钮（面板区）+ SnackBar 成功提示；托盘菜单同名项复用。
- 规格语义：片段固定会话 token（重启失效）；长期 token 的分发属配对（④）。

### D7 快照与界面的最小扩展

`AppSnapshot` 增 `elevation: { admin: bool, restricted_reason: Option<String> }` 与 `firewall: { manager: Option<String>, active: bool, applied: bool, detail: String }`；面板新增「管理员权限」「防火墙」行；受限模式用错误色横幅（复用现有 `_ErrorCard`）。widget 测试更新 + 新用例。

### D8 Windows 清单（requireAdministrator）

`windows/runner/runner.exe.manifest` 增加 `<requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>`——构建产物每次启动弹 UAC（符合「启动即请求」决策）；调试期 UAC 烦扰列入验收说明。Windows 全部机制（netsh、HKCU、manifest 行为）在验收清单中验证。

### D9 依赖与资源

- Rust：`is_elevated`（检测两端管理员；若镜像不可得则自实现：Unix `geteuid()==0`、Windows 经 `windows-sys` 令牌检测）。
- Flutter：`tray_manager`、`window_manager`（先验证 pub 镜像可得性与版本兼容；不可得再评估替代）。
- 资源：`assets/tray_icon.png`（PIL 生成 32×32 占位图标，变更 ⑤ 换正式物料）。

## Risks / Trade-offs

- [Linux root GUI 在 Wayland 的兼容风险] → 已知限制记入文档与验收；提权失败路径已有受限模式兜底
- [pkexec 各发行版 polkit 行为差异（弹窗文案/凭证缓存）] → 链路列用户验收；实现不依赖其返回值之外的信息
- [托盘插件在无 host 环境的异常面] → 强制 catch + 关闭语义降级；集成测试覆盖降级路径（Xvfb 即无 host）
- [ufw/firewalld 输出解析受区域影响] → 强制 `LC_ALL=C`；解析仅做包含匹配，留宽松度
- [防火墙命令权限不足（受限模式）] → 报告 `applied=false` 与原因，不重试不阻断
- [Windows 清单导致调试期 UAC 弹窗] → 验收说明；开发文档注明调试可临时取消清单（不提交）
- [剪贴板在 Xvfb 下的可用性] → X11 剪贴板无需窗口管理器；集成测试进程内读回验证

## Migration Plan

无部署态迁移：行为增量；旧配置无需变更。关窗语义在托盘可用环境改变（关窗=隐藏）——文档与首次运行时提示说明。回滚即回退提交。

## Open Questions

- 正式托盘/应用图标的美术物料 —— 变更 ⑤ 打包时统一（本变更用占位图标）
- Wayland 下 root 运行的兼容方案（如需支持）—— 另立变更评估
