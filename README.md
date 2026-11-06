# Starlit FOSS

Tauri 2 + npm 桌面应用骨架。前端尚未选型，当前 `src/` 下只有占位页。

## 环境要求

- Node.js 18+ / npm
- Rust stable
- Linux 需要 `webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0`

## 命令

```bash
npm install        # 安装依赖（含 @tauri-apps/cli）
npm run dev        # 开发模式（tauri dev）
npm run build      # 打包（tauri build）
npm run tauri -- <args>   # 直接调用 Tauri CLI
```

## 结构

```
.
├── package.json            # npm 脚本与 Tauri CLI
├── src/
│   └── index.html          # 前端占位页，接入框架时替换
└── src-tauri/
    ├── Cargo.toml          # Rust 依赖（tauri 2）
    ├── build.rs
    ├── tauri.conf.json     # 应用/窗口/打包配置
    ├── capabilities/       # 权限（默认仅 core:default）
    ├── icons/              # 由 icon.svg 生成的各平台图标
    └── src/
        ├── main.rs
        └── lib.rs          # Tauri Builder 入口
```

## 后续接入前端

1. 在 `src/` 中选择框架（Vite/React/Vue/Svelte 等）初始化。
2. 在 `tauri.conf.json` 的 `build` 中加 `devUrl`（如 `http://localhost:1420`）和 `beforeDevCommand`/`beforeBuildCommand`，`frontendDist` 指向构建产物（如 `../dist`）。
3. 在 `src-tauri/Cargo.toml` 与 `package.json` 中补充所需插件（fs、dialog、http 等），并在 `capabilities/default.json` 中按最小权限原则开放。

## 设计约定

- 权限从 `core:default` 起步，按功能增量开放。
- 窗口 label 为 `main`，与 `capabilities/default.json` 对应。
- 应用标识 `org.starlitfoss.app`，打包前可按需修改。
