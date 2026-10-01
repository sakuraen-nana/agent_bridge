// 端到端冒烟：在真实桌面环境加载 Rust 库并调用桥接面（需 `flutter test integration_test`）。

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:agent_bridge_app/src/bridge_service.dart';
import 'package:agent_bridge_app/src/rust/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('Rust 桥接可用：初始化返回本机快照', (tester) async {
    await RustLib.init();
    final snapshot = await RustBridgeService().init();
    expect(snapshot.uuid, isNotEmpty);
    expect(snapshot.version, isNotEmpty);
    expect(snapshot.system.platform, isNotEmpty);
  });
}
