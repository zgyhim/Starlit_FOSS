# Windows MSI 安装包

用 GNU `msitools` 里的 `wixl` 在 Linux 上直接生成 MSI，不需要 Windows、Wine 或 WiX。

## 依赖

```bash
sudo apt install msitools      # Debian / Ubuntu
sudo pacman -S msitools        # Arch
```

## 打包

1. 先交叉编译出 exe：

   ```bash
   ./scripts/build-windows.sh
   ```

   或者手动：

   ```bash
   npx tauri build --no-bundle --target x86_64-pc-windows-gnu --ci
   ```

2. 生成 MSI：

   ```bash
   ./scripts/build-msi.sh
   ```

产物：`dist/StarlitFOSS-<version>-x64.msi`。

`build-windows.sh` 在检测到 `wixl` 时会自动调用这一步，不需要重复执行。

## 行为

- 安装到 `%ProgramFiles%\Starlit FOSS`
- 创建开始菜单与桌面快捷方式
- 注册卸载项，可从「应用和功能」卸载
- 仅支持 64 位 Windows
- 卸载不会删除 `%APPDATA%\starlit-desktop`，用户数据默认保留
- 包含 `resources/template` 与 `resources/media`，与便携版一致

## 可选参数

```bash
INCLUDE_MEDIA=0 ./scripts/build-msi.sh       # 不含媒体包，体积约 8 MB
TARGET_DIR=src-tauri/target/release ./scripts/build-msi.sh
```

## 限制

- `wixl` 只支持 WiX 的子集：没有自定义动作、没有 Burn 引导程序、没有 `WixUI`
- 不支持 WebView2 预检与自动安装；未安装 WebView2 时 Windows 会提示下载
- 生成的 MSI 未做代码签名，首次运行会被 SmartScreen 提示

## 目录名必须为 ASCII

`wixl` 写字符串表时只支持 cp1252 范围内的字符，中文目录名会变成空字符串。
因此 `resources/template/persona/characters/` 下的目录使用角色 ID 命名（如 `3901000011`），
显示名仍由 `meta.json` 与 `characters.json` 提供。构建脚本也会跳过编码异常的目录。
