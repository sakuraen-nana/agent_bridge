import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'bridge_service.dart';
import 'rust/api/init.dart' show AppSnapshot;
import 'rust/api/pair.dart'
    show DiscoveredDeviceInfo, PeerStatusInfo, PendingPairingInfo;

/// 主界面：启动信息面板 + 本机默认短名设置 + 配置分享 + 开机自启。
class HomePage extends StatefulWidget {
  const HomePage({super.key, required this.service, this.trayReady = true});

  final BridgeService service;

  /// 托盘是否可用（false 时提示「关闭窗口将退出」）。
  final bool trayReady;

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  AppSnapshot? _snapshot;
  String? _fatalError;
  final TextEditingController _shortNameController = TextEditingController();
  bool _autostartEnabled = false;
  String _autostartDetail = '';
  bool _autostartLoaded = false;

  // 发现 / 配对 / 在线状态（design D5：轮询模式）
  List<DiscoveredDeviceInfo> _discovered = const [];
  List<PeerStatusInfo> _peerStatuses = const [];
  final Set<String> _pairingInProgress = {};
  bool _dialogOpen = false;
  BigInt? _shownPendingId;
  Timer? _discoveryTimer;
  Timer? _pendingTimer;
  Timer? _peersTimer;

  @override
  void initState() {
    super.initState();
    _bootstrap();
    _loadAutostart();
  }

  /// 初始化顺序：先完成 app_init（数据目录/配置/服务端就绪），再开始轮询，
  /// 避免多个首轮调用并发走「配置不存在→创建」路径（变更 ④ 实跑发现并修复）。
  Future<void> _bootstrap() async {
    await _load();
    if (!mounted) return;
    unawaited(_refreshDiscovered());
    unawaited(_refreshPeers());
    unawaited(_refreshPending());
    _discoveryTimer =
        Timer.periodic(const Duration(seconds: 1), (_) => _refreshDiscovered());
    _pendingTimer =
        Timer.periodic(const Duration(seconds: 1), (_) => _refreshPending());
    _peersTimer =
        Timer.periodic(const Duration(seconds: 5), (_) => _refreshPeers());
  }

  Future<void> _refreshDiscovered() async {
    try {
      final discovered = await widget.service.discoveredDevices();
      if (!mounted) return;
      setState(() => _discovered = discovered);
    } catch (_) {
      // 轮询失败静默（面板本身的状态行已呈现服务端/发现可用性）
    }
  }

  Future<void> _refreshPeers() async {
    try {
      final statuses = await widget.service.peersStatus();
      if (!mounted) return;
      setState(() => _peerStatuses = statuses);
    } catch (_) {
      // 同上
    }
  }

