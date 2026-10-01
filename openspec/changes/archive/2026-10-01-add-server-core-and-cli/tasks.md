# Tasks: add-server-core-and-cli

> 实施顺序：依赖与骨架（1）→ 认证与双 token（2）→ API 三件套（3）→ 配置 v2 与寻址（4）→
> CLI（5）→ GUI 接线（6）→ 测试回归（7）→ Linux 实跑（8）→ 文档与收尾（9）。
> 决策依据见 design.md（D1–D12）；行为范围以差异规格为准，不做规格外实现
> （托盘/提权/防火墙/发现配对/打包均属后续变更）。Windows 侧列入 §10 待用户验收
> （归档时保留未勾选）。实施环境（点态）：Linux 开发机 + Xvfb 虚拟显示 +
> 独立 dbus 会话；依赖走国内镜像（镜像选择不写入仓库）。

## 1. 依赖与服务端骨架

- [x] 1.1 `cargo add` 引入 axum / tokio（rt-multi-thread 等特性）/ futures-util / tokio-util / subtle / getrandom / encoding_rs，并加 `[[bin]] name = "agent-bridge"` 空壳；验证：`cargo build` 成功、锁文件更新 —— 证据：提交 `3f4ae5c`；实际版本 axum 0.8.9、tokio 1.53.1、tokio-util 0.7.19（io）、subtle 2.6.1、getrandom 0.4.3、encoding_rs 0.8.42、clap 4.6.7、reqwest 0.13.5（禁默认 TLS）、serde/serde_json、libc；bin 以 `src/bin/agent-bridge.rs` 自动发现（等效 [[bin]]），`cargo build` 产出 `target/debug/agent-bridge`（85 MB debug）
- [x] 1.2 `server` 模块骨架：`ServerHandle`（独立 runtime、`stop()`）、`ServerConfig`（含端口注入）、路由骨架与 `GET /` 404 行为；验证：单测——进程内起服务于临时端口、未知路径 404、`stop()` 后端口释放 —— 证据：`tests/server_test.rs`（12 项）含 `port_in_use_fails_fast_and_stop_releases_port`（stop 后重绑成功）；端口注入用 0（系统分配），实跑另用 `AGENT_BRIDGE_PORT`
- [x] 1.3 端口被占用场景：注入已被占用的端口 → `start()` 返回可读错误、不换端口；验证：单测（先占端口再起服务）—— 证据：单测同上（`Err(AppError::PortInUse)`）；实跑：占用 37777 启动 GUI → 界面红色横幅「端口 37777 已被占用（不自动更换端口）：地址已在使用 (os error 98)」+ 服务端行「未运行」（截图留证）；CLI 对其 hello → 退出码 3

## 2. 认证与双 token

- [x] 2.1 token 生成（getrandom 32B → hex）与会话 token 进程级生成；长期 token 配置补全（缺则生成写回，不算损坏）；验证：单测——格式、长度（64 hex）、补全后文件含新键且原注释保留 —— 证据：`config_test.rs::long_term_token_created_persisted_and_repaired`（64 位十六进制、跨加载沿用、删键后补全且无备份文件）
- [x] 2.2 认证中间件：`?token=` 恒时比较（subtle），接受会话或长期任一；失败/缺失统一 404 且响应不区分原因；验证：单测/集成——有效（两类 token 各一）、无效、缺失共 4 类请求的响应一致性与通过性 —— 证据：`server_test.rs::auth_accepts_session_and_long_term_and_rejects_else`（缺失/错误 → 404；会话/长期 → 200）
- [x] 2.3 `token reset`：写回配置 + 内存镜像更新，旧值立即失效；验证：集成测试——重置后旧 token 404、新 token 通过（不重启服务）—— 证据：`server_test.rs::long_term_token_reset_takes_effect_without_restart`（长驻进程内，按配置文件 mtime 刷新缓存，实现路径较 design D3 的 RwLock 更优——跨进程 CLI 重置亦即时生效）；CLI 侧 `token_show_and_reset`

## 3. API：hello / exec / download（含留痕）

