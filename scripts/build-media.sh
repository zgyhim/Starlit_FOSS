#!/usr/bin/env bash
# 生成媒体资源包（tar.zst）与 manifest。
# 源目录可用 STARLIT_SRC 覆盖，默认指向解包后的原版安装目录。
set -euo pipefail

SRC="${STARLIT_SRC:-/home/zgyhim/Starlit-rc15-unpacked/resources/template/data}"
OUT="$(cd "$(dirname "$0")/.." && pwd)/resources/media"
ZSTD_LEVEL="${ZSTD_LEVEL:-19}"

mkdir -p "$OUT"
cd "$SRC"

pack() {
  local name="$1" ; shift
  echo "packing $name"
  tar -cf - "$@" | zstd "-${ZSTD_LEVEL}" --long=27 -T0 -q -f -o "$OUT/$name"
}

pack stickers-mygo.tar.zst -C stickers mygo
pack stickers-blue-archive.tar.zst -C stickers "blue archive"
pack stickers-kaguya.tar.zst -C stickers "超时空辉夜姬表情包"
pack stickers-stolen.tar.zst -C stickers stolen
pack avatars.tar.zst avatars character_avatars
pack groups.tar.zst groups

cd "$OUT"
python3 - <<'PY'
import hashlib, json, os
entries = [
  ("avatars.tar.zst", "data", "startup", "avatars,character_avatars"),
  ("groups.tar.zst", "data", "startup", "groups"),
  ("stickers-mygo.tar.zst", "data/stickers", "background", "mygo"),
  ("stickers-blue-archive.tar.zst", "data/stickers", "background", "blue archive"),
  ("stickers-kaguya.tar.zst", "data/stickers", "background", "超时空辉夜姬表情包"),
  ("stickers-stolen.tar.zst", "data/stickers", "background", "stolen"),
]
out = []
for name, target, scope, content in entries:
    h = hashlib.sha256()
    with open(name, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    out.append({"name": name, "target": target, "scope": scope,
                "sha256": h.hexdigest(), "bytes": os.path.getsize(name),
                "content": content})
json.dump({"version": 1, "archives": out},
          open("manifest.json", "w", encoding="utf-8"), ensure_ascii=False, indent=2)
print("manifest written")
PY
