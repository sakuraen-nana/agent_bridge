import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'rust/api/device.dart' as rust_device;
import 'rust/api/init.dart' as rust_init;
import 'rust/api/init.dart' show AppSnapshot, AutostartInfo;
import 'rust/api/pair.dart' as rust_pair;
import 'rust/api/pair.dart'
    show DiscoveredDeviceInfo, PairOutcomeInfo, PeerStatusInfo, PendingPairingInfo;
import 'rust/sysinfo_view.dart' show SystemSnapshot;

/// 桥接服务抽象：UI 只依赖本接口，widget 测试注入假实现、不依赖真 Rust 库
/// （design D4/D10）。
abstract class BridgeService {
  /// 读取启动快照（解析数据目录、读取/创建配置、启动服务端、检测防火墙）。
  Future<AppSnapshot> init();

  /// 设置（[name] 非空）或清空（[name] 为 null）本机短名，返回刷新后的快照。
  Future<AppSnapshot> setShortName(String? name);

  /// 启动补全：本机短名为空时以本机设备名（主机名）初始化；返回补全后生效的短名。
  ///
  /// 仅由应用启动路径（`main()`）调用一次——不得并入 [init]：清空按钮为刷新
  /// 快照也走 [setShortName]→`app_init`，并入会使清空被当场填回。
  Future<String?> ensureDefaultShortName();

  /// 仅刷新系统信息。
  Future<SystemSnapshot> refreshSystem();

  /// 生成「本机配置」TOML 片段（含本次会话短期 token；供剪贴板）。
  Future<String> sharePayload();

  /// 查询开机自启状态。
  Future<AutostartInfo> autostartStatus();

  /// 设置开机自启（幂等）。
  Future<AutostartInfo> setAutostart(bool enabled);

  /// 托盘宿主是否可用（不可用时关闭窗口即退出）。
  Future<bool> trayHostAvailable();

  /// 发现到的设备列表（含已配对与冲突标记）。
  Future<List<DiscoveredDeviceInfo>> discoveredDevices();

  /// 当前待决配对请求（无则 null）。
  Future<PendingPairingInfo?> pairingPending();

  /// 对当前待决请求做出决定（同意 / 拒绝）。
  Future<void> respondPairing(bool approve);

  /// 向发现到的设备发起配对；同意后自动写入 [[peer]]。
  Future<PairOutcomeInfo> requestPairing(String uuid);

  /// 已配对设备的在线状态。
  Future<List<PeerStatusInfo>> peersStatus();
}

/// 真实实现：走 flutter_rust_bridge 生成的 API。
class RustBridgeService implements BridgeService {
  @override
  Future<AppSnapshot> init() => rust_init.appInit();

  @override
  Future<AppSnapshot> setShortName(String? name) =>
      rust_device.setShortName(name: name);

  @override
  Future<String?> ensureDefaultShortName() =>
      rust_device.ensureDefaultShortName();

  @override
  Future<SystemSnapshot> refreshSystem() => rust_init.refreshSystem();

  @override
  Future<String> sharePayload() => rust_init.sharePayload();

  @override
  Future<AutostartInfo> autostartStatus() => rust_init.autostartStatus();

  @override
  Future<AutostartInfo> setAutostart(bool enabled) =>
      rust_init.setAutostart(enabled: enabled);

  @override
  Future<bool> trayHostAvailable() => rust_init.trayHostAvailable();

  @override
  Future<List<DiscoveredDeviceInfo>> discoveredDevices() =>
      rust_pair.discoveredDevices();

  @override
  Future<PendingPairingInfo?> pairingPending() => rust_pair.pairingPending();

  @override
  Future<void> respondPairing(bool approve) =>
      rust_pair.respondPairing(approve: approve);

  @override
  Future<PairOutcomeInfo> requestPairing(String uuid) =>
      rust_pair.requestPairing(uuid: uuid);

  @override
  Future<List<PeerStatusInfo>> peersStatus() => rust_pair.peersStatus();
}

/// 从异常中提取面向用户的错误消息。
///
/// Rust 侧经 anyhow 返回的错误在 Dart 侧表现为 [AnyhowException]；其余异常
/// 回退到 `toString()`。frb 在应用启动时默认开启 `RUST_BACKTRACE`，anyhow
/// 错误的 Debug 形态因此可能在正文后附带「Stack backtrace:」段——展示前截掉，
/// 只保留人类可读文案。
String readableError(Object error) {
  if (error is AnyhowException) {
    return _cleanAnyhowMessage(error.message);
  }
  return error.toString();
}

String _cleanAnyhowMessage(String message) {
  final backtraceAt = message.indexOf('Stack backtrace:');
  final cleaned = backtraceAt >= 0 ? message.substring(0, backtraceAt) : message;
  return cleaned.trim();
}
