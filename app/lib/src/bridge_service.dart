import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'rust/api/device.dart' as rust_device;
import 'rust/api/init.dart' as rust_init;
import 'rust/api/init.dart' show AppSnapshot;
import 'rust/sysinfo_view.dart' show SystemSnapshot;

/// 桥接服务抽象：UI 只依赖本接口，widget 测试注入假实现、不依赖真 Rust 库
/// （design D4/D10）。
abstract class BridgeService {
  /// 读取启动快照（解析数据目录、读取/创建配置、采集系统信息）。
  Future<AppSnapshot> init();

  /// 设置（[name] 非空）或清空（[name] 为 null）本机短名，返回刷新后的快照。
  Future<AppSnapshot> setShortName(String? name);

  /// 仅刷新系统信息。
  Future<SystemSnapshot> refreshSystem();
}

/// 真实实现：走 flutter_rust_bridge 生成的 API。
class RustBridgeService implements BridgeService {
  @override
  Future<AppSnapshot> init() => rust_init.appInit();

  @override
  Future<AppSnapshot> setShortName(String? name) =>
      rust_device.setShortName(name: name);

  @override
  Future<SystemSnapshot> refreshSystem() => rust_init.refreshSystem();
}

/// 从异常中提取面向用户的错误消息。
///
/// Rust 侧经 anyhow 返回的错误在 Dart 侧表现为 [AnyhowException]（`message`
/// 即 Rust 错误的完整文案）；其余异常回退到 `toString()`。
String readableError(Object error) {
  if (error is AnyhowException) {
    return error.message;
  }
  return error.toString();
}
