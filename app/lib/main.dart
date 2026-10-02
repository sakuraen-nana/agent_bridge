import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'package:tray_manager/tray_manager.dart' as tray;
import 'package:window_manager/window_manager.dart';

import 'src/bridge_service.dart';
import 'src/home_page.dart';
import 'src/rust/frb_generated.dart';
import 'src/window_geometry.dart';

// 保持引用：托盘/菜单对象被 GC 会释放原生句柄（图标随之消失）。
// 仅在 _setupTray 中赋值、以顶层变量形式存活到进程结束，故免疫 unused 提示。
// ignore: unused_element
tray.TrayIcon? _trayIcon;
// ignore: unused_element
tray.Menu? _trayMenu;

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();

  // 短名启动补全：为空（未设置/已清空/空值）时以本机设备名初始化，仅启动路径
  // 调用一次（design D1）；失败不阻断启动——配置不可用等真实错误由随后的
  // app_init 如实呈现。
  try {
    await RustBridgeService().ensureDefaultShortName();
  } catch (error) {
    debugPrint('短名启动补全失败：$error');
  }

  await windowManager.ensureInitialized();

  // 托盘：宿主探测 + 创建失败任一不满足即降级为「关窗即退出」
  // （行为与升级前一致，不形成没有可见入口的幽灵进程）
  var trayReady = await _setupTray();
  if (trayReady) {
    try {
      final hostAvailable = await RustBridgeService().trayHostAvailable();
      if (!hostAvailable) {
        debugPrint('托盘宿主不可用（无 StatusNotifierWatcher），关闭窗口将退出');
        _trayIcon?.setVisible(false);
        trayReady = false;
      }
    } catch (error) {
      debugPrint('托盘宿主探测失败，按不可用处理：$error');
      trayReady = false;
    }
  }

  // 关闭窗口统一拦截：托盘可用 → 隐藏；不可用 → 真正退出
  await windowManager.setPreventClose(true);
  windowManager.addListener(_AppWindowListener(trayReady: trayReady));

  await windowManager.waitUntilReadyToShow(
    WindowOptions(
      size: await _startupWindowSize(),
      minimumSize: kMinimumWindowSize,
      center: true,
      title: 'agent-bridge',
    ),
    () async {
      await windowManager.show();
      await windowManager.focus();
    },
  );

  runApp(AgentBridgeApp(service: RustBridgeService(), trayReady: trayReady));
}

/// 启动窗口尺寸：宽:高 = 1:2 竖向窄窗（默认 480×960）；屏幕可用区域放不下时
/// 等比缩小（仍 1:2）。屏幕信息读取失败时回退默认尺寸、不阻断启动（design D3）。
Future<Size> _startupWindowSize() async {
  try {
    final display = await screenRetriever.getPrimaryDisplay();
    return startupWindowSize(display.visibleSize ?? display.size);
  } catch (error) {
    debugPrint('屏幕可用区域读取失败，按默认尺寸启动：$error');
    return kDefaultStartupWindowSize;
  }
}

/// 建立系统托盘（菜单：显示窗口 / 复制本机配置 / 退出）；失败返回 false。
Future<bool> _setupTray() async {
  try {
    final icon = tray.TrayIcon.create();
    final menu = tray.Menu.create();
    if (icon == null || menu == null) {
      throw StateError('系统托盘不可用');
    }
    _trayIcon = icon;
    _trayMenu = menu;

    final assetPath =
        Platform.isWindows ? 'assets/tray_icon.ico' : 'assets/tray_icon.png';
    final bytes = await rootBundle.load(assetPath);
    icon.icon = tray.Image.fromBase64(base64Encode(bytes.buffer.asUint8List()));

    final showItem =
        tray.MenuItem.createWithLabelAndType('显示窗口', tray.MenuItemType.normal);
    showItem?.addListener((event) {
      if (event is tray.MenuItemClickedEvent) {
        unawaited(_showWindow());
      }
    });
    final copyItem =
        tray.MenuItem.createWithLabelAndType('复制本机配置', tray.MenuItemType.normal);
    copyItem?.addListener((event) {
      if (event is tray.MenuItemClickedEvent) {
        unawaited(_copySharePayload());
      }
    });
    final exitItem =
        tray.MenuItem.createWithLabelAndType('退出', tray.MenuItemType.normal);
    exitItem?.addListener((event) {
      if (event is tray.MenuItemClickedEvent) {
        unawaited(windowManager.destroy());
      }
    });

    menu.addItem(showItem);
    menu.addSeparator();
    menu.addItem(copyItem);
    menu.addSeparator();
    menu.addItem(exitItem);
    icon.setContextMenu(menu);
    icon.addListener((event) {
      if (event is tray.TrayIconClickedEvent) {
        unawaited(_showWindow());
      }
    });
    icon.setVisible(true);
    return true;
  } catch (error) {
    debugPrint('托盘不可用，关闭窗口将退出：$error');
    return false;
  }
}

Future<void> _showWindow() async {
  await windowManager.show();
  await windowManager.focus();
}

Future<void> _copySharePayload() async {
  try {
    final payload = await RustBridgeService().sharePayload();
    await Clipboard.setData(ClipboardData(text: payload));
  } catch (error) {
    debugPrint('复制本机配置失败：${readableError(error)}');
  }
}

/// 窗口关闭拦截（design D4）。
class _AppWindowListener with WindowListener {
  _AppWindowListener({required this.trayReady});

  final bool trayReady;

  @override
  void onWindowClose() async {
    if (trayReady) {
      await windowManager.hide();
    } else {
      await windowManager.destroy();
    }
  }
}

/// 应用根组件（service 可注入，供 widget 测试复用）。
class AgentBridgeApp extends StatelessWidget {
  const AgentBridgeApp({
    super.key,
    required this.service,
    this.trayReady = true,
  });

  final BridgeService service;

  /// 托盘是否可用（false 时界面提示「关闭窗口将退出」）。
  final bool trayReady;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'agent-bridge',
      theme: ThemeData(colorSchemeSeed: Colors.blue, useMaterial3: true),
      home: HomePage(service: service, trayReady: trayReady),
    );
  }
}
