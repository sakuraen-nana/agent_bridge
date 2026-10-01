# agent-bridge Windows 安装器构建（在 Windows 上执行）
#
# 前置：Flutter SDK、Rust 工具链、Inno Setup 6（ISCC.exe）
#   winget install JRSoftware.InnoSetup
# 产物：dist\agent-bridge-<版本>-windows-x64-setup.exe
#
# 版本号取自 rust\Cargo.toml 的 package.version（与 Linux 构建同源）。

param(
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"

$AppRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$RepoRoot = Resolve-Path (Join-Path $AppRoot "..")

# 版本：单一版本源
$cargoToml = Get-Content (Join-Path $AppRoot "rust\Cargo.toml") -Raw
$version = [regex]::Match($cargoToml, '(?m)^version = "([^"]+)"').Groups[1].Value
if (-not $version) { throw "未能从 rust\Cargo.toml 解析版本号" }
Write-Host "[build] 版本: $version"

Push-Location $AppRoot
try {
    Write-Host "[build] flutter build windows --release"
    flutter build windows --release

    Write-Host "[build] cargo build --release --bin agent-bridge"
    cargo build --release --manifest-path rust\Cargo.toml --bin agent-bridge

    # 定位 ISCC
    $iscc = (Get-Command iscc.exe -ErrorAction SilentlyContinue)?.Source
    if (-not $iscc) {
        $candidate = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
        if (Test-Path $candidate) { $iscc = $candidate }
    }
    if (-not $iscc) {
        throw "未找到 Inno Setup 6 的 ISCC.exe；请先安装: winget install JRSoftware.InnoSetup"
    }

    Write-Host "[build] $iscc /DAppVersion=$version agent-bridge.iss"
    & $iscc "/DAppVersion=$version" (Join-Path $PSScriptRoot "agent-bridge.iss")
    if ($LASTEXITCODE -ne 0) { throw "ISCC 构建失败（退出码 $LASTEXITCODE）" }

    $out = Join-Path $AppRoot "dist\agent-bridge-$version-windows-x64-setup.exe"
    if (-not (Test-Path $out)) { throw "未找到预期安装器产物: $out" }
    Write-Host "[build] 完成: $out"
} finally {
    Pop-Location
}
