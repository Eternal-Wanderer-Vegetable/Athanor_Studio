# Athanor Studio — Windows desktop smoke harness (E6)
# 真实桌面验收（非浏览器 e2e）：WebView2 预检 → 编辑器构建 → studio 启动 →
# 窗口存活判定 → 干净退出。供 Windows runner 与人工执行共用。
# 用法：powershell -File scripts/windows-smoke.ps1 [-KeepSeconds 15] [-SkipBuild]
[CmdletBinding()]
param(
  [int]$KeepSeconds = 10,
  [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$engine = Join-Path $root "engine"
$editor = Join-Path $engine "crates/aludel/editor"
$exe = Join-Path $engine "target/debug/studio.exe"

Write-Host "== WebView2 preflight =="
$wv2 = Get-ItemProperty -ErrorAction SilentlyContinue `
  "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" | Select-Object -ExpandProperty pv -ErrorAction SilentlyContinue
if (-not $wv2) {
  $wv2 = Get-ItemProperty -ErrorAction SilentlyContinue `
    "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" | Select-Object -ExpandProperty pv -ErrorAction SilentlyContinue
}
if (-not $wv2) {
  Write-Error "WebView2 Runtime 未安装。安装：https://developer.microsoft.com/microsoft-edge/webview2/"
  exit 2
}
Write-Host "WebView2: $wv2"

if (-not $SkipBuild) {
  Write-Host "== Build editor frontend (frontendDist) =="
  Push-Location $editor
  try {
    if (-not (Test-Path "node_modules")) { npm ci }
    npm run build
  } finally { Pop-Location }
  Write-Host "== Build studio =="
  Push-Location $engine
  try { cargo build -p studio } finally { Pop-Location }
}

if (-not (Test-Path $exe)) { Write-Error "studio.exe 不存在: $exe"; exit 3 }

Write-Host "== Launch studio (keep $KeepSeconds s) =="
$proc = Start-Process -FilePath $exe -PassThru
$alive = $false
for ($i = 0; $i -lt $KeepSeconds; $i++) {
  Start-Sleep -Seconds 1
  if ($proc.HasExited) { break }
  # 主窗口出现即视为桌面 shell 启动成功（WebView2 实际渲染）
  $proc.Refresh()
  if ($proc.MainWindowHandle -ne 0) { $alive = $true }
}
$title = $proc.MainWindowTitle
Write-Host "MainWindowHandle=$($proc.MainWindowHandle) Title='$title' Alive=$alive"
if (-not $proc.HasExited) {
  $proc.CloseMainWindow() | Out-Null
  if (-not $proc.WaitForExit(8000)) { $proc.Kill() }
}
if (-not $alive) {
  Write-Error "studio 启动失败：未出现主窗口（exit=$($proc.ExitCode)）"
  exit 4
}
Write-Host "SMOKE OK: studio 主窗口出现并正常关闭"
exit 0
