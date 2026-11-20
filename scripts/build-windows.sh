#!/usr/bin/env bash
# Windows 交叉编译（GNU 目标，仅生成 exe，不打包安装器）
set -Eeuo pipefail
cd "$(dirname "$0")/.."
mkdir -p dist
LOG="dist/windows-cross-build.log"
: > "$LOG"
exec > >(tee -a "$LOG") 2>&1

step() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }
fail() {
  printf '\n\033[1;31m构建失败：%s\033[0m\n' "$*"
  printf '日志：%s\n' "$LOG"
  printf '\n按 Enter 关闭窗口…'
  read -r _
  exit 1
}

step "环境检查"
echo "工作目录: $(pwd)"
echo "rustc:    $(rustc --version)"
echo "cargo:    $(cargo --version)"
echo "target:   x86_64-pc-windows-gnu"
rustup target list --installed | grep -q '^x86_64-pc-windows-gnu$' || fail "未安装 x86_64-pc-windows-gnu"
command -v x86_64-w64-mingw32-gcc >/dev/null || fail "缺少 mingw-w64"
echo "mingw:    $(x86_64-w64-mingw32-gcc --version | head -n1)"

step "开始构建（命令会逐行显示，不隐藏输出）"
set -x
npx tauri build --no-bundle --target x86_64-pc-windows-gnu --ci
status=$?
set +x

if [ "$status" -ne 0 ]; then
  fail "tauri build 退出码 $status"
fi

step "构建完成"
EXE="src-tauri/target/x86_64-pc-windows-gnu/release/starlit-foss.exe"
if [ -f "$EXE" ]; then
  ls -lh "$EXE"
  echo
  echo "产物：$EXE"
else
  echo "未找到预期的 exe，请检查 target 目录。"
fi
echo
echo "说明：本次只编译 exe，没有生成 NSIS 安装包（本机未安装 makensis）。"
echo "日志已保存：$LOG"
printf '\n按 Enter 关闭窗口…'
read -r _
