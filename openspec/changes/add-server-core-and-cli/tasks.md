# Tasks: add-server-core-and-cli

> 实施顺序：依赖与骨架（1）→ 认证与双 token（2）→ API 三件套（3）→ 配置 v2 与寻址（4）→
> CLI（5）→ GUI 接线（6）→ 测试回归（7）→ Linux 实跑（8）→ 文档与收尾（9）。
> 决策依据见 design.md（D1–D12）；行为范围以差异规格为准，不做规格外实现
> （托盘/提权/防火墙/发现配对/打包均属后续变更）。Windows 侧列入 §10 待用户验收。

## 1. 依赖与服务端骨架

- [ ] 1.1 `cargo add` 引入 axum / tokio（rt-multi-thread 等特性）/ futures-util / tokio-util / subtle / getrandom / encoding_rs，并加 `[[bin]] name = "agent-bridge"` 空壳；验证：`cargo build` 成功、锁文件更新
- [ ] 1.2 `server` 模块骨架：`ServerHandle`（独立 runtime、`stop()`）、`ServerConfig`（含端口注入）、路由骨架与 `GET /` 404 行为；验证：单测——进程内起服务于临时端口、未知路径 404、`stop()` 后端口释放
- [ ] 1.3 端口被占用场景：注入已被占用的端口 → `start()` 返回可读错误、不换端口；验证：单测（先占端口再起服务）

## 2. 认证与双 token

- [ ] 2.1 token 生成（getrandom 32B → hex）与会话 token 进程级生成；长期 token 配置补全（缺则生成写回，不算损坏）；验证：单测——格式、长度（64 hex）、补全后文件含新键且原注释保留
- [ ] 2.2 认证中间件：`?token=` 恒时比较（subtle），接受会话或长期任一；失败/缺失统一 404 且响应不区分原因；验证：单测/集成——有效（两类 token 各一）、无效、缺失共 4 类请求的响应一致性与通过性
- [ ] 2.3 `token reset`：写回配置 + 内存镜像更新，旧值立即失效；验证：集成测试——重置后旧 token 404、新 token 通过（不重启服务）

## 3. API：hello / exec / download（含留痕）

- [ ] 3.1 `POST /hello`：返回规格字段（版本/UUID/短名/主机名/用户/系统/平台/workdir/局域网 IP/启动时刻）；验证：集成测试逐字段断言（含短名未设置时 null）
- [ ] 3.2 `POST /exec`：NDJSON 流式（output/exit 形态与 Python 版一致）、cwd/timeout 语义、UTF-8 解码回退；验证：集成测试——流式多段输出、相对 cwd、缺省 timeout 标注
- [ ] 3.3 exec 终止语义：超时终止进程树（含派生后台进程）、客户端断开终止；验证：集成测试——`sleep` 派生后台进程后超时，验证进程消失；断开连接后同断言
- [ ] 3.4 `POST /download`：octet-stream + Content-Length、错误 JSON 区分「不存在 / 是目录」；验证：集成测试——字节一致、两类错误
- [ ] 3.5 请求留痕：`server.log` 记录（含被拒请求与脱敏状态；exec 不记输出只记命令与结束状态；download 不记内容）；1 MiB 轮转 `server.log.1`；验证：集成测试——写入断言 + 缩容轮转（注入小阈值或写满）核对；`server.log` 全文不含任何 token 取值

## 4. 配置 v2 与短名寻址

- [ ] 4.1 `[[peer]]` 读取（缺省 port=37777、非法条目跳过不改写文件）与 `workdir`（缺省用户主目录）读取；验证：单测——多段解析、容错条目、workdir 缺省
- [ ] 4.2 `resolve_peer`：UUID 直取第一条；短名比较键唯一命中；0 命中 Unknown；≥2 命中 Conflict（含各 UUID）；逐键独立；验证：单测覆盖全部分支（含大小写不敏感与冲突无效化）
- [ ] 4.3 配置 v1→v2 升级写回回归：旧文件加载后补全，注释/未知键保留；验证：单测沿用变更 ① 夹具 + 新增断言

## 5. 设备 CLI

