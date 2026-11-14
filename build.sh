#!/usr/bin/env bash
# 构建星伴 FOSS：先准备媒体包，再交给 Tauri 打包。
set -euo pipefail
cd "$(dirname "$0")"

./scripts/fetch-media.sh

if [ ! -d node_modules ]; then
  npm install
fi

npm run build "$@"