  Future<void> _refreshPending() async {
    if (_dialogOpen) return;
    PendingPairingInfo? pending;
    try {
      pending = await widget.service.pairingPending();
    } catch (_) {
      return;
    }
    if (!mounted || pending == null || pending.id == _shownPendingId) return;
    _dialogOpen = true;
    _shownPendingId = pending.id;
    final approve = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (context) => AlertDialog(
        title: const Text('配对请求'),
        content: Text(
          '设备「${pending!.shortName ?? "未命名"}」请求与本机配对：\n\n'
          'UUID：${pending.uuid}\n'
          '来源 IP：${pending.sourceIp}\n'
          '服务端口：${pending.port}\n\n'
          '同意后，本机配置（含长期 token）将发送给请求方。',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('拒绝'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('同意'),
          ),
        ],
      ),
    );
    _dialogOpen = false;
    if (approve == null) return;
    try {
      await widget.service.respondPairing(approve);
      _showMessage(approve ? '已同意配对' : '已拒绝配对');
      _refreshPeers();
    } catch (error) {
      _showMessage(readableError(error));
    }
  }

  Future<void> _startPairing(String uuid) async {
    setState(() => _pairingInProgress.add(uuid));
    try {
      final outcome = await widget.service.requestPairing(uuid);
      final message = switch (outcome.status) {
        'paired' => outcome.detail,
        'rejected' => '对端拒绝了配对请求',
        'timeout' => '等待对端决定超时（可重试）',
        'busy' => '对端已有待处理的配对请求（请稍后重试）',
        'not_found' => outcome.detail,
        _ => outcome.detail,
      };
      _showMessage(message);
      _refreshDiscovered();
      _refreshPeers();
    } catch (error) {
      _showMessage(readableError(error));
    } finally {
      if (mounted) {
        setState(() => _pairingInProgress.remove(uuid));
      }
    }
  }

  Future<void> _loadAutostart() async {
    try {
      final info = await widget.service.autostartStatus();
      if (!mounted) return;
      setState(() {
        _autostartEnabled = info.enabled;
        _autostartDetail = info.detail;
        _autostartLoaded = true;
      });
    } catch (error) {
      if (!mounted) return;
      setState(() => _autostartLoaded = true);
      _showMessage(readableError(error));
    }
  }

  Future<void> _toggleAutostart(bool enabled) async {
    try {
      final info = await widget.service.setAutostart(enabled);
      if (!mounted) return;
      setState(() {
        _autostartEnabled = info.enabled;
        _autostartDetail = info.detail;
      });
      _showMessage(enabled ? '已开启开机自启' : '已关闭开机自启');
    } catch (error) {
      _showMessage(readableError(error));
    }
  }

  Future<void> _copySharePayload() async {
    try {
      final payload = await widget.service.sharePayload();
      await Clipboard.setData(ClipboardData(text: payload));
      _showMessage('已复制到剪贴板（token 为本次会话短期 token，重启后失效）');
    } catch (error) {
      _showMessage(readableError(error));
    }
  }

  @override
  void dispose() {
    _discoveryTimer?.cancel();
    _pendingTimer?.cancel();
    _peersTimer?.cancel();
    _shortNameController.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final snapshot = await widget.service.init();
      if (!mounted) return;
      setState(() {
        _snapshot = snapshot;
        _fatalError = null;
        _shortNameController.text = snapshot.shortName ?? '';
      });
    } catch (error) {
      if (!mounted) return;
      setState(() => _fatalError = readableError(error));
    }
  }

  Future<void> _refreshSystem() async {
    try {
      final system = await widget.service.refreshSystem();
      final current = _snapshot;
      if (!mounted || current == null) return;
      setState(() {
        _snapshot = AppSnapshot(
          version: current.version,
          uuid: current.uuid,
          shortName: current.shortName,
          notice: null,
          system: system,
          server: current.server,
          elevation: current.elevation,
          firewall: current.firewall,
          discovery: current.discovery,
        );
      });
    } catch (error) {
      _showMessage(readableError(error));
    }
  }

  Future<void> _saveShortName() async {
    await _applyShortName(_shortNameController.text, '短名已保存');
  }

  Future<void> _clearShortName() async {
    await _applyShortName(null, '短名已清空');
  }

  Future<void> _applyShortName(String? name, String successMessage) async {
    try {
      final snapshot = await widget.service.setShortName(name);
      if (!mounted) return;
      setState(() {
        _snapshot = snapshot;
        _shortNameController.text = snapshot.shortName ?? '';
      });
      _showMessage(successMessage);
    } catch (error) {
      _showMessage(readableError(error));
    }
  }

  void _showMessage(String message) {
    if (!mounted) return;
    ScaffoldMessenger.of(context)
      ..hideCurrentSnackBar()
      ..showSnackBar(SnackBar(content: Text(message)));
  }

  @override
  Widget build(BuildContext context) {
    final snapshot = _snapshot;
    final fatalError = _fatalError;
    return Scaffold(
      appBar: AppBar(
        title: const Text('agent-bridge'),
        actions: [
          IconButton(
            tooltip: '刷新',
            onPressed: snapshot == null ? null : _refreshSystem,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: fatalError != null
          ? _FatalErrorView(message: fatalError)
          : snapshot == null
              ? const Center(child: CircularProgressIndicator())
              : _buildContent(snapshot),
    );
  }

  Widget _buildContent(AppSnapshot snapshot) {
    final system = snapshot.system;
    final ips =
        system.ipAddresses.isEmpty ? '无' : system.ipAddresses.join('\n');
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        if (snapshot.server.error != null)
          _ErrorCard(text: '服务端未运行：${snapshot.server.error}'),
        if (!snapshot.elevation.admin)
          _ErrorCard(text: snapshot.elevation.detail),
        if (snapshot.notice != null) _NoticeCard(text: snapshot.notice!),
        if (!widget.trayReady)
          const _NoticeCard(text: '托盘不可用：关闭窗口将退出应用'),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('本机信息', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 8),
                _InfoRow(label: '应用版本', value: snapshot.version),
                _InfoRow(label: '设备 UUID', value: snapshot.uuid),
                _InfoRow(label: '本机短名', value: snapshot.shortName ?? '未设置'),
                _InfoRow(label: '操作系统', value: system.platform),
                _InfoRow(label: '区域与语言', value: system.locale),
                _InfoRow(label: '本地时间', value: system.localTime),
                _InfoRow(label: 'CPU', value: system.cpu),
                _InfoRow(label: '内存', value: system.memory),
                _InfoRow(label: '局域网 IP', value: ips),
                _InfoRow(
                  label: '服务端',
                  value: snapshot.server.running
                      ? '运行中（端口 ${snapshot.server.port}）'
                      : '未运行',
                ),
                _InfoRow(label: '管理员权限', value: snapshot.elevation.detail),
                _InfoRow(label: '防火墙', value: snapshot.firewall.detail),
                _InfoRow(label: '发现', value: snapshot.discovery.detail),
              ],
            ),
          ),
        ),
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('本机默认短名', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 8),
                TextField(
                  controller: _shortNameController,
                  decoration: const InputDecoration(
                    labelText: '短名（1–32 字符，不含空白）',
                    border: OutlineInputBorder(),
                  ),
                  onSubmitted: (_) => _saveShortName(),
                ),
                const SizedBox(height: 12),
                Row(
                  children: [
                    FilledButton(
                      onPressed: _saveShortName,
                      child: const Text('保存'),
                    ),
                    const SizedBox(width: 8),
                    OutlinedButton(
                      onPressed: _clearShortName,
                      child: const Text('清空'),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('本机配置分享', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 8),
                Text(
                  '复制含本次会话短期 token 的配置片段；直接粘贴到对端 config.toml 即可使用（重启本应用后该 token 失效）。',
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                        color: Theme.of(context).colorScheme.onSurfaceVariant,
                      ),
                ),
                const SizedBox(height: 12),
                FilledButton.icon(
                  onPressed: _copySharePayload,
                  icon: const Icon(Icons.copy),
                  label: const Text('复制本机配置'),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const SizedBox(height: 8),
                Text('开机自启', style: Theme.of(context).textTheme.titleMedium),
                SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  title: const Text('开机自动启动'),
                  subtitle: Text(_autostartLoaded ? _autostartDetail : '（加载中…）'),
                  value: _autostartEnabled,
                  onChanged: _autostartLoaded ? _toggleAutostart : null,
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('发现设备', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 4),
                Text(
                  '每 3 秒扫描局域网；点「发起配对」向目标设备发送请求，需对方在弹窗中同意。',
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                        color: Theme.of(context).colorScheme.onSurfaceVariant,
                      ),
                ),
                const SizedBox(height: 8),
                if (_discovered.isEmpty)
                  Text(
                    '（暂无发现设备；同网段设备运行本应用后约 3 秒内出现）',
                    style: Theme.of(context).textTheme.bodySmall,
                  )
                else
                  for (final device in _discovered)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 4),
                      child: Row(
                        children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  device.conflicted
                                      ? '${device.shortName ?? "（未命名）"}（短名冲突，无效）'
                                      : (device.shortName ?? '（未命名）'),
                                  style: Theme.of(context).textTheme.bodyMedium,
                                ),
                                SelectableText(
                                  '${device.hostname} · ${device.sourceIp}:${device.port}\n${device.uuid}',
                                  style: Theme.of(context).textTheme.bodySmall,
                                ),
                              ],
                            ),
                          ),
                          if (device.paired)
                            const Text('已配对')
                          else
                            FilledButton(
                              onPressed: _pairingInProgress.contains(device.uuid)
                                  ? null
                                  : () => _startPairing(device.uuid),
                              child: Text(
                                _pairingInProgress.contains(device.uuid)
                                    ? '配对中…'
                                    : '发起配对',
                              ),
                            ),
                        ],
                      ),
                    ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('已配对设备', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 8),
                if (_peerStatuses.isEmpty)
                  Text(
                    '（暂无已配对设备；可用「发起配对」或手工编辑 config.toml 的 [[peer]] 段）',
                    style: Theme.of(context).textTheme.bodySmall,
                  )
                else
                  for (final peer in _peerStatuses)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 4),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Padding(
                            padding: const EdgeInsets.only(top: 4),
                            child: _StatusDot(status: peer.status),
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  peer.conflicted
                                      ? '${peer.shortName ?? "（未命名）"}（短名冲突，无效；请改名或用 UUID）'
                                      : (peer.shortName ?? '（未命名）'),
                                  style: Theme.of(context).textTheme.bodyMedium,
                                ),
                                SelectableText(
                                  '${peer.address}:${peer.port} · ${peer.uuid}\n${_statusText(peer)}',
                                  style: Theme.of(context).textTheme.bodySmall,
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
                    ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  String _statusText(PeerStatusInfo peer) => switch (peer.status) {
        'online' => peer.note.isEmpty ? '在线' : '在线 · ${peer.note}',
        'unauthorized' =>
          peer.note.isEmpty ? '在线但凭据失效（需重新配对）' : peer.note,
        _ => peer.note.isEmpty ? '离线' : '离线（${peer.note}）',
      };
}

