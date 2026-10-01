// 端到端冒烟：真实桌面环境 + 真实 Rust 库 + 真实配置文件 + 真实 HTTP 服务端。
//
// 运行（需可用的 DISPLAY 与隔离的 XDG_CONFIG_HOME；AGENT_BRIDGE_PORT 指定测试端口）：
//   全流程：AGENT_BRIDGE_PORT=<port> XDG_CONFIG_HOME=<临时目录> \
//            flutter test integration_test -d linux --plain-name '端到端：初始化'
//   端口占用：AGENT_BRIDGE_PORT=<port> XDG_CONFIG_HOME=<临时目录> \
//            flutter test integration_test -d linux --plain-name '端口被占用'

import 'dart:convert';
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

  testWidgets('端到端：初始化快照、服务端可用与短名设置/清空', (tester) async {
    await RustLib.init();

    // 1) 服务级：初始化返回本机快照，服务端应已启动
    final snapshot = await RustBridgeService().init();
    expect(snapshot.uuid, isNotEmpty);
    expect(snapshot.version, isNotEmpty);
    expect(snapshot.system.platform, isNotEmpty);
    expect(snapshot.server.running, isTrue,
        reason: '服务端应随应用启动：${snapshot.server.error}');

    // 2) UI 级：启动真实界面，面板展示 UUID 与服务端运行状态
    await tester.pumpWidget(AgentBridgeApp(service: RustBridgeService()));
    await tester.pumpAndSettle();
    expect(find.text(snapshot.uuid), findsOneWidget);
    expect(find.textContaining('运行中（端口'), findsOneWidget);

    final xdg = Platform.environment['XDG_CONFIG_HOME'];
    expect(xdg, isNotNull, reason: '测试须以隔离的 XDG_CONFIG_HOME 运行');
    final configFile = File('$xdg/agent-bridge/config.toml');

    // 3) 服务端真实可用：以长期 token 直接调用 /hello
    final configText = configFile.readAsStringSync();
    final tokenMatch =
        RegExp(r'long_term_token = "([0-9a-f]{64})"').firstMatch(configText);
    expect(tokenMatch, isNotNull, reason: '配置应含长期 token');
    final client = HttpClient();
    try {
      final request = await client.postUrl(
        Uri.parse(
            'http://127.0.0.1:${snapshot.server.port}/hello?token=${tokenMatch!.group(1)}'),
      );
      final response = await request.close();
      expect(response.statusCode, 200);
      final body = await response.transform(utf8.decoder).join();
      expect(body, contains(snapshot.uuid));
    } finally {
      client.close(force: true);
    }

    // 4) 设置短名：点击输入框聚焦后输入（真实窗口下需先点击以建立文本输入连接），
    //    UI 保存 → 文件落地 → 重新初始化可读回（重启语义）
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '端到端-A');
    await tester.tap(find.text('保存'));
    await pumpUntilFound(tester, find.text('短名已保存'));
    expect(configFile.existsSync(), isTrue);
    expect(configFile.readAsStringSync(), contains('端到端-A'));
    expect((await RustBridgeService().init()).shortName, '端到端-A');

    // 5) 非法短名被拒：真实 Rust 校验、错误文案可见、原值不变
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '含 空格');
    await tester.tap(find.text('保存'));
    await pumpUntilFound(tester, find.textContaining('短名无效'));
    expect(configFile.readAsStringSync(), contains('端到端-A'), reason: '被拒后原值不变');
    expect((await RustBridgeService().init()).shortName, '端到端-A');

    // 6) 清空：键移除并持久
    await tester.tap(find.text('清空'));
    await pumpUntilFound(tester, find.text('短名已清空'));
    expect(configFile.readAsStringSync(), isNot(contains('short_name')));
    expect((await RustBridgeService().init()).shortName, isNull);
  });

  testWidgets('端到端：端口被占用时界面提示服务端未运行', (tester) async {
    await RustLib.init();
    final portText = Platform.environment['AGENT_BRIDGE_PORT'];
    expect(portText, isNotNull, reason: '测试须以 AGENT_BRIDGE_PORT 指定端口');
    final port = int.parse(portText!);

    // 先占用目标端口，再初始化应用：服务端应启动失败、不换端口
    final squatter =
        await ServerSocket.bind(InternetAddress.anyIPv4, port, shared: false);
    try {
      final snapshot = await RustBridgeService().init();
      expect(snapshot.server.running, isFalse);
      expect(snapshot.server.error, isNotNull);

      await tester.pumpWidget(AgentBridgeApp(service: RustBridgeService()));
      await tester.pumpAndSettle();
      expect(find.textContaining('服务端未运行'), findsOneWidget);
      expect(find.textContaining('已被占用'), findsOneWidget);
      expect(find.text('未运行'), findsOneWidget);
    } finally {
      await squatter.close();
    }
  });
}
