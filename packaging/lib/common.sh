#!/bin/sh
# 共用打包函数：由 packaging/package.sh 调用，不单独执行。
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
VERSION=$(grep -o '"version"[[:space:]]*:[[:space:]]*"[^"]*"' "$ROOT/src-tauri/tauri.conf.json" | head -n1 | sed 's/.*"\([^"]*\)"$/\1/')
[ -n "$VERSION" ] || { echo "cannot read version from tauri.conf.json" >&2; exit 1; }

BIN=${STARLIT_BIN:-$ROOT/src-tauri/target/release/starlit-foss}
DIST=${DIST:-$ROOT/dist}
MEDIA_DIR=$ROOT/resources/media
TEMPLATE_DIR=$ROOT/resources/template
INCLUDE_MEDIA=${INCLUDE_MEDIA:-1}

mkdir -p "$DIST"

# 每次打包使用独立临时目录，退出（含中断）时清理，避免污染源码树和残留缓存。
WORK=$(mktemp -d "${TMPDIR:-/tmp}/starlit-package.XXXXXX") || {
  echo "cannot create temporary directory" >&2
  exit 1
}
cleanup() {
  rm -rf "$WORK"
}
trap 'cleanup' EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM
trap 'cleanup; exit 129' HUP

require_file() {
  [ -f "$1" ] || { echo "missing $1" >&2; exit 1; }
}

stage_app() {
  # $1 = staging root（不含 prefix）, $2 = lib 目录（相对 prefix）
  stage=$1
  libdir=$2
  require_file "$BIN"
  mkdir -p "$stage$libdir/resources/media"
  cp "$BIN" "$stage$libdir/starlit-foss"
  cp -R "$TEMPLATE_DIR" "$stage$libdir/resources/template"
  cp "$MEDIA_DIR/manifest.json" "$stage$libdir/resources/media/manifest.json"
  if [ "$INCLUDE_MEDIA" = 1 ]; then
    for archive in "$MEDIA_DIR"/*.tar.zst; do
      [ -f "$archive" ] || continue
      cp "$archive" "$stage$libdir/resources/media/"
    done
  fi
}

write_launcher() {
  # $1 = path, $2 = lib dir（相对 prefix）
  cat > "$1" <<LAUNCH
#!/bin/sh
export STARLIT_RESOURCE_ROOT=/usr/local$2/resources
exec /usr/local$2/starlit-foss "\$@"
LAUNCH
  chmod 755 "$1"
}
