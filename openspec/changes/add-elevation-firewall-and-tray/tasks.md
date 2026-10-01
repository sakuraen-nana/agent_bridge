# Tasks: add-elevation-firewall-and-tray

> 实施顺序：提权与数据目录（1）→ 防火墙（2）→ 托盘与窗口（3）→ 开机自启（4）→
> 剪贴板片段（5）→ 测试回归（6）→ Linux 实跑（7）→ 文档与收尾（9）。
> 决策依据见 design.md（D1–D9）；行为范围以差异规格为准。Windows 与
> Linux 桌面 pkexec 弹窗链路列入 §8 待用户验收（归档时保留未勾选）。

## 1. 提权与数据目录

- [ ] 1.1 `is_elevated` 依赖引入（或自实现两端检测）+ `elevation.rs`：状态判定（AlreadyAdmin / 可重启 / Restricted{原因}）；验证：单测——root 直通（本机即 root）、无图形会话为非 root 时报 Restricted（setpriv 模拟）、pkexec 缺失时报 Restricted
- [ ] 1.2 pkexec 重启命令构造（显式 env 白名单，含 `XDG_CONFIG_HOME=<调用者 .config>`）与「等待 → 失败进受限模式 / 成功退出本实例」；验证：单测锁定命令与环境；真机路径见 §7（本机 root 直达）
- [ ] 1.3 `config.rs` 数据目录按调用者：`SUDO_USER`/`PKEXEC_UID` → /etc/passwd 家目录 → `<home>/.config`；验证：单测（注入 env）覆盖 SUDO_USER / PKEXEC_UID / 用户不存在回退
- [ ] 1.4 `AppSnapshot.elevation` 入快照 + 受限模式横幅（错误色）；验证：widget 测试新用例 + codegen 重跑

## 2. 防火墙

- [ ] 2.1 `firewall.rs`：`CommandRunner` 注入 + ufw / firewalld / Defender 探测与幂等放行全实现（`LC_ALL=C`）；验证：单测用注入 runner 回放各管理器输出——活跃放行、已放行跳过、未激活跳过、Unsupported、命令缺失，全分支覆盖
- [ ] 2.2 接入 `app_init`（服务端启动后执行，端口取实际绑定值）；`AppSnapshot.firewall` 快照 + 面板「防火墙」行；验证：widget 测试 + 实跑（§7：本机 ufw 未激活 → 跳过路径）
- [ ] 2.3 Windows Defender 分支（netsh 探测/加规则/查重）实现完备（不可本机验证）；验证：单测注入 netsh 输出样例；真机进 §8

## 3. 托盘与窗口

- [ ] 3.1 依赖验证与接入：`tray_manager` / `window_manager` 经 pub 镜像可得并编译通过（不可得则评估替代并在证据中说明）；`main.dart` 接线 `setPreventClose`；验证：`flutter build linux` 成功
- [ ] 3.2 托盘图标资源（PIL 生成 32×32 PNG 入库）+ 菜单（显示窗口 / 复制本机配置 / 退出）；关窗 → 隐藏；托盘创建失败 → catch 降级（`setPreventClose(false)` + 提示）；验证：集成测试——Xvfb（无托盘 host）下为降级路径：关窗触发后进程存活/退出语义符合降级定义；菜单动作单测（显示/退出回调）
- [ ] 3.3 退出路径：托盘菜单退出 → 进程结束（服务端随进程停止）；验证：集成测试（Xvfb 可直接 destroy 验证）+ 实跑

## 4. 开机自启

- [ ] 4.1 `autostart.rs`：Linux `.desktop` 写/删（Exec 指向应用，提权由应用启动逻辑自理；临时文件+rename 幂等）/ 状态查询；Windows `HKCU\...\Run` 经 `reg` 的查/写/删；验证：单测（Linux 侧真实临时家目录 + 注入 Windows runner 输出）
- [ ] 4.2 GUI 开关（默认关）+ 状态行；桥接面 `set_autostart(bool)` / `autostart_status()`；验证：widget 测试；实跑写/删文件核对（§7）

## 5. 剪贴板片段

