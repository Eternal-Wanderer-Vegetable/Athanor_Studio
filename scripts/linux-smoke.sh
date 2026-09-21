#!/usr/bin/env bash
# Athanor Studio — Linux desktop smoke harness（对齐 scripts/windows-smoke.ps1）
# 真实桌面验收：依赖预检 → 构建 studio → xvfb 虚显启动 → 主窗口存活判定 → 干净退出。
# 供 Linux runner 与人工执行共用。用法：bash scripts/linux-smoke.sh [keep_seconds]
set -euo pipefail
KEEP_SECONDS="${1:-10}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exe="$root/engine/target/debug/studio"

for cmd in xvfb-run xdotool cargo; do
  command -v "$cmd" >/dev/null 2>&1 || {
    echo "缺少依赖: $cmd（Debian/Ubuntu: sudo apt install xvfb xdotool）" >&2
    exit 2
  }
done

echo "== Build studio =="
cargo build -p studio --manifest-path "$root/engine/Cargo.toml"
if [ ! -x "$exe" ]; then
  echo "studio 不存在: $exe" >&2
  exit 3
fi

echo "== Launch studio (keep ${KEEP_SECONDS}s) =="
export SMOKE_EXE="$exe" SMOKE_KEEP="$KEEP_SECONDS"
# 虚拟显示中启动：主窗口出现（xdotool 按 WM_NAME 检索）即视为桌面 shell 正常。
if ! xvfb-run -a -s "-screen 0 1280x820x24" bash -c '
  set -u
  "$SMOKE_EXE" & app=$!
  alive=0
  for _ in $(seq 1 "$SMOKE_KEEP"); do
    sleep 1
    kill -0 "$app" 2>/dev/null || break
    if xdotool search --name "^Athanor Studio$" >/dev/null 2>&1; then alive=1; break; fi
  done
  echo "WindowFound=$alive"
  if kill -0 "$app" 2>/dev/null; then
    kill -TERM "$app" 2>/dev/null || true
    for _ in $(seq 1 8); do
      kill -0 "$app" 2>/dev/null || break
      sleep 1
    done
    kill -KILL "$app" 2>/dev/null || true
  fi
  wait "$app" 2>/dev/null || true
  [ "$alive" -eq 1 ] || { echo "studio 启动失败：未出现主窗口" >&2; exit 4; }
'; then
  exit $?
fi
echo "SMOKE OK: studio 主窗口出现并正常关闭"
