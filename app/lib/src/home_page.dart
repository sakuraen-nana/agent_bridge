import 'package:flutter/material.dart';

import 'bridge_service.dart';
import 'rust/api/init.dart' show AppSnapshot;

/// 主界面：启动信息面板 + 本机默认短名设置。
class HomePage extends StatefulWidget {
  const HomePage({super.key, required this.service});

  final BridgeService service;

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  AppSnapshot? _snapshot;
  String? _fatalError;
  final TextEditingController _shortNameController = TextEditingController();

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
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
        if (snapshot.notice != null) _NoticeCard(text: snapshot.notice!),
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
      ],
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