/// 已配对设备的状态圆点（绿=在线 / 橙=凭据失效 / 灰=离线）。
class _StatusDot extends StatelessWidget {
  const _StatusDot({required this.status});

  final String status;

  @override
  Widget build(BuildContext context) {
    final color = switch (status) {
      'online' => Colors.green,
      'unauthorized' => Colors.orange,
      _ => Colors.grey,
    };
    return Container(
      width: 10,
      height: 10,
      decoration: BoxDecoration(color: color, shape: BoxShape.circle),
    );
  }
}

/// 加载失败时的视图（如数据目录不可用）。
class _FatalErrorView extends StatelessWidget {
  const _FatalErrorView({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.error_outline, size: 40),
            const SizedBox(height: 12),
            Text(message, textAlign: TextAlign.center),
          ],
        ),
      ),
    );
  }
}

/// 关键错误提示（如服务端启动失败）。
class _ErrorCard extends StatelessWidget {
  const _ErrorCard({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Card(
      color: colors.errorContainer,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            Icon(Icons.error_outline, color: colors.onErrorContainer),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                text,
                style: TextStyle(color: colors.onErrorContainer),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 一次性提示（如配置损坏重建）。
class _NoticeCard extends StatelessWidget {
  const _NoticeCard({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Card(
      color: colors.tertiaryContainer,
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            Icon(Icons.info_outline, color: colors.onTertiaryContainer),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                text,
                style: TextStyle(color: colors.onTertiaryContainer),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 信息面板的一行：左标签、右值（值可选中复制）。
class _InfoRow extends StatelessWidget {
  const _InfoRow({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 96,
            child: Text(
              label,
              style: theme.textTheme.bodyMedium
                  ?.copyWith(color: theme.colorScheme.onSurfaceVariant),
            ),
          ),
          Expanded(child: SelectableText(value)),
        ],
      ),
    );
  }
}
