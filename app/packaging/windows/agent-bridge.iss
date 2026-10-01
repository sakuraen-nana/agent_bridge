; agent-bridge Windows 安装器（Inno Setup 6）
;
; 在 Windows 上由 build.ps1 调用（会以 /DAppVersion=<版本> 注入版本号）。
; 要点：
;   - 安装应用束（flutter build windows --release）与 CLI（cargo --bin agent-bridge）
;   - 把安装目录加入「用户级 PATH」：安装前记录原值、卸载恢复（不误删用户自己的项）
;   - 运行期管理员提权由应用执行清单触发 UAC（安装器本身以管理员运行，便于 Program Files 写入）
;
; 预期行为（验收对照）：
;   1) 安装后新开 cmd：`agent-bridge --version` 输出与安装产物一致的版本
;   2) 卸载后：安装目录、PATH 条目、快捷方式均被移除

#ifndef AppVersion
  #error 必须通过 /DAppVersion=<版本> 传入版本号（build.ps1 会处理）
#endif

#define AppName "agent-bridge"

[Setup]
AppId={{8E31B2B7-2E0A-4B8C-9F41-3AA0C6B6C7D1}
AppName={#AppName}
AppVersion={#AppVersion}
DefaultDirName={autopf}\agent-bridge
DefaultGroupName={#AppName}
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=agent-bridge-{#AppVersion}-windows-x64-setup
SetupIconFile=..\icons\agent-bridge.ico
UninstallDisplayIcon={app}\agent_bridge_app.exe
Compression=lzma2
SolidCompression=yes
DisableProgramGroupPage=yes

[Files]
; 应用束（Release 目录全部内容；lib/、data/ 一并）
Source: "..\..\build\windows\x64\runner\Release\*"; DestDir: "{app}"; Flags: recursesubdirs createallsubdirs ignoreversion
; CLI
Source: "..\..\rust\target\release\agent-bridge.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\agent_bridge_app.exe"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\agent_bridge_app.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; GroupDescription: "附加任务:"

[Code]
const
  EnvironmentKey = 'Environment';
  BackupValueName = 'agent-bridge-PathBackup';

// 读取用户 PATH（HKCU\Environment\Path）
function GetUserPath(): string;
begin
  if not RegQueryStringValue(HKCU, EnvironmentKey, 'Path', Result) then
    Result := '';
end;

// 是否需要追加（避免重复加入）
function NeedsAddPath(Param: string): boolean;
var
  Path: string;
begin
  Path := GetUserPath();
  Result := Pos(';' + Uppercase(Param) + ';', ';' + Uppercase(Path) + ';') = 0;
end;

procedure WriteUserPath(Value: string);
begin
  RegWriteExpandStringValue(HKCU, EnvironmentKey, 'Path', Value);
end;

procedure AddToUserPath();
var
  OldPath, NewPath: string;
begin
  OldPath := GetUserPath();
  // 记录原值（卸载时恢复；如原本无 Path 值，记为哨兵 <none>）
  if OldPath = '' then
    RegWriteStringValue(HKCU, EnvironmentKey, BackupValueName, '<none>')
  else
    RegWriteStringValue(HKCU, EnvironmentKey, BackupValueName, OldPath);
  if NeedsAddPath(ExpandConstant('{app}')) then
  begin
    if OldPath = '' then
      NewPath := ExpandConstant('{app}')
    else
      NewPath := OldPath + ';' + ExpandConstant('{app}');
    WriteUserPath(NewPath);
    // 通知已运行的程序环境变更（新开 cmd 会读取新值）
    SendBroadcastMessage(0, 0, 0, 0);
  end;
end;

procedure RemoveFromUserPath();
var
  Path, Backup, AppDir, NewPath: string;
  P: Integer;
begin
  Path := GetUserPath();
  AppDir := ExpandConstant('{app}');
  P := Pos(';' + Uppercase(AppDir), Uppercase(Path));
  if P > 0 then
  begin
    Delete(Path, P, Length(AppDir) + 1); // 去掉「;<app>」
    if Path = AppDir then Path := '';    // 极端：仅剩自身
    WriteUserPath(Path);
  end
  else if Path = AppDir then
    WriteUserPath('');

  // 恢复安装前记录的原值（更稳妥：若备份存在，以其为准）
  if RegQueryStringValue(HKCU, EnvironmentKey, BackupValueName, Backup) then
  begin
    if Backup = '<none>' then
    begin
      RegDeleteValue(HKCU, EnvironmentKey, 'Path');
    end
    else
      WriteUserPath(Backup);
    RegDeleteValue(HKCU, EnvironmentKey, BackupValueName);
  end;
  SendBroadcastMessage(0, 0, 0, 0);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
    AddToUserPath();
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    RemoveFromUserPath();
end;