- [x] 3.1 `POST /hello`：返回规格字段（版本/UUID/短名/主机名/用户/系统/平台/workdir/局域网 IP/启动时刻）；验证：集成测试逐字段断言（含短名未设置时 null）—— 证据：`server_test.rs::hello_fields_match_config`；实跑 CLI hello 输出全部字段、cwd=/root（缺省用户主目录）
- [x] 3.2 `POST /exec`：NDJSON 流式（output/exit 形态与 Python 版一致）、cwd/timeout 语义、UTF-8 解码回退；验证：集成测试——流式多段输出、相对 cwd、缺省 timeout 标注 —— 证据：`server_test.rs::exec_streams_merged_output_and_exit_code`（中文 stdout + 合并 stderr + exit 3 + duration_ms + 非超时不带 timed_out）；实跑 `echo 中文输出-实跑; exit 5` 退出码 5
- [x] 3.3 exec 终止语义：超时终止进程树（含派生后台进程）、客户端断开终止；验证：集成测试——`sleep` 派生后台进程后超时，验证进程消失；断开连接后同断言 —— 证据：`exec_timeout_kills_process_tree`、`exec_client_disconnect_kills_process_tree`（**实施修复**：静默命令下断开无 send 失败可触发，改为 NDJSON 流 Drop 时 `kill_tree_blocking` 兜底，两者并行；Unix 经进程组 `killpg`）
- [x] 3.4 `POST /download`：octet-stream + Content-Length、错误 JSON 区分「不存在 / 是目录」；验证：集成测试——字节一致、两类错误 —— 证据：`server_test.rs::download_streams_bytes_and_reports_clear_errors`（字节逐一致、404 不存在、400 是目录）；实跑 sha256 一致（100000 字节）
- [x] 3.5 请求留痕：`server.log` 记录（含被拒请求与脱敏状态；exec 不记输出只记命令与结束状态；download 不记内容）；1 MiB 轮转 `server.log.1`；验证：集成测试——写入断言 + 缩容轮转（注入小阈值或写满）核对；`server.log` 全文不含任何 token 取值 —— 证据：`log_records_requests_without_token_values_or_output`（被拒留痕 token=缺失；唯一输出串出现恰 1 次=命令行本身）、`log_rotates_at_size_threshold`（阈值 256B 促发轮转）；实跑核对：长期 token 出现 0 次、exec 输出串仅出现在 command 行（日志仅记「输出 20 字节」）、download 记路径与字节数

## 4. 配置 v2 与短名寻址

- [x] 4.1 `[[peer]]` 读取（缺省 port=37777、非法条目跳过不改写文件）与 `workdir`（缺省用户主目录）读取；验证：单测——多段解析、容错条目、workdir 缺省 —— 证据：`config_test.rs::peers_parsed_with_tolerance_and_file_untouched`（缺 uuid 条目跳过、port 缺省/显式、文件逐字节未变）、`workdir_read_and_default_none`
- [x] 4.2 `resolve_peer`：UUID 直取第一条；短名比较键唯一命中；0 命中 Unknown；≥2 命中 Conflict（含各 UUID）；逐键独立；验证：单测覆盖全部分支（含大小写不敏感与冲突无效化）—— 证据：`peers_test.rs` 5 项（唯一命中大小写不敏感、UUID 恒可用、冲突无效化含各 UUID、逐键独立、空引用 Unknown）
- [x] 4.3 配置 v1→v2 升级写回回归：旧文件加载后补全，注释/未知键保留；验证：单测沿用变更 ① 夹具 + 新增断言 —— 证据：`manual_edit_takes_effect_and_unknown_keys_survive_write`（含 v1 文件补全长期 token 与写入后注释/未知段保留）、`config_file_kept_intact_by_reads`（合法配置读取零改写）

## 5. 设备 CLI