- [ ] 5.1 `api::share_payload()`：片段生成（uuid/短名可选/地址选择（排除虚拟接口、私网优先）/端口/会话 token）；无地址或服务端未运行 → 可读错误；验证：单测（地址筛选规则、短名缺省、格式可直接被 TOML 解析）
- [ ] 5.2 GUI「复制本机配置」按钮（面板区 + 托盘菜单复用）+ `Clipboard.setData` + SnackBar；验证：widget 测试（按钮触发 → 服务被调用）
- [ ] 5.3 端到端有效性：集成测试——点按按钮后读回系统剪贴板，将片段写入对端配置并以 CLI 连通本机 `/hello`（会话 token 有效）；重启应用后旧片段 token 被拒（轮换语义）；验证：集成测试两段 + 实跑

## 6. 测试回归

- [ ] 6.1 Rust 全量；验证：`cargo test` 退出码 0、用例数不少于变更前（46）
- [ ] 6.2 Flutter 全量；验证：`flutter analyze` 零问题、`flutter test` 全绿、集成测试全绿（含新增场景）
- [ ] 6.3 Python 版回归；验证：`python3 -m unittest discover -s tests` 全绿（69）

## 7. Linux 实跑验证（本机）

- [ ] 7.1 提权路径实跑：本机以 root 启动 → 面板「管理员权限：已具备」；以 `setpriv nobody` 且无 DISPLAY 启动 → 受限模式横幅（截图）且服务端仍运行；`SUDO_USER=<某用户> sudo` 语义核对（解析单测 + 真机以环境变量模拟核对数据目录落点）
- [ ] 7.2 防火墙实跑：本机 ufw 未激活 → 面板「未激活、无需放行」；模拟 Unsupported（临时 PATH 排除 ufw/firewall-cmd）→「需手动放行」提示；截图/输出留证
- [ ] 7.3 托盘降级实跑：Xvfb（无 StatusNotifier host）→ 提示托盘不可用、关窗即退出（行为与升级前一致）；截图留证
- [ ] 7.4 开机自启实跑：开关开启 → `~/.config/autostart/agent-bridge.desktop` 生成（内容含提权启动命令）；再关闭 → 删除；重复开启两次仍仅一份；留证
- [ ] 7.5 剪贴板实跑：点击复制 → 读回剪贴板内容为完整片段 → 写入 CLI 侧配置 → `hello/exec` 以片段所载会话 token 连通；留证
- [ ] 7.6 回归核对：上述实跑后 `server.log` 无 token 取值；配置目录仍 0600/0700；`peers`/面板字段一致

## 8. 待用户验收清单（需真机人工操作）

- [ ] 8.1 **Linux 桌面**：普通用户图形会话启动 → pkexec 弹窗（取消一次 → 受限模式横幅；再启动确认 → 以 root 运行且数据目录仍在用户目录）；Wayland 会话记录是否可用（已知限制）
- [ ] 8.2 **Linux 桌面**：托盘图标出现、菜单三项可用；关窗隐藏、服务端通道保持；自启项重启系统后生效（含提权弹窗）
- [ ] 8.3 **Windows**：构建并启动 → UAC 弹窗（清单 requireAdministrator）→ 面板「管理员权限：已具备」
- [ ] 8.4 **Windows**：Defender 活跃时自动放行 37777/tcp（`netsh advfirewall firewall show rule name=agent-bridge` 核对）；重复启动不重复添加
- [ ] 8.5 **Windows**：托盘常驻、关窗隐藏、菜单三项；HKCU Run 自启开关生效
- [ ] 8.6 **Windows**：复制本机配置 → 粘贴到对端（或用 CLI）连通验证

## 9. 文档与收尾

- [ ] 9.1 `README.md`：安全声明扩充（全程管理员、自动放行规则及其边界、托盘常驻、剪贴板片段为会话 token）；桌面应用章节补托盘/自启/复制配置用法与 Wayland 已知限制
- [ ] 9.2 `AGENTS.md`：状态行更新（0.3.0、三个变更已归档）
- [ ] 9.3 分提交推送（提权与数据目录 / 防火墙 / 托盘与自启 / 剪贴板 / 文档各自成提交）；验证：`git status` 干净、与远端一致
- [ ] 9.4 证据登记：勾选附证据；未实跑不勾选
- [ ] 9.5 归档前版本推进：`0.2.0` → `0.3.0`；验证：组件版本一致、提交推送

## 10. 跟进项（本变更不实现，记录于此）

- [ ] 10.1 Wayland 会话下 root GUI 的兼容方案 —— 需要时另立
- [ ] 10.2 防火墙的裸 nft/iptables 规则接管 —— 现为如实报告 + 手动放行
- [ ] 10.3 托盘通知（关窗提示气泡）的跨平台一致性 —— 视插件能力后续评估
