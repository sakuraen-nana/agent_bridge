import 'package:flutter/material.dart';

import 'src/bridge_service.dart';
import 'src/home_page.dart';
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  runApp(AgentBridgeApp(service: RustBridgeService()));
}

/// 应用根组件（service 可注入，供 widget 测试复用）。
class AgentBridgeApp extends StatelessWidget {
  const AgentBridgeApp({super.key, required this.service});

  final BridgeService service;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'agent-bridge',
      theme: ThemeData(colorSchemeSeed: Colors.blue, useMaterial3: true),
      home: HomePage(service: service),
    );
  }
}
