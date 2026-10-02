// 启动窗口尺寸计算（纯函数）用例：默认 1:2 竖向，小屏等比缩小且恒 1:2。

import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';

import 'package:agent_bridge_app/src/window_geometry.dart';

void main() {
  test('常规屏幕（可用高 ≥ 960）取默认 480×960', () {
    expect(startupWindowSize(const Size(1920, 1080)),
        kDefaultStartupWindowSize);
    expect(startupWindowSize(const Size(2560, 1440)),
        kDefaultStartupWindowSize);
  });

  test('恰为默认可用区域时取默认尺寸', () {
    expect(startupWindowSize(const Size(480, 960)), kDefaultStartupWindowSize);
  });

  test('可用高度不足时按高度等比缩小', () {
    expect(startupWindowSize(const Size(1920, 728)), const Size(364, 728));
  });

  test('极窄屏按宽度再缩', () {
    expect(startupWindowSize(const Size(400, 2000)), const Size(400, 800));
  });

  test('各分支恒满足宽:高 = 1:2 且不超出可用区域', () {
    const cases = [
      Size(1920, 1080),
      Size(1366, 728),
      Size(400, 2000),
      Size(480, 960),
      Size(300, 300),
    ];
    for (final visible in cases) {
      final size = startupWindowSize(visible);
      expect(size.width * 2, closeTo(size.height, 1e-9),
          reason: '$visible 下应保持 1:2');
      expect(size.width, lessThanOrEqualTo(visible.width), reason: '$visible');
      expect(size.height, lessThanOrEqualTo(visible.height), reason: '$visible');
    }
  });
}
