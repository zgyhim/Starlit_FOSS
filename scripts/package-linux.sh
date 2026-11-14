#!/usr/bin/env bash
# 生成 Linux x86_64 便携包（含媒体资源）。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="$(python3 -c 'import json;print(json.load(open("'"$ROOT"'/src-tauri/tauri.conf.json"))["version"])')"
NAME="StarlitFOSS-${VERSION}-linux-x86_64"
OUT="$ROOT/dist/$NAME"
BIN="$ROOT/src-tauri/target/release/starlit-foss"

if [ ! -x "$BIN" ]; then
  echo "缺少 release 二进制：$BIN" >&2
  exit 1
fi

rm -rf "$OUT"
mkdir -p "$OUT/bin" "$OUT/resources"
cp "$BIN" "$OUT/bin/starlit-foss"
cp -r "$ROOT/resources/template" "$OUT/resources/template"
cp -r "$ROOT/resources/media" "$OUT/resources/media"
cp "$ROOT/LICENSE" "$OUT/LICENSE"
cp "$ROOT/README.md" "$OUT/README.md"

cat > "$OUT/starlit-foss" <<'LAUNCH'
#!/bin/sh
DIR="$(cd "$(dirname "$0")" && pwd)"
export STARLIT_RESOURCE_ROOT="$DIR/resources"
exec "$DIR/bin/starlit-foss" "$@"
LAUNCH
chmod +x "$OUT/starlit-foss" "$OUT/bin/starlit-foss"

mkdir -p "$ROOT/dist"
tar -C "$ROOT/dist" -czf "$ROOT/dist/$NAME.tar.gz" "$NAME"
sha256sum "$ROOT/dist/$NAME.tar.gz" > "$ROOT/dist/$NAME.tar.gz.sha256"
echo "打包完成：$ROOT/dist/$NAME.tar.gz"
