// 端到端：启动窗口为宽:高 = 1:2 竖向窄窗（默认 480×960；屏幕可用高度不足时
// 等比缩小）——走真实 main() 启动路径（窗口选项在 main() 中应用）。
//
// 运行（需 DISPLAY 与隔离的 XDG_CONFIG_HOME；建议 1920×1080 虚拟屏以断言默认尺寸）：
//   Xvfb :99 -screen 0 1920x1080x24 &
//   DISPLAY=:99 AGENT_BRIDGE_PORT=<port> XDG_CONFIG_HOME=<临时目录> \
//     flutter test integration_test/startup_window_test.dart -d linux
//
// 注：frb 同进程不可二次初始化，集成用例按「每文件一条」组织（整文件或
// --plain-name 单跑皆可）。

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'package:window_manager/window_manager.dart';

import 'package:agent_bridge_app/main.dart' as app;
import 'package:agent_bridge_app/src/window_geometry.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('端到端：启动窗口为 1:2 竖向（默认或按屏幕可用高缩小）', (tester) async {
    // 走真实启动路径（含窗口选项与短名启动补全）
    await app.main();
    await tester.pumpAndSettle();

    final display = await screenRetriever.getPrimaryDisplay();
    final visible = display.visibleSize ?? display.size;
    final expected = startupWindowSize(visible);
    final size = await windowManager.getSize();
    // 留证：实际/期望尺寸随测试输出可见
    // ignore: avoid_print
    print('启动窗口尺寸：$size（期望 $expected，屏幕可用 $visible）');

    expect(size.width * 2, closeTo(size.height, 1.0),
        reason: '宽:高 应为 1:2（实际 $size）');
    expect(size.height, closeTo(expected.height, 1.0),
        reason: '高应为 min(960, 屏幕可用高)（实际 $size，期望 $expected）');
  });
}
