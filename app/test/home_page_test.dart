import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:agent_bridge_app/src/bridge_service.dart';
import 'package:agent_bridge_app/src/home_page.dart';
import 'package:agent_bridge_app/src/rust/api/init.dart'
    show AppSnapshot, AutostartInfo, ElevationSnapshot, FirewallSnapshot, ServerSnapshot;
import 'package:agent_bridge_app/src/rust/sysinfo_view.dart' show SystemSnapshot;

SystemSnapshot _system({List<String>? ips}) => SystemSnapshot(
      platform: 'TestOS 1.0（内核 9.9）',
      locale: 'zh-CN',
      localTime: '2026-10-01 20:00:00 +08:00',
      cpu: 'TestCPU 9000（8 物理核）',
      memory: '总计 16.0 GiB · 可用 8.0 GiB',
      ipAddresses: ips ?? const ['192.168.1.10 (eth0)'],
    );

AppSnapshot _snapshot({
  String? shortName,
  String? notice,
  List<String>? ips,
  ServerSnapshot? server,
  ElevationSnapshot? elevation,
  FirewallSnapshot? firewall,
}) =>
    AppSnapshot(
      version: '0.0.0',
      uuid: '11111111-2222-4333-8444-555555555555',
      shortName: shortName,
      notice: notice,
      system: _system(ips: ips),
      server: server ?? const ServerSnapshot(running: true, port: 37777),
      elevation: elevation ??
          const ElevationSnapshot(admin: true, detail: '已具备（root/管理员）'),
      firewall: firewall ??
          const FirewallSnapshot(
            manager: 'ufw',
            active: false,
            applied: false,
            detail: 'ufw 未激活，无需放行',
          ),
    );

class _FakeBridgeService implements BridgeService {
  _FakeBridgeService(this._current);

  AppSnapshot _current;
  Object? errorOnSet;
  SystemSnapshot? nextSystem;
  String payload = '# 片段\n[[peer]]\nuuid = "x"\ntoken = "y"\n';
  Object? payloadError;
  bool autostartEnabled = false;
  final List<String?> setCalls = [];
  final List<bool> autostartCalls = [];

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
      server: _current.server,
      elevation: _current.elevation,
      firewall: _current.firewall,
    );
    return _current;
  }

  @override
  Future<String> sharePayload() async {
    final error = payloadError;
    if (error != null) {
      throw error;
    }
    return payload;
  }

  @override
  Future<AutostartInfo> autostartStatus() async => AutostartInfo(
        enabled: autostartEnabled,
        mechanism: 'autostart .desktop',
        detail: '/home/u/.config/autostart/agent-bridge.desktop',
      );

  @override
  Future<AutostartInfo> setAutostart(bool enabled) async {
    autostartCalls.add(enabled);
    autostartEnabled = enabled;
    return autostartStatus();
  }

  @override
  Future<bool> trayHostAvailable() async => true;
}

Future<void> _pumpHome(
  WidgetTester tester,
  BridgeService service, {
  bool trayReady = true,
}) async {
  // 用与真实窗口相近的视口，避免按钮被挤出默认 800x600 的测试画布
  tester.view.physicalSize = const Size(1400, 1400);
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(home: HomePage(service: service, trayReady: trayReady)),
  );
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

  testWidgets('服务端运行中展示端口', (tester) async {
    await _pumpHome(tester, _FakeBridgeService(_snapshot()));
    expect(find.text('服务端'), findsOneWidget);
    expect(find.text('运行中（端口 37777）'), findsOneWidget);
  });

  testWidgets('服务端启动失败展示错误横幅与未运行', (tester) async {
    await _pumpHome(
      tester,
      _FakeBridgeService(_snapshot(
        server: const ServerSnapshot(
          running: false,
          port: 37777,
          error: '端口 37777 已被占用（不自动更换端口）：Address already in use',
        ),
      )),
    );
    expect(find.textContaining('服务端未运行'), findsOneWidget);
    expect(find.textContaining('已被占用'), findsOneWidget);
    expect(find.text('未运行'), findsOneWidget);
  });

  testWidgets('展示管理员权限与防火墙状态行', (tester) async {
    await _pumpHome(
      tester,
      _FakeBridgeService(_snapshot(
        elevation: const ElevationSnapshot(
          admin: false,
          detail: '受限模式：无图形会话（DISPLAY/WAYLAND_DISPLAY 未设置），无法发起提权请求',
        ),
        firewall: const FirewallSnapshot(
          manager: null,
          active: false,
          applied: false,
          detail: '未检测到受支持的活跃防火墙（ufw/firewalld）；如有自管规则请手动放行 37777/tcp',
        ),
      )),
    );
    expect(find.text('管理员权限'), findsOneWidget);
    // 受限模式：显著横幅 + 状态行各一处
    expect(find.textContaining('受限模式'), findsNWidgets(2));
    expect(find.text('防火墙'), findsOneWidget);
    expect(find.textContaining('手动放行 37777/tcp'), findsOneWidget);
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

  testWidgets('复制本机配置写入剪贴板', (tester) async {
    final service = _FakeBridgeService(_snapshot());
    final calls = <MethodCall>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (call) async {
        calls.add(call);
        return null;
      },
    );
    addTearDown(() {
      tester.binding.defaultBinaryMessenger
          .setMockMethodCallHandler(SystemChannels.platform, null);
    });

    await _pumpHome(tester, service);
    await tester.ensureVisible(find.text('复制本机配置'));
    await tester.tap(find.text('复制本机配置'));
    await tester.pumpAndSettle();

    final clipboardCall = calls.firstWhere(
      (call) => call.method == 'Clipboard.setData',
      orElse: () => throw StateError('未写入剪贴板'),
    );
    expect(clipboardCall.arguments['text'], service.payload);
    expect(find.textContaining('已复制到剪贴板'), findsOneWidget);
  });

  testWidgets('复制失败展示错误', (tester) async {
    final service = _FakeBridgeService(_snapshot());
    service.payloadError = Exception('服务端未运行：请先确保应用面板显示「服务端：运行中」');
    await _pumpHome(tester, service);

    await tester.ensureVisible(find.text('复制本机配置'));
    await tester.tap(find.text('复制本机配置'));
    await tester.pumpAndSettle();

    expect(find.textContaining('服务端未运行'), findsOneWidget);
  });

  testWidgets('开机自启开关调用服务并更新', (tester) async {
    final service = _FakeBridgeService(_snapshot());
    await _pumpHome(tester, service);

    expect(find.text('开机自启'), findsOneWidget);
    await tester.ensureVisible(find.byType(SwitchListTile));
    await tester.tap(find.byType(SwitchListTile));
    await tester.pumpAndSettle();

    expect(service.autostartCalls, [true]);
    expect(find.text('已开启开机自启'), findsOneWidget);
  });

  testWidgets('托盘不可用时提示关窗即退出', (tester) async {
    await _pumpHome(tester, _FakeBridgeService(_snapshot()), trayReady: false);
    expect(find.textContaining('托盘不可用'), findsOneWidget);
  });
}
