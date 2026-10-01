## Context

动机见 `proposal.md`；行为契约见差异规格。现状与约束：

- ③ 已归档：ServerHandle/独立 runtime、防火墙注入执行器、peers 寻址与冲突规则、GUI 区块模式、测试基建（Rust 69 / widget 14 / 集成 4 场）齐备。
- frb 桥接采集为「拉」模式（无事件流）——GUI 的配对弹窗与列表刷新用轮询实现（1 秒 / 5 秒），与既有刷新按钮同一路径。
- 本机双实例可端到端（不同数据目录/端口、同机 UDP 广播互见）。
- 安全边界敏感点：`/pair/request` 是本应用**唯一免 token 端点**。

## Goals / Non-Goals

**Goals:**

- 发现 / 配对（同意、拒绝、超时、忙、幂等重配）/ 在线三态在 Linux 双实例端到端可验
- 免 token 端点的暴露面最小化并写进规格与文档

**Non-Goals:**

- 跨子网发现（仅同广播域）、mDNS、IPv6 发现
- 配对记录的管理界面（删除/改名/历史）与自动信任 —— 后续评估
- 配对请求的显式取消按钮（发起方超时即放弃，对端决定后回执失败无害）

## Decisions

### D1 发现协议：UDP 37778，JSON 信标，定向广播兜底多网卡

- 信标：`{"proto":"agent-bridge/1","uuid","short_name","hostname","port"}`（≤512B，无凭据）。
- 发送：0.0.0.0 绑定的单 socket + `set_broadcast(true)`；除 `255.255.255.255` 外，对每个**非虚拟私网接口**按其 prefix 计算定向广播地址逐一发送（sysinfo 的 IpNetwork 已含 prefix）——多网卡覆盖，无新依赖；发送失败逐个忽略。
- 接收：同 socket 监听；本机 uuid 自播忽略；表为 `HashMap<uuid, Discovered{short_name, hostname, port, source_ip, last_seen}>`，30 秒过期（读取时惰性清理）。
- 周期 3 秒；任务随服务端 runtime 生命周期（`ServerHandle.stop` 自然结束）。
- UDP 绑定失败 → `discovery_unavailable: Some(原因)`，仅为状态，不阻断（规格 Scenario）。

### D2 配对服务端：单待决 + 长轮询 + 人工决定

- `ServerState` 增 `pairing: Mutex<Option<Pending>>`；`Pending { requester: {uuid, short_name, port, source_ip}, decided: oneshot::Sender<bool> }`。
- `POST /pair/request`（唯一免 token 端点）：
  - 已有待决 → `429 {"error":"已有待处理的配对请求"}`；
  - 否则入表，长轮询 `timeout(120s, rx)`；
  - 同意 → `{"approved":true,"config":{"uuid","short_name","port","token"（长期）}}`；拒绝/超时 → `{"approved":false,"reason":…}`；
  - 决定后（或超时）take 清表；请求方中途断开时决定仍可发生（写响应失败即结束，日志留痕）。
- 桥接面：`pairing_pending()`（GUI 1 秒轮询取待决摘要）/ `respond_pairing(approve)`（发决定；无待决报错）。
- 留痕：请求行 token 状态固定为「免认证端点（配对请求）」；决定与结果记明细行。

### D3 出站配对与配置写入

- `request_pairing(uuid)`：从发现表取 `source_ip`（无 → 「未发现该设备」）；`POST /pair/request`（总超时 125 秒）；同意 → `config::add_peer(data_dir, Peer{ uuid, short_name, address: source_ip, port, token })` → `paired`；被拒 / 超时 / 忙 / 不可达分别回执。
- `config::add_peer`（toml_edit）：`[[peer]]` 不存在则建；存在同 uuid 条目则**覆盖**其字段（幂等），否则 append；注释与未知键保留（既有口径）。
- 地址取 `source_ip`（发现来源）而非目标自报，防多网卡不可达。

### D4 在线探测

`peers_status()`：对全体 peer 并发 `/hello`（单机 2 秒超时，总超时 3 秒）：成功 → `online`；404 → `unauthorized`（凭据失效，提示重新配对）；连接失败/超时 → `offline`；短名冲突标记复用 peers 模块。

### D5 GUI 交互（轮询模式）

- 「发现设备」区块：1 秒定时器刷新 `discovered_devices()`（过滤已配对 uuid → 显示「已配对」禁用按钮）；行含 短名/主机名/UUID/来源 IP + 「发起配对」按钮（请求期间禁用并显示进行中；等待上限即桥接超时）。
- 待决配对对话框：1 秒轮询 `pairing_pending()`，有则 `AlertDialog`（请求方 UUID/短名/来源 IP + 同意/拒绝）；决定后本地去重避免重复弹。
- 「已配对设备」区块：5 秒定时器 `peers_status()`；三态用色点 + 文案。
- 定时器在 `dispose` 取消；widget 测试用 `tester.pump(Duration)` 驱动。

### D6 防火墙扩展：TCP 服务端口 + UDP 发现端口

`firewall::ensure(port, udp_discovery: bool, runner)`：ufw 逐条幂等放行 `tcp` 与 `udp 37778`；firewalld 同理（query/add-port）；Defender 规则增至两条（TCP 服务端口与 UDP 37778，规则名 `agent-bridge` 与 `agent-bridge-udp`）。未检出提示语含「所需端口」。单测更新全分支。

### D7 依赖与测试

- 无新增第三方依赖（tokio UdpSocket、reqwest、futures 均已在树）。
- 单测：信标编解码与自过滤、表过期、add_peer 幂等、配对状态机（注入/决定/忙/超时以缩短超时注入）、探测三态（MockRunner 式注入 HTTP？——探测走 reqwest，用进程内服务端实测）。
- 集成/实跑：双实例（不同 XDG/端口）互见 → 发起配对 → 对端（真实 GUI 或桥接决定）同意 → 自动写入 → CLI 连通；拒绝/超时/忙/凭据失效态。

## Risks / Trade-offs

- [长轮询悬挂拖慢 graceful shutdown] → ServerHandle::stop 已有 2 秒 join 超时 + shutdown_timeout 强杀；挂起配对被中断可接受（记录）
- [容器/受限网络里 255.255.255.255 发送失败] → 定向逐网段广播兜底；两者皆失败仅记日志不阻断
- [同机多实例互相广播导致的闪断条目] → uuid 过滤 + 过期窗口（30 秒）
- [免 token 端点被局域网内滥用（垃圾请求）] → 单待决 + 忙拒绝 + 人工同意前零信息；来源 IP 展示供判断；日志留痕
- [探测并发对大量 peer 的负载] → 2 秒单机超时 + 并发上限（现实现为全并发；peer 数量级为个位数，足够）
- [Windows UDP 放行需两条规则] → 见 D6；Windows 真机验证进验收清单

## Migration Plan

无部署态迁移。新增 UDP 监听与端点均为增量；旧配置（无 peer）照常。回滚即回退提交。

## Open Questions

- 配对记录的管理界面（增删/改名/历史）——下一变更或独立评估
- 跨子网发现（如多播/中继）——需要时另立
