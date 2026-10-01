// 端到端冒烟：真实桌面环境 + 真实 Rust 库 + 真实配置文件。
//
// 运行（需可用的 DISPLAY 与隔离的 XDG_CONFIG_HOME）：
//   XDG_CONFIG_HOME=<临时目录> flutter test integration_test -d linux

import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:agent_bridge_app/main.dart';
import 'package:agent_bridge_app/src/bridge_service.dart';
import 'package:agent_bridge_app/src/rust/frb_generated.dart';

/// 轮询等待某个 Finder 命中（真实异步下等待 Rust 调用完成与界面刷新）。
Future<void> pumpUntilFound(
  WidgetTester tester,
  Finder finder, {
  Duration timeout = const Duration(seconds: 15),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (!tester.any(finder)) {
    if (DateTime.now().isAfter(deadline)) {
      throw TestFailure('等待超时：$finder');
    }
    await tester.pump(const Duration(milliseconds: 100));
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('端到端：初始化快照 + UI 设置/清空短名并持久化', (tester) async {
    await RustLib.init();

    // 1) 服务级：初始化返回本机快照
    final snapshot = await RustBridgeService().init();
    expect(snapshot.uuid, isNotEmpty);
    expect(snapshot.version, isNotEmpty);
    expect(snapshot.system.platform, isNotEmpty);

    // 2) UI 级：启动真实界面，面板展示 UUID
    await tester.pumpWidget(AgentBridgeApp(service: RustBridgeService()));
    await tester.pumpAndSettle();
    expect(find.text(snapshot.uuid), findsOneWidget);

    final xdg = Platform.environment['XDG_CONFIG_HOME'];
    expect(xdg, isNotNull, reason: '测试须以隔离的 XDG_CONFIG_HOME 运行');
    final configFile = File('$xdg/agent-bridge/config.toml');

    // 3) 设置短名：点击输入框聚焦后输入（真实窗口下需先点击以建立文本输入连接），
    //    UI 保存 → 文件落地 → 重新初始化可读回（重启语义）
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '端到端-A');
    await tester.tap(find.text('保存'));
    await pumpUntilFound(tester, find.text('短名已保存'));
    expect(configFile.existsSync(), isTrue);
    expect(configFile.readAsStringSync(), contains('端到端-A'));
    expect((await RustBridgeService().init()).shortName, '端到端-A');

    // 4) 非法短名被拒：真实 Rust 校验、错误文案可见、原值不变
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '含 空格');
    await tester.tap(find.text('保存'));
    await pumpUntilFound(tester, find.textContaining('短名无效'));
    expect(configFile.readAsStringSync(), contains('端到端-A'), reason: '被拒后原值不变');
    expect((await RustBridgeService().init()).shortName, '端到端-A');

    // 5) 清空：键移除并持久
    await tester.tap(find.text('清空'));
    await pumpUntilFound(tester, find.text('短名已清空'));
    expect(configFile.readAsStringSync(), isNot(contains('short_name')));
    expect((await RustBridgeService().init()).shortName, isNull);
  });
}
