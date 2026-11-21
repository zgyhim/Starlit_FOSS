#!/bin/sh
# Linux 便携 tar.gz：自带资源，解压即用。
set -eu

. "$(dirname "$0")/../lib/common.sh"

NAME=StarlitFOSS-$VERSION-linux-x86_64
STAGE=$WORK/stage
mkdir -p "$STAGE"

APP=$STAGE/$NAME
require_file "$BIN"
mkdir -p "$APP/bin" "$APP/resources"
cp "$BIN" "$APP/bin/starlit-foss"
cp -R "$TEMPLATE_DIR" "$APP/resources/template"
mkdir -p "$APP/resources/media"
cp "$MEDIA_DIR/manifest.json" "$APP/resources/media/manifest.json"
if [ "$INCLUDE_MEDIA" = 1 ]; then
  for archive in "$MEDIA_DIR"/*.tar.zst; do
    [ -f "$archive" ] || continue
    cp "$archive" "$APP/resources/media/"
  done
fi
cp "$ROOT/LICENSE" "$ROOT"/README*.md "$APP/" 2>/dev/null || cp "$ROOT/LICENSE" "$APP/"

cat > "$APP/starlit-foss" <<'LAUNCH'
#!/bin/sh
DIR="$(cd "$(dirname "$0")" && pwd)"
export STARLIT_RESOURCE_ROOT="$DIR/resources"
exec "$DIR/bin/starlit-foss" "$@"
LAUNCH
chmod 755 "$APP/starlit-foss" "$APP/bin/starlit-foss"

OUT=$DIST/$NAME.tar.gz
tar -C "$STAGE" -czf "$OUT" "$NAME"
sha256sum "$OUT" > "$OUT.sha256" 2>/dev/null || sha256 -r "$OUT" > "$OUT.sha256"

echo "tar: $OUT"
ls -lh "$OUT"
