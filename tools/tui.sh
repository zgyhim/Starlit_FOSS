#!/usr/bin/env bash
# 星伴 FOSS TUI 工具（Linux / macOS）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NONINTERACTIVE=0
case "${1:-}" in --*) NONINTERACTIVE=1 ;; esac
APP_ROOT="${STARLIT_APP_ROOT:-$(cd "$SCRIPT_DIR/.." && pwd)}"
DATA_ROOT="${STARLIT_DATA_ROOT:-$HOME/.local/share/starlit-desktop}"
LOG_DIR="$DATA_ROOT/log"
RES_DIR="$APP_ROOT/resources"
MEDIA_DIR="$RES_DIR/media"

C_RESET=$'\033[0m'
C_BOLD=$'\033[1m'
C_CYAN=$'\033[36m'
C_GREEN=$'\033[32m'
C_YELLOW=$'\033[33m'
C_RED=$'\033[31m'

say()  { printf '%s\n' "$*"; }
ok()   { printf '%s\n' "${C_GREEN}[OK]${C_RESET} $*"; }
warn() { printf '%s\n' "${C_YELLOW}[WARN]${C_RESET} $*"; }
err()  { printf '%s\n' "${C_RED}[FAIL]${C_RESET} $*"; }
title(){ printf '\n%s\n' "${C_BOLD}${C_CYAN}$*${C_RESET}"; }

pause() { read -r -p "按 Enter 返回菜单…" _ || true; }

export_logs() {
  title "导出日志"
  if [ ! -d "$LOG_DIR" ]; then
    err "日志目录不存在：$LOG_DIR"
    pause; return
  fi
  local out
  if [ "$NONINTERACTIVE" -eq 1 ]; then
    out="$PWD"
  else
    read -r -p "输出目录（默认当前目录）：" out || true
    out="${out:-$PWD}"
  fi
  mkdir -p "$out"
  local stamp name target
  stamp="$(date +%Y%m%d-%H%M%S)"
  if command -v zstd >/dev/null 2>&1; then
    name="starlit-logs-$stamp.tar.zst"
    target="$out/$name"
    tar -C "$LOG_DIR" -cf - . | zstd -19 -T0 -q -f -o "$target"
  else
    name="starlit-logs-$stamp.tar.gz"
    target="$out/$name"
    tar -C "$LOG_DIR" -czf "$target" .
  fi
  ok "已导出：$target"
  ls -lh "$target"
  pause
}

install_webview2() {
  title "安装 WebView2"
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      say "检测到 Windows 兼容环境，转交 PowerShell 脚本。"
      if command -v powershell.exe >/dev/null 2>&1; then
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_DIR/tui.ps1" -Action webview2
      else
        err "找不到 powershell.exe"
      fi
      ;;
    *)
      warn "WebView2 仅适用于 Windows。"
      say "Linux 使用 WebKitGTK，无需安装 WebView2。"
      ;;
  esac
  pause
}

check_integrity() {
  title "检查文件完整性"
  local failures=0

  say "应用目录：$APP_ROOT"
  say "数据目录：$DATA_ROOT"

  local exe=""
  for candidate in \
    "$APP_ROOT/bin/starlit-foss" \
    "$APP_ROOT/starlit-foss" \
    "$APP_ROOT/bin/StarlitFOSS" \
    "$APP_ROOT/StarlitFOSS" \
    "$APP_ROOT/src-tauri/target/release/starlit-foss" \
    "$APP_ROOT/src-tauri/target/debug/starlit-foss"; do
    if [ -f "$candidate" ]; then exe="$candidate"; break; fi
  done
  if [ -n "$exe" ]; then
    ok "可执行文件：$exe"
  else
    err "未找到应用可执行文件"
    failures=$((failures + 1))
  fi

  local manifest="$MEDIA_DIR/manifest.json"
  if [ ! -f "$manifest" ]; then
    err "缺少媒体清单：$manifest"
    failures=$((failures + 1))
  elif ! command -v python3 >/dev/null 2>&1; then
    warn "未找到 python3，跳过媒体 SHA-256 校验"
  else
    local total missing bad
    read -r total missing bad < <(python3 - "$manifest" "$MEDIA_DIR" <<'PY'
import hashlib, json, os, sys
manifest, media = sys.argv[1], sys.argv[2]
data = json.load(open(manifest, encoding="utf-8"))
total = missing = bad = 0
for entry in data.get("archives", []):
    total += 1
    path = os.path.join(media, entry["name"])
    if not os.path.isfile(path):
        missing += 1
        print(f"MISSING {entry['name']}", file=sys.stderr)
        continue
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    if h.hexdigest() != entry.get("sha256"):
        bad += 1
        print(f"BAD {entry['name']}", file=sys.stderr)
print(total, missing, bad)
PY
)
    if [ "${missing:-0}" -eq 0 ] && [ "${bad:-0}" -eq 0 ]; then
      ok "媒体包完整：$total 个"
    else
      err "媒体包异常：缺失 $missing，校验失败 $bad（共 $total）"
      failures=$((failures + 1))
    fi
  fi

  local db="$DATA_ROOT/data/companion.db"
  if [ -f "$db" ] && command -v sqlite3 >/dev/null 2>&1; then
    local result
    result="$(sqlite3 "$db" 'PRAGMA integrity_check;' 2>/dev/null | head -n1 || true)"
    if [ "$result" = "ok" ]; then
      ok "数据库完整性：ok"
    else
      err "数据库完整性检查失败：${result:-unknown}"
      failures=$((failures + 1))
    fi
  else
    warn "跳过数据库检查（文件或 sqlite3 不存在）"
  fi

  if [ -d "$LOG_DIR" ]; then
    ok "日志目录：$LOG_DIR"
  else
    warn "日志目录尚未创建"
  fi

  if [ "$failures" -eq 0 ]; then
    printf '\n%s\n' "${C_GREEN}全部检查通过。${C_RESET}"
  else
    printf '\n%s\n' "${C_RED}存在 $failures 项问题。${C_RESET}"
  fi
  pause
}

main_menu() {
  while true; do
    clear 2>/dev/null || true
    printf '%s\n' "${C_BOLD}${C_CYAN}星伴 FOSS · TUI 工具${C_RESET}"
    printf '%s\n' "应用目录：$APP_ROOT"
    printf '%s\n' "数据目录：$DATA_ROOT"
    printf '%s\n' "----------------------------------------"
    printf '%s\n' "1) 导出日志文件夹"
    printf '%s\n' "2) 安装 WebView2（Windows）"
    printf '%s\n' "3) 检查文件完整性"
    printf '%s\n' "0) 退出"
    read -r -p "请选择： " choice || exit 0
    case "$choice" in
      1) export_logs ;;
      2) install_webview2 ;;
      3) check_integrity ;;
      0) exit 0 ;;
      *) warn "无效选项" ;;
    esac
  done
}

case "${1:-}" in
  --export) export_logs ;;
  --check) check_integrity ;;
  --webview2) install_webview2 ;;
  -h|--help) say "用法：$0 [--export|--check|--webview2]" ;;
  *) main_menu ;;
esac
