# Windows NSIS 安装包

`starlit.nsi` 是独立安装脚本，不依赖 Tauri 内置 NSIS 模板。

## 打包步骤

1. 在 Windows x64 上构建 release：

   ```powershell
   npm run build
   ```

   产物位于 `src-tauri\target\release`（含 `starlit-foss.exe` 与 `resources`）。

2. 生成安装包：

   ```powershell
   makensis /DAPP_SOURCE=src-tauri\target\release installer\starlit.nsi
   ```

3. 如需离线安装 WebView2，把 `MicrosoftEdgeWebview2Setup.exe` 放在与生成的安装包同一目录，安装时会检测并静默安装。

## 行为

- 默认安装到 `%ProgramFiles%\星伴 FOSS`
- 创建开始菜单与桌面快捷方式
- 写入卸载注册表项
- 卸载时默认保留 `%APPDATA%\starlit-desktop`，会先询问是否删除
- 仅支持 64 位 Windows
- 未安装 WebView2 时会提示下载地址

## 备注

- 脚本尚未在 Windows 实机验证。
- Tauri 自带的 `npm run build -- --bundles nsis` 也能生成 NSIS 包；本脚本用于需要自定义安装行为的手动打包。