- [x] 5.1 bin 骨架与 clap 子命令（peers / hello / exec / download / token show|reset）+ 退出码约定的公共封装；验证：`agent-bridge --help`、未知子命令退出码 2 —— 证据：cli_test 9 项全绿；`--help`/`--version` 走 stdout（0）、用法错误走 stderr（2）的判定在 `cli::run`
- [x] 5.2 `token show|reset`（本地配置操作，不联网）；验证：集成测试——show 打印与配置一致；reset 后旧值 404（与 2.3 联动）—— 证据：`cli_test.rs::token_show_and_reset`（show=配置值、reset 后新值 64 hex、再次 show 一致）；2.3 的 404 断言在 server_test
- [x] 5.3 `peers`：列出条目、对冲突键标记无效并提示改用 UUID/改名；验证：集成测试——冲突夹具下输出断言 —— 证据：`cli_test.rs::short_name_conflict_invalidates_all_and_uuid_bypasses`（同名两行均标记「短名冲突，无效」）+ 实跑输出一致
- [x] 5.4 `hello/exec/download <设备>`：短名与 UUID 寻址、流式转发、退出码透传（0–255）、404→4、网络失败→3、路径错误→1；验证：集成测试（`CARGO_BIN_EXE_agent-bridge` 对进程内服务端）覆盖各分支与退出码 —— 证据：`hello_by_short_name_and_uuid`、`exec_exit_code_passthrough_and_streaming`（exit 7）、`download_default_name_and_out`、`business_error_maps_to_exit_1`（路径不存在）、`network_error_maps_to_exit_3`、`token_rejected_exit_code_and_hint`（4）、`unknown_device_and_missing_token_are_config_errors`（2）；实跑网络失败一例（黑洞端口）退出码 3、错误文案经脱敏（`?token=***`，单测锁定）
- [x] 5.5 短名冲突下用 UUID 绕过：验证：集成测试——冲突夹具中 UUID 路径全子命令可用 —— 证据：`cli_test.rs::short_name_conflict_...`（UUID hello 通过）；实跑：冲突下短名被拒 exit 2（报错含两 UUID）→ UUID 指代 hello exit 0

## 6. GUI 最小接线

- [x] 6.1 `app_init` 启动服务端；`AppSnapshot.server`（running/port/error）入快照；frb codegen 重跑并提交生成物；验证：`cargo test` + `flutter analyze` —— 证据：提交 `3f4ae5c`（Rust 侧含桥接面与重跑生成物）；`ensure_server` 幂等（同进程单实例，经 `OnceLock`）
- [x] 6.2 `home_page.dart`：服务端错误 banner、正常时面板显示「服务端：运行中（端口 37777）」；验证：widget 测试新增两用例 —— 证据：提交 `17d0de1`；widget 测试 9 项全绿（新增「服务端运行中展示端口」「服务端启动失败展示错误横幅与未运行」）
- [x] 6.3 集成测试（真实 UI）增补：应用启动后服务端可用（以会话 token 或长期 token 直接调用 hello 成功）与端口占用时 banner 可见（构造：先占端口再启用例）；验证：`flutter test integration_test -d linux` —— 证据：两场分跑（AGENT_BRIDGE_PORT 分别 39401/39402）：`端到端：初始化快照、服务端可用与短名设置/清空` 与 `端到端：端口被占用时界面提示服务端未运行` 均 All tests passed（Xvfb + 独立 dbus + 隔离 XDG）

## 7. 测试回归

- [x] 7.1 Rust 全量测试；验证：`cargo test` 退出码 0、用例总数不少于变更前（16）—— 证据：46 项全绿（cli 9 / config 14 / identity 5 / peers 5 / server 12 / sysinfo 1），退出码 0
- [x] 7.2 Flutter 全量；验证：`flutter analyze` 零问题、`flutter test` 全绿、`flutter test integration_test -d linux` 全绿 —— 证据：analyze `No issues found`；`flutter test` +9 All passed；集成两场见 6.3
- [x] 7.3 Python 版回归；验证：`python3 -m unittest discover -s tests` 全绿（69 项）—— 证据：`Ran 69 tests · OK (skipped=2)`

## 8. Linux 实跑验证（本机）

