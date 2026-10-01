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
import 'package:flutter/services.dart';
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

/// 滚动列表直至目标可见（ListView 懒构建，屏外子项尚未构建时 finder 为空）。
Future<void> scrollTo(WidgetTester tester, Finder finder) async {
  await tester.scrollUntilVisible(
    finder,
    220,
    scrollable: find.byType(Scrollable).first,
    maxScrolls: 40,
  );
  await tester.pumpAndSettle();
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
    // 权限与防火墙状态行（取值随环境变化，只断言有可读说明）
    expect(find.text('管理员权限'), findsOneWidget);
    expect(snapshot.elevation.detail, isNotEmpty);
    expect(find.text('防火墙'), findsOneWidget);
    expect(snapshot.firewall.detail, isNotEmpty);

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

    // 4) 设置短名：滚动到输入区，点击输入框聚焦后输入（真实窗口下需先点击以建立
    //    文本输入连接），UI 保存 → 文件落地 → 重新初始化可读回（重启语义）
    await scrollTo(tester, find.byType(TextField));
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '端到端-A');
    await scrollTo(tester, find.text('保存'));
    await tester.tap(find.text('保存'));
    await pumpUntilFound(tester, find.text('短名已保存'));
    expect(configFile.existsSync(), isTrue);
    expect(configFile.readAsStringSync(), contains('端到端-A'));
    expect((await RustBridgeService().init()).shortName, '端到端-A');

    // 5) 非法短名被拒：真实 Rust 校验、错误文案可见、原值不变
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField), '含 空格');
    await scrollTo(tester, find.text('保存'));
    await tester.tap(find.text('保存'));
    await pumpUntilFound(tester, find.textContaining('短名无效'));
    expect(configFile.readAsStringSync(), contains('端到端-A'), reason: '被拒后原值不变');
    expect((await RustBridgeService().init()).shortName, '端到端-A');

    // 6) 清空：键移除并持久
    await scrollTo(tester, find.text('清空'));
    await tester.tap(find.text('清空'));
    await pumpUntilFound(tester, find.text('短名已清空'));
    expect(configFile.readAsStringSync(), isNot(contains('short_name')));
    expect((await RustBridgeService().init()).shortName, isNull);
  });

  testWidgets('端到端：复制本机配置 → 剪贴板片段可直接连通', (tester) async {
    await RustLib.init();
    final snapshot = await RustBridgeService().init();

    await tester.pumpWidget(AgentBridgeApp(service: RustBridgeService()));
    await tester.pumpAndSettle();
    await scrollTo(tester, find.text('复制本机配置'));
    await tester.tap(find.text('复制本机配置'));
    await pumpUntilFound(tester, find.textContaining('已复制到剪贴板'));

    // 读回真实剪贴板
    final data = await Clipboard.getData('text/plain');
    final snippet = data?.text ?? '';
    expect(snippet, contains(snapshot.uuid), reason: '片段应含本机 UUID');
    expect(snippet, contains('[[peer]]'));
    expect(snippet, matches(RegExp(r'token = "[0-9a-f]{64}"')), reason: '应含会话 token');
    expect(snippet, contains('address = "'));

    // 片段写入 CLI 侧配置 → 以该片段的会话 token 连通本机
    final cli = Platform.environment['AGENT_BRIDGE_CLI_BIN'];
    expect(cli, isNotNull, reason: '需以 AGENT_BRIDGE_CLI_BIN 指定 agent-bridge 二进制');
    final receiverHome = Directory.systemTemp.createTempSync('ab-receiver');
    final receiverConfig = Directory('${receiverHome.path}/agent-bridge')
      ..createSync(recursive: true);
    File('${receiverConfig.path}/config.toml').writeAsStringSync(
      '[device]\nuuid = "dddddddd-dddd-4ddd-8ddd-dddddddddddd"\n'
      'long_term_token = "${'e' * 64}"\n\n$snippet',
    );
    final result = await Process.run(
      cli!,
      ['hello', snapshot.uuid],
      environment: {
        'XDG_CONFIG_HOME': receiverHome.path,
        'HOME': receiverHome.path,
      },
    );
    expect(result.exitCode, 0, reason: 'stderr: ${result.stderr}');
    expect(result.stdout, contains(snapshot.uuid));
  });

  testWidgets('端到端：开机自启开关写删机制文件', (tester) async {
    await RustLib.init();
    await RustBridgeService().init();

    final home = Platform.environment['HOME'];
    expect(home, isNotNull, reason: '测试须指定 HOME');
    final desktopFile = File('$home/.config/autostart/agent-bridge.desktop');
    if (desktopFile.existsSync()) {
      desktopFile.deleteSync();
    }

    await tester.pumpWidget(AgentBridgeApp(service: RustBridgeService()));
    await tester.pumpAndSettle();
    await scrollTo(tester, find.byType(SwitchListTile));
    await tester.tap(find.byType(SwitchListTile));
    await pumpUntilFound(tester, find.text('已开启开机自启'));
    expect(desktopFile.existsSync(), isTrue, reason: '应写入 autostart .desktop');
    final content = desktopFile.readAsStringSync();
    expect(content, contains('[Desktop Entry]'));
    expect(content, contains('Exec='));

    await tester.tap(find.byType(SwitchListTile));
    await pumpUntilFound(tester, find.text('已关闭开机自启'));
    expect(desktopFile.existsSync(), isFalse, reason: '关闭应删除机制文件');
  });

  testWidgets('端到端：发现与配对区块就绪（单实例空态）', (tester) async {
    await RustLib.init();
    final snapshot = await RustBridgeService().init();
    expect(snapshot.discovery.available, isTrue, reason: snapshot.discovery.detail);
    await tester.pumpWidget(AgentBridgeApp(service: RustBridgeService()));
    await tester.pumpAndSettle();
    await scrollTo(tester, find.text('发现设备'));
    expect(find.textContaining('暂无发现设备'), findsOneWidget);
    await scrollTo(tester, find.text('已配对设备'));
    expect(find.textContaining('暂无已配对设备'), findsOneWidget);
  });

  testWidgets('端到端：图形配对全链路（需外部对端）', (tester) async {
    final peerUuid = Platform.environment['AGENT_BRIDGE_PEER_UUID'];
    if (peerUuid == null) {
      // 未编排外部对端时跳过（真机脚本会提供该变量）
      return;
    }
    await RustLib.init();
    await RustBridgeService().init();
    await tester.pumpWidget(AgentBridgeApp(service: RustBridgeService()));
    await tester.pumpAndSettle();

    // 等待自发现到对端（信标周期 3 秒）
    var found = false;
    final deadline = DateTime.now().add(const Duration(seconds: 40));
    while (!found && DateTime.now().isBefore(deadline)) {
      final discovered = await RustBridgeService().discoveredDevices();
      found = discovered.any((device) => device.uuid == peerUuid);
      if (!found) {
        await tester.pump(const Duration(seconds: 1));
      }
    }
    expect(found, isTrue, reason: '40 秒内应发现对端信标');

    await tester.pump(const Duration(seconds: 1));
    await scrollTo(tester, find.text('发起配对'));
    await tester.tap(find.text('发起配对').first);
    // 外部脚本会在对端窗口点击「同意」；等到结果 SnackBar（带冒号，
    // 避免误配「已配对设备」区块标题）
    await pumpUntilFound(
      tester,
      find.textContaining('已配对：'),
      timeout: const Duration(seconds: 90),
    );

    // 配置已写入且 CLI 可连通
    final xdg = Platform.environment['XDG_CONFIG_HOME']!;
    final configText = File('$xdg/agent-bridge/config.toml').readAsStringSync();
    expect(configText, contains(peerUuid));
    final cli = Platform.environment['AGENT_BRIDGE_CLI_BIN']!;
    final home = Platform.environment['HOME']!;
    final result = await Process.run(
      cli,
      ['hello', peerUuid],
      environment: {'XDG_CONFIG_HOME': xdg, 'HOME': home},
    );
    expect(result.exitCode, 0, reason: 'stderr: ${result.stderr}');
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
      // 横幅（错误卡）与「服务端」状态行；防火墙行同样含「服务端未运行」字样，故用更精确的断言
      expect(find.textContaining('服务端未运行：端口'), findsOneWidget);
      expect(find.textContaining('已被占用'), findsOneWidget);
      expect(find.text('未运行'), findsOneWidget);
    } finally {
      await squatter.close();
    }
  });
}
