#!/usr/bin/env bash
# 获取媒体包：优先使用本地完整文件/分片，否则从 Release 下载。
# 可用 STARLIT_MEDIA_URL 覆盖 manifest 中的 base_url。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MEDIA="$ROOT/resources/media"
MANIFEST="$MEDIA/manifest.json"

if [ ! -f "$MANIFEST" ]; then
  echo "缺少 $MANIFEST" >&2
  exit 1
fi

BASE_URL="${STARLIT_MEDIA_URL:-$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1])).get("base_url",""))' "$MANIFEST")}"
BASE_URL="${BASE_URL%/}"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

verify() {
  local file="$1" expected="$2"
  local actual
  actual="$(sha256_of "$file")"
  if [ "$actual" != "$expected" ]; then
    echo "校验失败：$file" >&2
    echo "  期望 $expected" >&2
    echo "  实际 $actual" >&2
    exit 1
  fi
}

python3 -c '
import json,sys
for item in json.load(open(sys.argv[1]))["archives"]:
    print(item["name"], item["sha256"])
' "$MANIFEST" | while read -r name sha; do
  target="$MEDIA/$name"
  if [ -f "$target" ]; then
    verify "$target" "$sha"
    echo "已存在：$name"
    continue
  fi

  shopt -s nullglob
  parts=("$target".*)
  shopt -u nullglob
  if [ ${#parts[@]} -gt 0 ]; then
    echo "合并分片：$name"
    tmp="$target.tmp"
    cat $(printf '%s\n' "${parts[@]}" | sort) > "$tmp"
    verify "$tmp" "$sha"
    mv "$tmp" "$target"
    rm -f "${parts[@]}"
    echo "完成：$name"
    continue
  fi

  if [ -z "$BASE_URL" ]; then
    echo "缺少 $name，且未配置下载地址。" >&2
    echo "请设置 STARLIT_MEDIA_URL，或把媒体包放到 $MEDIA/" >&2
    exit 1
  fi

  echo "下载：$BASE_URL/$name"
  tmp="$target.tmp"
  curl -fL --retry 3 --retry-delay 2 -o "$tmp" "$BASE_URL/$name"
  verify "$tmp" "$sha"
  mv "$tmp" "$target"
  echo "完成：$name"
done