- [ ] 5.1 bin 骨架与 clap 子命令（peers / hello / exec / download / token show|reset）+ 退出码约定的公共封装；验证：`agent-bridge --help`、未知子命令退出码 2
- [ ] 5.2 `token show|reset`（本地配置操作，不联网）；验证：集成测试——show 打印与配置一致；reset 后旧值 404（与 2.3 联动）
- [ ] 5.3 `peers`：列出条目、对冲突键标记无效并提示改用 UUID/改名；验证：集成测试——冲突夹具下输出断言
- [ ] 5.4 `hello/exec/download <设备>`：短名与 UUID 寻址、流式转发、退出码透传（0–255）、404→4、网络失败→3、路径错误→1；验证：集成测试（`CARGO_BIN_EXE_agent-bridge` 对进程内服务端）覆盖各分支与退出码
- [ ] 5.5 短名冲突下用 UUID 绕过：验证：集成测试——冲突夹具中 UUID 路径全子命令可用

## 6. GUI 最小接线

- [ ] 6.1 `app_init` 启动服务端；`AppSnapshot.server`（running/port/error）入快照；frb codegen 重跑并提交生成物；验证：`cargo test` + `flutter analyze`
- [ ] 6.2 `home_page.dart`：服务端错误 banner、正常时面板显示「服务端：运行中（端口 37777）」；验证：widget 测试新增两用例
- [ ] 6.3 集成测试（真实 UI）增补：应用启动后服务端可用（以会话 token 或长期 token 直接调用 hello 成功）与端口占用时 banner 可见（构造：先占端口再启用例）；验证：`flutter test integration_test -d linux`

## 7. 测试回归

- [ ] 7.1 Rust 全量测试；验证：`cargo test` 退出码 0、用例总数不少于变更前（16）
- [ ] 7.2 Flutter 全量；验证：`flutter analyze` 零问题、`flutter test` 全绿、`flutter test integration_test -d linux` 全绿
- [ ] 7.3 Python 版回归；验证：`python3 -m unittest discover -s tests` 全绿（69 项）

## 8. Linux 实跑验证（本机）

- [ ] 8.1 起 GUI（Xvfb + 独立 dbus）后：以 CLI 对 127.0.0.1 全子命令实跑（hello / exec 流式与退出码透传 / download 校验哈希 / peers / token show）；留证
- [ ] 8.2 配置 v2 实跑：手工写入 `[[peer]]`（含一对冲突短名）→ `peers` 标记冲突、短名寻址被拒并给出 UUID、UUID 寻址可用；留证
- [ ] 8.3 端口占用实跑：预先占用 37777 启动 GUI → 界面 banner 提示、CLI 不可达（网络错误退出码 3）；留证
- [ ] 8.4 日志核对：`server.log` 含各请求留痕、无任何 token 取值、exec 输出不入日志；权限核对；留证
- [ ] 8.5 跨实现互操作抽查（非承诺）：Python 客户端（token 文档指向本机 + 长期 token）对 Rust 服务端跑 hello / exec / download；留证
- [ ] 8.6 会话 token 轮换实跑：重启 GUI 后旧会话 token 404、新会话 token 可用、长期 token 仍可用；留证

## 9. 文档与收尾

- [ ] 9.1 `README.md`：桌面应用章节增补 CLI 用法（子命令、设备寻址、退出码表）与协议要点（双 token、日志位置）；验证：按 README 步骤可复现 CLI 调用
- [ ] 9.2 分提交推送（服务端核心 / 配置与寻址 / CLI / GUI 接线 / 文档各自成提交）；验证：`git status` 干净、与远端一致
- [ ] 9.3 证据登记：勾选附证据（提交哈希 / 命令 / 退出码）；未实跑不勾选
- [ ] 9.4 归档前版本推进：`0.1.0` → `0.2.0`（「归档即 bump」）；验证：`/hello` 与面板版本一致、提交推送

## 10. 待用户验收清单（需在 Windows 机器上人工操作）

- [ ] 10.1 构建并在 Windows 运行 GUI；**预期**：服务端启动、面板显示「运行中（端口 37777）」；防火墙放行提示按系统弹窗处理（本变更不含自动放行——属变更 ③）
- [ ] 10.2 以 CLI 对 127.0.0.1 实跑 hello / exec（含中文输出）/ download；**预期**：输出正确、退出码透传
- [ ] 10.3 exec 超时与断开；**预期**：超时后 `timed_out` 收尾且派生进程被终止（`tasklist` 核对）
- [ ] 10.4 手工制造端口占用后启动；**预期**：界面 banner 提示、不换端口

## 11. 跟进项（本变更不实现，记录于此）

- [ ] 11.1 GUI 的 peer 增删改界面与冲突高亮 —— 变更 ③/④ 评估
- [ ] 11.2 CLI 的 `--json` 输出模式（脚本化消费）—— 需要时另立
- [ ] 11.3 日志的界面查看器 —— 变更 ③ 评估
