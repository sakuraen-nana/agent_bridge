import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:agent_bridge_app/src/bridge_service.dart';
import 'package:agent_bridge_app/src/home_page.dart';
import 'package:agent_bridge_app/src/rust/api/init.dart' show AppSnapshot;
import 'package:agent_bridge_app/src/rust/sysinfo_view.dart' show SystemSnapshot;

SystemSnapshot _system({List<String>? ips}) => SystemSnapshot(
      platform: 'TestOS 1.0（内核 9.9）',
      locale: 'zh-CN',
      localTime: '2026-10-01 20:00:00 +08:00',
      cpu: 'TestCPU 9000（8 物理核）',
      memory: '总计 16.0 GiB · 可用 8.0 GiB',
      ipAddresses: ips ?? const ['192.168.1.10 (eth0)'],
    );

AppSnapshot _snapshot({String? shortName, String? notice, List<String>? ips}) =>
    AppSnapshot(
      version: '0.0.0',
      uuid: '11111111-2222-4333-8444-555555555555',
      shortName: shortName,
      notice: notice,
      system: _system(ips: ips),
    );

class _FakeBridgeService implements BridgeService {
  _FakeBridgeService(this._current);

  AppSnapshot _current;
  Object? errorOnSet;
  SystemSnapshot? nextSystem;
  final List<String?> setCalls = [];

  @override
  Future<AppSnapshot> init() async => _current;

  @override
  Future<SystemSnapshot> refreshSystem() async =>
      nextSystem ?? _current.system;

  @override
  Future<AppSnapshot> setShortName(String? name) async {
    setCalls.add(name);
    final error = errorOnSet;
    if (error != null) {
      throw error;
    }
    _current = AppSnapshot(
      version: _current.version,
      uuid: _current.uuid,
      shortName: name,
      notice: null,
      system: _current.system,
    );
    return _current;
  }
}

Future<void> _pumpHome(WidgetTester tester, BridgeService service) async {
  await tester.pumpWidget(MaterialApp(home: HomePage(service: service)));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('信息面板展示全部字段与取值', (tester) async {
    await _pumpHome(tester, _FakeBridgeService(_snapshot()));

    expect(find.text('应用版本'), findsOneWidget);
    expect(find.text('0.0.0'), findsOneWidget);
    expect(find.text('设备 UUID'), findsOneWidget);
    expect(find.text('11111111-2222-4333-8444-555555555555'), findsOneWidget);
    expect(find.text('未设置'), findsOneWidget);
    expect(find.text('TestOS 1.0（内核 9.9）'), findsOneWidget);
    expect(find.text('zh-CN'), findsOneWidget);
    expect(find.text('2026-10-01 20:00:00 +08:00'), findsOneWidget);
    expect(find.text('TestCPU 9000（8 物理核）'), findsOneWidget);
    expect(find.text('总计 16.0 GiB · 可用 8.0 GiB'), findsOneWidget);
    expect(find.text('192.168.1.10 (eth0)'), findsOneWidget);
  });

  testWidgets('无局域网地址时展示占位', (tester) async {
    await _pumpHome(tester, _FakeBridgeService(_snapshot(ips: const [])));
    expect(find.text('无'), findsOneWidget);
  });

  testWidgets('配置重建提示可见', (tester) async {
    await _pumpHome(
      tester,
      _FakeBridgeService(_snapshot(notice: '配置文件无法解析，原文件已备份并重建默认配置')),
    );
    expect(find.textContaining('已备份并重建'), findsOneWidget);
  });

  testWidgets('保存合法短名更新展示', (tester) async {
    final service = _FakeBridgeService(_snapshot());
    await _pumpHome(tester, service);

    await tester.enterText(find.byType(TextField), '开发机-A');
    await tester.tap(find.text('保存'));
    await tester.pumpAndSettle();

    expect(service.setCalls, ['开发机-A']);
    expect(find.text('开发机-A'), findsWidgets);
    expect(find.text('短名已保存'), findsOneWidget);
  });

  testWidgets('非法短名被拒并提示，原值不变', (tester) async {
    final service = _FakeBridgeService(_snapshot(shortName: '原名'));
    service.errorOnSet = Exception('短名无效：不能包含空白或控制字符');
    await _pumpHome(tester, service);

    await tester.enterText(find.byType(TextField), '含 空格');
    await tester.tap(find.text('保存'));
    await tester.pumpAndSettle();

    expect(find.textContaining('短名无效'), findsOneWidget);
    expect(find.text('原名'), findsOneWidget, reason: '面板应仍显示原短名');
  });

  testWidgets('清空短名', (tester) async {
    final service = _FakeBridgeService(_snapshot(shortName: '要清掉的'));
    await _pumpHome(tester, service);

    await tester.tap(find.text('清空'));
    await tester.pumpAndSettle();

    expect(service.setCalls, [null]);
    expect(find.text('未设置'), findsOneWidget);
    expect(find.text('短名已清空'), findsOneWidget);
  });

  testWidgets('刷新系统信息', (tester) async {
    final service = _FakeBridgeService(_snapshot());
    service.nextSystem = SystemSnapshot(
      platform: 'TestOS 1.0（内核 9.9）',
      locale: 'zh-CN',
      localTime: '2026-10-01 21:30:00 +08:00',
      cpu: 'TestCPU 9000（8 物理核）',
      memory: '总计 16.0 GiB · 可用 7.5 GiB',
      ipAddresses: const ['192.168.1.11 (eth0)'],
    );
    await _pumpHome(tester, service);

    await tester.tap(find.byTooltip('刷新'));
    await tester.pumpAndSettle();

    expect(find.text('2026-10-01 21:30:00 +08:00'), findsOneWidget);
    expect(find.text('192.168.1.11 (eth0)'), findsOneWidget);
  });
}
