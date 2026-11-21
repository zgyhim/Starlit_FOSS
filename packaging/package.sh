#!/bin/sh
# 打包入口：按平台分发到 packaging/pm/<name>.sh
#
# 用法: ./packaging/package.sh <format>
#   freebsd | pkg   FreeBSD .pkg
#   tar | linux     Linux 便携 tar.gz
#   msi | windows   Windows MSI（需要 wixl）
#
# 新增包管理器：在 packaging/pm/ 下加 <name>.sh 即可，无需改这里。
set -eu

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)
PM_DIR=$HERE/pm

usage() {
  echo "usage: $0 <format>"
  echo
  echo "available formats:"
  for f in "$PM_DIR"/*.sh; do
    [ -f "$f" ] || continue
    printf '  %s\n' "$(basename "$f" .sh)"
  done
  echo
  echo "environment:"
  echo "  STARLIT_BIN     override the binary path"
  echo "  DIST            output directory (default: dist)"
  echo "  INCLUDE_MEDIA   1 to bundle media archives, 0 to skip"
}

[ $# -ge 1 ] || { usage; exit 1; }

case "$1" in
  freebsd|pkg)  exec sh "$PM_DIR/freebsd.sh" ;;
  tar|linux)    exec sh "$PM_DIR/tar.sh" ;;
  msi|windows)  exec sh "$PM_DIR/msi.sh" ;;
  -h|--help|list) usage ;;
  *)
    if [ -f "$PM_DIR/$1.sh" ]; then
      exec sh "$PM_DIR/$1.sh"
    fi
    echo "unknown format: $1" >&2
    usage
    exit 1
    ;;
esac
