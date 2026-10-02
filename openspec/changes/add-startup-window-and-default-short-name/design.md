## Context

动机见 `proposal.md`；行为契约见差异规格。现状与约束：

- 启动窗口尺寸在 `app/lib/main.dart` 的 `WindowOptions` 设定（现 1000×720、最小 760×560）；`window_manager` 0.5.2 经 `waitUntilReadyToShow` 应用窗口选项。
- `screen_retriever` 0.2.2 已随 `window_manager` 传递引入（锁文件在树），`Display` 含 `size` 与可空 `visibleSize`；Linux 后端以 `gdk_monitor_get_workarea` 计算可用区域（扣除面板），Windows 后端亦有实现。直接使用需提升为直接依赖。
- 短名逻辑：`config::load_or_create` 被多路调用——GUI 初始化、服务端 `device_snapshot`（按配置 mtime 刷新缓存）、配对 API、配置分享与 CLI；`set_short_name(None)`（清空）现为删除键。GUI「清空」按用户确认的语义：当次运行内生效、重启填回。
- **关键陷阱**：清空按钮（`api::device::set_short_name`）为返回刷新后的快照也调用 `app_init`——自动补全若放进 `app_init`，清空会被当场填回。
- 桥接面改动（新增函数）须重跑 `flutter_rust_bridge_codegen generate` 并提交生成物（工具 2.11.1 已就位，与依赖版本一致）。
- 设备名取值已有先例：`sysinfo::System::host_name()`（hello 响应与发现信标在用），Linux 返回主机名、Windows 返回计算机名。

## Goals / Non-Goals

**Goals:**

- 启动窗口 1:2 竖向（默认 480×960、小屏等比缩小）在 Linux 本机可实跑验证
- 短名默认初始化只在 GUI 启动路径发生，可单测、不波及 CLI 与运行中会话

**Non-Goals:**

- 运行中锁定窗口比例（启动后用户可自由调整）
- 面板新增「设备名」展示行（短名行已可见初始化结果）
- 自动填入时的一次性界面提示
- 短名的其它自动来源或命名改写规则

## Decisions

### D1 短名初始化落点：独立桥接函数，由 Dart `main()` 在启动路径调用一次

核心 `config::ensure_default_short_name(data_dir, device_name) -> Result<Option<String>>`（可直接单测）：短名为空（键缺失或空值）且设备名通过 `identity::validate_short_name` 时写回并返回 `Some(名)`；设备名不合法保持为空返回 `None`；已有非空短名原样返回、不写。桥接面新增 `api::device::ensure_default_short_name()`（设备名取 `sysinfo::System::host_name().unwrap_or_default()`），由 Dart `main()` 在 `runApp` 前调用一次——失败仅记日志、不阻断启动（配置不可用等真实错误由随后的 `app_init` 如实呈现）。

- 备选 A：并入 `app_init`——**否决（关键陷阱）**：清空按钮为刷新快照也调用 `app_init`，在其中补全会使「清空当次运行内生效」不成立（清空被当场填回）。
- 备选 B：进程内 `OnceLock` 只补全一次——否决：同一进程内多次初始化的场景（桥接/集成测试）补全与否取决于调用顺序，测试顺序敏感、语义隐晦。
- 备选 C：Dart 侧用既有 API 组合（`init` + `setShortName` + `Platform.localHostname`）——否决：会把 `app_init`（含提权重启）提前到窗口显示之前、改变启动时序；逻辑也失去 Rust 单测覆盖。
- 备选 D：引入「已清空」标记键以区分从未设置——否决：用户已确认「只要为空就填回」，无需区分。

### D2 设备名校验复用短名规则，不截断、不替换字符

`identity::validate_short_name` 同时完成去空白与规则校验；失败（空、>32 字符、含空白/控制字符）即保持未设置——不截断、不替换字符，避免生成用户未预期的名字。Linux 主机名含点号等按既有短名规则本属合法，不额外收紧。

### D3 窗口尺寸：可单测纯函数 + screen_retriever 读屏幕可用区域

- 新增 `app/lib/src/window_geometry.dart`：`kDefaultStartupWindowSize = Size(480, 960)`、`kMinimumWindowSize = Size(360, 720)`、`Size startupWindowSize(Size visible)`——高取 `min(960, visible.height)`；宽度不足（`高/2 > visible.width`）时按宽度再缩；返回 `Size(高/2, 高)`，恒为 1:2。
- `main.dart`：`screenRetriever.getPrimaryDisplay()` 取 `visibleSize ?? size`；读取失败回退默认 480×960（不阻断启动）；`WindowOptions(size: …, minimumSize: kMinimumWindowSize, center: true)`，运行中不设比例约束。
- `pubspec.yaml` 增 `screen_retriever: ^0.2.2` 直接依赖（与锁文件现值一致，不升级）。
- 备选：固定 480×960——用户已选小屏自适应；复用 `window_manager` 内部 `calc_window_position` 工具——未导出，不可用。

### D4 验证方式

- Rust：`config_test.rs` 增用例（为空则填设备名并持久化 / 不覆盖已有非空 / 非法设备名保持为空不写 / 清空后再次调用填回 / 幂等），`cargo test` 全绿。
- Dart：`window_geometry_test.dart` 纯函数用例（常规屏 / 小屏 / 极窄屏 / 恰为默认，恒 1:2）；`flutter analyze && flutter test`。
- 集成（Linux 本机，Xvfb）：① 启动后 `windowManager.getSize()` 断言宽:高 = 1:2 且高 = min(960, 屏幕可用高)；② 显式调用 `ensureDefaultShortName()`：清空后调用被填回（= 本机主机名）、已设置时不覆盖；现有「清空后同会话保持为空」断言继续成立（`app_init` 不含补全）。
- Windows 真机（窗口表现、短名初始化）列入「待用户验收清单」。

## Risks / Trade-offs

- [屏幕可用区域读取失败（后端异常等）] → 回退默认 480×960，不阻断启动
- [既有安装升级后首次启动被自动填名] → 即本次需求语义（此前为空者获得设备名）；规格与文档写明
- [清空后重启填回不符个别用户预期] → 用户确认的语义；规格显式写明「清空仅在当次运行内生效」
- [Linux 主机名超 32 字符或含空白] → 保持未设置，面板显示「未设置」，可手动设置
- [某平台后端不提供 visibleSize] → `visibleSize ?? size` 回退整屏尺寸（仍保持 1:2）

## Migration Plan

无部署态迁移：短名初始化是启动时幂等补全，窗口尺寸仅影响启动表现。升级后首次启动即生效；回滚即回退提交。

## Open Questions

- 自动填入时是否给一次性界面提示（现不做，面板短名行即可见结果）——需要时另立
