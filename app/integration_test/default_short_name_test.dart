// 端到端：短名启动补全——为空（未设置/已清空/空值）时以本机设备名（主机名）
// 初始化并持久化；清空仅当次运行内生效（app_init 不填回）；已设置不覆盖。
//
// 运行（需 DISPLAY 与隔离的 XDG_CONFIG_HOME）：
//   DISPLAY=:99 AGENT_BRIDGE_PORT=<port> XDG_CONFIG_HOME=<临时目录> \
//     flutter test integration_test/default_short_name_test.dart -d linux
//
// 注：frb 同进程不可二次初始化，集成用例按「每文件一条」组织（整文件或
// --plain-name 单跑皆可）。

import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'package:agent_bridge_app/src/bridge_service.dart';
import 'package:agent_bridge_app/src/rust/frb_generated.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('端到端：短名为空时启动补全填回设备名；已设置不覆盖', (tester) async {
    await RustLib.init();
    final service = RustBridgeService();
    final xdg = Platform.environment['XDG_CONFIG_HOME'];
    expect(xdg, isNotNull, reason: '测试须以隔离的 XDG_CONFIG_HOME 运行');
    final configFile = File('$xdg/agent-bridge/config.toml');

    // 清空：当次运行内保持为空——init()（app_init）不得填回（design D1）
    await service.setShortName(null);
    expect((await service.init()).shortName, isNull,
        reason: '清空后同会话不应被 app_init 填回');
    expect(configFile.readAsStringSync(), isNot(contains('short_name')));

    // 启动补全：填回本机设备名（主机名），并持久化
    final filled = await service.ensureDefaultShortName();
    expect(filled, isNotNull, reason: '本机设备名应满足短名规则');
    expect(filled, Platform.localHostname, reason: '应等于本机设备名（主机名）');
    expect((await service.init()).shortName, filled, reason: '快照应读到补全值');
    expect(configFile.readAsStringSync(), contains('short_name = "$filled"'));

    // 已有非空短名不被覆盖
    await service.setShortName('自定义名');
    expect(await service.ensureDefaultShortName(), '自定义名');
    expect((await service.init()).shortName, '自定义名');
    expect(configFile.readAsStringSync(), contains('short_name = "自定义名"'));
  });
}
