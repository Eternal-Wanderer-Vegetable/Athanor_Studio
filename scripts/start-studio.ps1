# Athanor Studio — Windows 一键启动（G4）
# 依赖预检 → 前端依赖安装（锁文件）→ 前端构建 → 启动 studio。
# 任何子进程失败即停止，不会带着旧 dist 启动。
# 用法：powershell -NoProfile -ExecutionPolicy Bypass -File scripts/start-studio.ps1 [-SkipFrontendBuild] [-Dev]
[CmdletBinding()]
param(
  [switch]$SkipFrontendBuild,
  [switch]$Dev
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$engine = Join-Path $root "engine"
$editor = Join-Path $engine "crates/aludel/editor"

function Fail([string]$msg, [int]$code = 1) {
  Write-Error $msg
  exit $code
}

Write-Host "== 依赖预检 ==" -ForegroundColor Cyan
$node = Get-Command node -ErrorAction SilentlyContinue
if (-not $node) { Fail "未找到 Node.js——请先安装 Node（建议 LTS），再运行本脚本。" 2 }
Write-Host "Node: $(& node --version)"
$npm = Get-Command npm -ErrorAction SilentlyContinue
if (-not $npm) { Fail "未找到 npm——请检查 Node 安装。" 2 }
Write-Host "npm: $(& npm --version)"
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) { Fail "未找到 Rust/cargo——请安装 rustup 与 MSVC 工具链。" 2 }
Write-Host "cargo: $(& cargo --version)"

$wv2 = Get-ItemProperty -ErrorAction SilentlyContinue `
  "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" | Select-Object -ExpandProperty pv -ErrorAction SilentlyContinue
if (-not $wv2) {
  $wv2 = Get-ItemProperty -ErrorAction SilentlyContinue `
    "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" | Select-Object -ExpandProperty pv -ErrorAction SilentlyContinue
}
if (-not $wv2) { Fail "WebView2 Runtime 未安装——安装后重试：https://developer.microsoft.com/microsoft-edge/webview2/" 2 }
Write-Host "WebView2: $wv2"

if (-not $SkipFrontendBuild) {
  Write-Host "== 前端依赖（锁文件安装）==" -ForegroundColor Cyan
  Push-Location $editor
  try {
    if (-not (Test-Path "node_modules")) { npm ci; if ($LASTEXITCODE -ne 0) { Fail "npm ci 失败" } }
    Write-Host "== 前端构建（tsc + vite singlefile → dist/index.html）==" -ForegroundColor Cyan
    npm run build
    if ($LASTEXITCODE -ne 0) { Fail "前端构建失败——不启动旧 dist" }
  } finally { Pop-Location }
}

$dist = Join-Path $editor "dist/index.html"
if (-not (Test-Path $dist)) { Fail "缺少前端产物 dist/index.html——先运行前端构建（或去掉 -SkipFrontendBuild）。" 3 }

Write-Host "== 启动 studio ==" -ForegroundColor Cyan
Push-Location $engine
try {
  if ($Dev) {
    cargo run -p studio
  } else {
    cargo build -p studio
    if ($LASTEXITCODE -ne 0) { Fail "studio 构建失败" }
    $exe = Join-Path $engine "target/debug/studio.exe"
    & $exe
  }
  if ($LASTEXITCODE -ne 0) { Fail "studio 退出码 $LASTEXITCODE" $LASTEXITCODE }
} finally { Pop-Location }
