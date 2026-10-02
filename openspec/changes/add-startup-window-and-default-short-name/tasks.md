# Tasks: add-startup-window-and-default-short-name

> 实施顺序：Rust 补全（1）→ 桥接面与接线（2）→ 窗口尺寸（3）→ 测试与实跑（4）→
> 回归（5）→ 文档与收尾（6）。
> 决策依据见 design.md（D1–D4）；行为范围以差异规格为准。
> Windows 真机验证列入 §7 待用户验收（归档时保留未勾选）。
> 实施环境（点态）：Linux 开发机（可用 Xvfb、xdotool、ffmpeg）；Windows 不可本机运行。

## 1. 短名启动补全（Rust 核心）

- [ ] 1.1 `config::ensure_default_short_name(data_dir, device_name) -> Result<Option<String>>`：短名为空（键缺失或空值）且设备名通过 `identity::validate_short_name` → 写回并返回 `Some(名)`；设备名不合法 → 保持为空返回 `None`（不写、不截断、不替换字符）；已有非空短名 → 原样返回、不写。验证：`config_test.rs` 新用例（为空则填设备名并持久化 / 已有非空不覆盖 / 非法设备名保持为空且文件无键 / 清空后再次调用填回 / 幂等二次调用无变化）全过，`cargo test --manifest-path rust/Cargo.toml` 全绿

## 2. 桥接面与 Dart 接线

- [ ] 2.1 `api/device.rs::ensure_default_short_name()`：解析数据目录 + `sysinfo::System::host_name().unwrap_or_default()` + 调 1.1；返回生效短名。验证：`cargo test` 编译与既有用例全过
- [ ] 2.2 重跑 `flutter_rust_bridge_codegen generate`（app/ 下）并提交生成物；验证：生成物含 `ensureDefaultShortName`，`flutter analyze` 通过
- [ ] 2.3 `bridge_service.dart`：`BridgeService` 抽象与 `RustBridgeService` 增 `ensureDefaultShortName()`；`home_page_test.dart` 的 `_FakeBridgeService` 同步实现；验证：`flutter test` 全绿（假件可编译、既有 widget 用例不受影响）
- [ ] 2.4 `main.dart`：`runApp` 前调用一次 `RustBridgeService().ensureDefaultShortName()`（try/catch，失败仅 `debugPrint`、不阻断启动）；验证：`flutter analyze` 通过；行为由 4.2/4.3 断言

## 3. 启动窗口尺寸（Flutter）

- [ ] 3.1 新增 `lib/src/window_geometry.dart`：`kDefaultStartupWindowSize = Size(480, 960)`、`kMinimumWindowSize = Size(360, 720)`、`startupWindowSize(Size visible)`（高取 min(960, 可用高)；宽度不足时按宽再缩；恒 1:2）。验证：`test/window_geometry_test.dart`（1920×1080 → 480×960；小屏按可用高缩；极窄屏按宽再缩；恰为默认；各分支宽:高 = 1:2）全过
- [ ] 3.2 `pubspec.yaml` 增 `screen_retriever: ^0.2.2` 直接依赖；验证：`flutter pub get` 成功且 `pubspec.lock` 中该包版本不变（0.2.2，原为传递依赖）
- [ ] 3.3 `main.dart`：`WindowOptions` 改用 `startupWindowSize(visibleSize ?? size)`（`getPrimaryDisplay()` 读取失败回退默认 480×960）、`minimumSize: kMinimumWindowSize`；验证：`flutter analyze && flutter test` 通过；行为由 4.1/4.3 断言

## 4. 测试与实跑（Linux）

- [ ] 4.1 集成用例（新文件）：调用应用真实 `main()` 后 `windowManager.getSize()` 断言宽:高 = 1:2 且高 = min(960, 屏幕可用高)；实跑 `Xvfb :99 -screen 0 1920x1080x24` + `DISPLAY=:99 XDG_CONFIG_HOME=<临时目录> flutter test integration_test -d linux --plain-name '<用例名>'`；证据：输出留证
- [ ] 4.2 集成用例：清空短名 → `ensureDefaultShortName()` 填回（= 本机主机名，与 `Platform.localHostname` 一致）；已设置自定义名时不覆盖；随后 `init()` 与配置一致；证据：输出留证（既有冒烟用例「清空后同会话保持为空」继续通过，佐证 `app_init` 不填回）
- [ ] 4.3 Xvfb 实跑真实二进制（`flutter build linux --release` 或 debug 运行）：全新数据目录 → `xdotool search --name agent-bridge getwindowgeometry` 核对 480×960（或按可用高缩小、恒 1:2）→ `config.toml` 中 `short_name` = 本机主机名 → ffmpeg x11grab 截图留证（面板「本机短名」可见）

## 5. 测试回归

- [ ] 5.1 三套全量照跑：`cargo test --manifest-path rust/Cargo.toml`、`flutter analyze && flutter test`、集成测试（既有 6 场 + 新增）；Python 版未动照跑 `python3 -m unittest discover -s tests`；验证：全绿并记录计数

## 6. 文档与收尾

- [ ] 6.1 README 桌面应用小节：短名「为空时自动取本机设备名（清空仅在当次运行内生效）」与「启动窗口为 1:2 竖向窄窗」各一句；验证：按文档可复述行为，与规格一致
- [ ] 6.2 分提交推送（Rust 补全 / 生成物与接线 / 窗口尺寸 / 文档与证据各自成提交）；验证：`git status` 干净、与远端一致
- [ ] 6.3 证据登记：勾选附证据；未实跑不勾选
- [ ] 6.4 归档前版本推进：`app/rust/Cargo.toml` `0.5.0` → `0.6.0`；验证：面板「应用版本」与版本源一致（截图/输出留证）、提交推送
- [ ] 6.5 `/opsx:archive` 归档；主规格核对（`openspec validate --strict --no-interactive` 通过、`openspec list` 无在途变更）

## 7. 待用户验收清单（需真机人工操作）

- [ ] 7.1 **Windows 真机**：启动窗口为 1:2 竖向（480×960，或按屏幕可用高度等比缩小）；全新数据目录下短名自动为计算机名并在面板可见；清空后重启自动填回
- [ ] 7.2 **Linux 真实桌面**：真实窗管下窗口尺寸/居中/最小尺寸约束符合预期（Xvfb 已验主链路）；短名初始化与清空-重启行为符合预期

> 以上两项在归档时保留未勾选（实施环境为无窗管的 Xvfb）。结果如与预期不符，另立修复变更处理。

## 8. 跟进项（本变更不实现，记录于此）

- [ ] 8.1 自动填入短名时的一次性界面提示 —— 需要时另立
- [ ] 8.2 运行中锁定窗口比例（1:2 恒定）—— 需要时另立