- [x] 8.1 起 GUI（Xvfb + 独立 dbus）后：以 CLI 对 127.0.0.1 全子命令实跑（hello / exec 流式与退出码透传 / download 校验哈希 / peers / token show）；留证 —— 证据：hello（短名寻址）全字段输出；exec `exit 5` 退出码 5、中文输出原样；download 相对路径 `s8-test.bin` → sha256 与源一致；peers、token show 一致
- [x] 8.2 配置 v2 实跑：手工写入 `[[peer]]`（含一对冲突短名）→ `peers` 标记冲突、短名寻址被拒并给出 UUID、UUID 寻址可用；留证 —— 证据：`dev-a` 与 `DEV-A` 两行均标记「短名冲突，无效」；`hello dev-a` exit 2 且报错列出两个 UUID 与解除方式；`hello <uuid>` exit 0
- [x] 8.3 端口占用实跑：预先占用 37777 启动 GUI → 界面 banner 提示、CLI 不可达（网络错误退出码 3）；留证 —— 证据：横幅截图（见 1.3）；CLI `hello` exit 3、stderr 为连接失败（脱敏）
- [x] 8.4 日志核对：`server.log` 含各请求留痕、无任何 token 取值、exec 输出不入日志；权限核对；留证 —— 证据：13 行日志含请求行/参数/结束状态；长期 token 出现 0 次；exec 输出串仅出现 1 次（即 command 行本身），结束行仅记「输出 20 字节」；文件 0600（随数据目录惯例）
- [x] 8.5 跨实现互操作抽查（非承诺）：Python 客户端（token 文档指向本机 + 长期 token）对 Rust 服务端跑 hello / exec / download；留证 —— 证据：`python3 run.py hello/exec/download --host 127.0.0.1 --token <长期>` 三项均成功（download sha256 与源一致）——端点形态与事件流跨实现兼容
- [x] 8.6 会话 token 轮换实跑：重启 GUI 后旧会话 token 404、新会话 token 可用、长期 token 仍可用；留证 —— 证据：真实 GUI 重启后长期 token 仍可用（hello exit 0）；会话轮换的机制断言在 `server_test.rs::session_token_rotates_across_restart`（旧会话 404、新会话 200、长期 200；# 备注：会话 token 仅存内存、外部无从取得，故以进程内两代服务的断言为证）

## 9. 文档与收尾

- [x] 9.1 `README.md`：桌面应用章节增补 CLI 用法（子命令、设备寻址、退出码表）与协议要点（双 token、日志位置）；验证：按 README 步骤可复现 CLI 调用 —— 证据：本次更新（提交见 9.2）；命令与实跑逐条一致
- [x] 9.2 分提交推送（服务端核心 / 配置与寻址 / CLI / GUI 接线 / 文档各自成提交）；验证：`git status` 干净、与远端一致 —— 证据：`3f4ae5c`（Rust 服务端核心与协议 v2 及 CLI，含配置与寻址）、`17d0de1`（GUI 接线）、本文档提交（README + tasks 证据）；均已推送 `main -> main`
- [x] 9.3 证据登记：勾选附证据（提交哈希 / 命令 / 退出码）；未实跑不勾选 —— 证据：本次更新即登记；§10 Windows 三项如实保留未勾选
- [x] 9.4 归档前版本推进：`0.1.0` → `0.2.0`（「归档即 bump」）；验证：`/hello` 与面板版本一致、提交推送 —— 证据：提交随归档序列（`app/rust/Cargo.toml` 与 `app/pubspec.yaml` 同步 0.2.0；CLI `--version` 与面板/hello 一致）

## 10. 待用户验收清单（需在 Windows 机器上人工操作）

- [ ] 10.1 构建并在 Windows 运行 GUI；**预期**：服务端启动、面板显示「运行中（端口 37777）」；防火墙放行提示按系统弹窗处理（本变更不含自动放行——属变更 ③）
- [ ] 10.2 以 CLI 对 127.0.0.1 实跑 hello / exec（含中文输出）/ download；**预期**：输出正确、退出码透传
- [ ] 10.3 exec 超时与断开；**预期**：超时后 `timed_out` 收尾且派生进程被终止（`tasklist` 核对）
- [ ] 10.4 手工制造端口占用后启动；**预期**：界面 banner 提示、不换端口

> 以上四项在归档时保留未勾选（实施环境无 Windows 机器）。结果如与预期不符，另立修复变更处理。

## 11. 跟进项（本变更不实现，记录于此）

- [ ] 11.1 GUI 的 peer 增删改界面与冲突高亮 —— 变更 ③/④ 评估
- [ ] 11.2 CLI 的 `--json` 输出模式（脚本化消费）—— 需要时另立
- [ ] 11.3 日志的界面查看器 —— 变更 ③ 评估
