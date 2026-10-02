import 'dart:math';
import 'dart:ui';

/// 启动窗口默认尺寸（宽:高 = 1:2 竖向窄窗，design D3）。
const Size kDefaultStartupWindowSize = Size(480, 960);

/// 窗口最小尺寸（同为 1:2）。
const Size kMinimumWindowSize = Size(360, 720);

/// 计算启动窗口尺寸：默认 480×960；屏幕可用区域放不下时等比缩小（仍 1:2）。
///
/// [visible] 为屏幕可用区域（扣除任务栏/面板等）。高取 min(默认高, 可用高)；
/// 宽度不足（高/2 超出可用宽）时按宽度再缩；返回值恒满足 宽 = 高 / 2。
/// 比例仅在启动时应用——运行中不锁定，用户可自由调整（规格「启动窗口尺寸」）。
Size startupWindowSize(Size visible) {
  var height = min(kDefaultStartupWindowSize.height, visible.height);
  if (height / 2 > visible.width) {
    height = visible.width * 2;
  }
  return Size(height / 2, height);
}
