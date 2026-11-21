#!/bin/sh
# FreeBSD .pkg 打包：依赖交给 pkg 解析，只装二进制与资源。
set -eu

. "$(dirname "$0")/../lib/common.sh"

PREFIX=/usr/local
LIBDIR=/lib/starlit-foss
STAGE=$WORK/stage
mkdir -p "$STAGE"

stage_app "$STAGE$PREFIX" "$LIBDIR"
mkdir -p "$STAGE$PREFIX/bin"
write_launcher "$STAGE$PREFIX/bin/starlit-foss" "$LIBDIR"

mkdir -p "$STAGE$PREFIX/share/applications"
cat > "$STAGE$PREFIX/share/applications/starlit-foss.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Starlit FOSS
Comment=Local-first AI companion
Exec=/usr/local/bin/starlit-foss
Terminal=false
Categories=Network;Chat;
DESKTOP

mkdir -p "$STAGE$PREFIX/share/licenses/starlit-foss"
cp "$ROOT/LICENSE" "$STAGE$PREFIX/share/licenses/starlit-foss/LICENSE"

MANIFEST=$STAGE/+MANIFEST
cat > "$MANIFEST" <<EOF
name: starlit-foss
origin: misc/starlit-foss
version: "$VERSION"
comment: "Local-first AI companion"
www: "https://github.com/zgyhim/Starlit_FOSS"
maintainer: "zgyhim <noreply@github.com>"
prefix: $PREFIX
categories: [network, chat]
licenselogic: single
licenses: [AGPL-3.0]
desc: <<EOD
Starlit FOSS is a local-first chat companion built with Tauri 2.
It keeps chat history, memory and personas on the local machine and
talks to any OpenAI-compatible model endpoint.
EOD
deps: {
  webkit2-gtk_41: {origin: "www/webkit2-gtk_41"},
  gtk3: {origin: "x11-toolkits/gtk30"},
  libsoup3: {origin: "devel/libsoup3"},
  dbus: {origin: "devel/dbus"}
}
EOF

PLIST=$STAGE/+PLIST
(cd "$STAGE$PREFIX" && find . -type f | sed 's|^\./||' | sort) > "$PLIST"

OUT=$DIST/starlit-foss-$VERSION.pkg
pkg create -M "$MANIFEST" -p "$PLIST" -r "$STAGE" -o "$DIST" >/dev/null
# pkg create 会按 name-version 命名，统一成带 starlit-foss 前缀
for candidate in "$DIST"/starlit-foss-"$VERSION"*.pkg; do
  [ -f "$candidate" ] || continue
  mv "$candidate" "$OUT"
done

echo "pkg: $OUT"
ls -lh "$OUT"
