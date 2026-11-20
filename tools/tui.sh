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

BOX_INNER=66

fit_text() {
  local text="$1"
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$text" "$BOX_INNER" <<'PY'
import sys, unicodedata
text, width = sys.argv[1], int(sys.argv[2])

def cell(ch):
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1

out = ""
used = 0
for ch in text:
    if used + cell(ch) > width - 1:
        out += "…"
        used += 1
        break
    out += ch
    used += cell(ch)
print(out + " " * (width - used))
PY
  else
    printf '%-*s' "$BOX_INNER" "$text"
  fi
}

box_rule() {
  local count=$((BOX_INNER + 2)) i
  for ((i = 0; i < count; i++)); do printf '═'; done
}
box_top()    { printf '╔'; box_rule; printf '╗\n'; }
box_sep()    { printf '╠'; box_rule; printf '╣\n'; }
box_bottom() { printf '╚'; box_rule; printf '╝\n'; }
box_line()   { printf '║ %s ║\n' "$(fit_text "$1")"; }
box_begin()  { box_top; box_line "$1"; box_sep; }
box_end()    { box_bottom; }

export_logs() {
  if [ ! -d "$LOG_DIR" ]; then
    box_begin "导出日志"
    box_line "日志目录不存在"
    box_line "$LOG_DIR"
    box_end
    pause; return
  fi

  local out
  if [ "$NONINTERACTIVE" -eq 1 ]; then
    out="$PWD"
  else
    read -r -p "输出目录（默认当前目录）: " out || true
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

  box_begin "导出日志"
  box_line "日志目录: $LOG_DIR"
  box_line "输出文件: $target"
  box_line "大小: $(du -h "$target" | cut -f1)"
  box_end
  pause
}

install_webview2() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      if command -v powershell.exe >/dev/null 2>&1; then
        box_begin "安装 WebView2"
        box_line "平台: Windows 兼容环境"
        box_line "转交 PowerShell 工具"
        box_end
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_DIR/tui.ps1" -Action webview2
      else
        box_begin "安装 WebView2"
        box_line "未找到 powershell.exe"
        box_end
      fi
      ;;
    *)
      box_begin "安装 WebView2"
      box_line "当前平台: $(uname -s)"
      box_line "WebView2 仅适用于 Windows"
      box_line "Linux 使用 WebKitGTK，无需安装"
      box_end
      ;;
  esac
  pause
}

check_integrity() {
  local -a results=()
  local failures=0

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
    results+=("[OK] 可执行文件: $exe")
  else
    results+=("[FAIL] 未找到应用可执行文件")
    failures=$((failures + 1))
  fi

  local manifest="$MEDIA_DIR/manifest.json"
  if [ ! -f "$manifest" ]; then
    results+=("[FAIL] 缺少媒体清单")
    failures=$((failures + 1))
  elif ! command -v python3 >/dev/null 2>&1; then
    results+=("[WARN] 未找到 python3，跳过媒体校验")
  else
    local summary
    summary="$(python3 - "$manifest" "$MEDIA_DIR" <<'PY'
import hashlib, json, os, sys
manifest, media = sys.argv[1], sys.argv[2]
data = json.load(open(manifest, encoding="utf-8"))
total = missing = bad = 0
for entry in data.get("archives", []):
    total += 1
    path = os.path.join(media, entry["name"])
    if not os.path.isfile(path):
        missing += 1
        continue
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    if h.hexdigest() != entry.get("sha256"):
        bad += 1
if missing == 0 and bad == 0:
    print(f"[OK] 媒体包完整: {total} 个")
else:
    print(f"[FAIL] 媒体包异常: 缺失 {missing}, 校验失败 {bad}, 共 {total}")
PY
)"
    results+=("$summary")
    case "$summary" in
      "[FAIL]"*) failures=$((failures + 1)) ;;
    esac
  fi

  local db="$DATA_ROOT/data/companion.db"
  if [ -f "$db" ] && command -v sqlite3 >/dev/null 2>&1; then
    local result
    result="$(sqlite3 "$db" 'PRAGMA integrity_check;' 2>/dev/null | head -n1 || true)"
    if [ "$result" = "ok" ]; then
      results+=("[OK] 数据库完整性: ok")
    else
      results+=("[FAIL] 数据库完整性: ${result:-unknown}")
      failures=$((failures + 1))
    fi
  else
    results+=("[WARN] 跳过数据库检查")
  fi

  if [ -d "$LOG_DIR" ]; then
    results+=("[OK] 日志目录: $LOG_DIR")
  else
    results+=("[WARN] 日志目录尚未创建")
  fi

  box_begin "检查文件完整性"
  box_line "[INFO] 应用目录: $APP_ROOT"
  box_line "[INFO] 数据目录: $DATA_ROOT"
  local line
  for line in "${results[@]}"; do
    box_line "$line"
  done
  box_sep
  if [ "$failures" -eq 0 ]; then
    box_line "结果: 全部检查通过"
  else
    box_line "结果: 存在 $failures 项问题"
  fi
  box_end
  pause
}

main_menu() {
  while true; do
    clear 2>/dev/null || true
    box_top
    box_line "星伴 FOSS TUI 工具"
    box_sep
    box_line "应用目录: $APP_ROOT"
    box_line "数据目录: $DATA_ROOT"
    box_sep
    box_line "1  导出日志文件夹"
    box_line "2  安装 WebView2（Windows）"
    box_line "3  检查文件完整性"
    box_line "0  退出"
    box_bottom
    read -r -p "请选择: " choice || exit 0
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
