# 星伴 Starlit FOSS

**一个本地运行、专注聊天的 AI 伙伴。**

[English](README.md) | 简体中文 | [繁體中文](README.zh-TW.md)

星伴 FOSS 是对「星伴」桌面应用的独立重实现。它基于原版**逆向分析得到的前端源码**，保留了你熟悉的界面、角色、人格和表情包，但把底层那套复杂的 Electron + Python + OneBot 架构整个换掉了。

没有 QQ 协议，没有本地 HTTP/WebSocket 服务，没有令牌，没有后台 Python 进程。打开就是聊天。

> **重要注意事项：安装包没有交互向导。** 双击后会直接开始安装：没有许可协议页、没有安装目录选择、也没有确认步骤，直接装到 `%ProgramFiles%\Starlit FOSS`。想好再打开。卸载走 **设置 → 应用 → 已安装的应用**。

---

## 为什么要重写

原版星伴其实是一个「用 QQ 机器人协议驱动聊天界面」的应用：Electron 外壳负责启动 Python 后端，后端再跑 OneBot 服务端，前端通过 WebSocket 收发消息。

这套架构能工作，但对一个**单机陪伴应用**来说太重了：

- 要维护 Python 运行时和打包产物
- 要开本地端口，还要处理访问令牌
- 消息要先转成 OneBot 的协议格式，再转回来
- 任何一环出问题，聊天就打不开

星伴 FOSS 把这些全部去掉：

| 原版 | 现在 |
|---|---|
| Electron + Python 双进程 | Tauri 2 单进程 |
| OneBot 11 服务端 / WebSocket | 直接调用 Rust 命令 |
| 本地 HTTP `/api/*` + 访问令牌 | 无端口、无令牌 |
| 消息段 / CQ 码 / 事件推送 | 中性的消息结构 |
| 需要等后端健康检查 | 打开即用 |

### 换来了什么

- **更纯净的聊天**：没有协议层，消息直接进数据库、直接上屏
- **更轻**：不需要 Python，不需要多开一个后端
- **更稳**：少了进程间通信和端口占用，退出不会残留后台
- **更安全**：不监听任何端口，局域网里扫不到
- **更好维护**：前端沿用原版，后端是清晰的 Rust 模块
- **仍然保留原味**：原版界面、10 位角色、人格设定、表情包、头像

---

## 她是什么样的伙伴

- **一对一私聊**：文字、表情包、引用、撤回、转发
- **有自己的人设**：每位角色有独立的说话方式、性格和边界
- **会记住你**：偏好、约定和发生过的事会被记下来，之后自然提起
- **会主动找你**：上线问候、想念、分享心情、回顾聊过的事
- **有自己的节奏**：不会秒回轰炸，也不会在你忙时打扰，深夜会安静下来；回复速度可调（瞬间 / 快 / 标准 / 慢）
- **会接着聊**：连发几条会合并成一次回应；你不回时她会过一会儿轻轻补一句
- **本地面貌**：原版头像、表情包合集、聊天气泡和背景

## 现在能做什么

| 能力 | 状态 |
|---|---|
| 私聊对话与模型回复 | ✅ 支持回复速度调节 |
| 长期记忆（抽取、纠正、置顶、召回） | ✅ |
| 主动陪伴（问候、想念、心情、摘要） | ✅ |
| 角色头像与表情包 | ✅ |
| 聊天历史、引用、撤回、转发 | ✅ |
| 多角色切换 | ✅ |
| 群聊 | ✅ 基础（接话、@ 补全、管理、ambient） |
| 好感 / 羁绊 | ✅ 基础（每日成长、绑定） |
| 表情包自动搭配 | ✅ |
| 备份 | ✅ 导出 / 恢复（重启生效） |
| 共同生活 | ✅ 约定到期跟进、纪念日、日记 |
| 搜索 / 诊断 | ✅ |
| 角色卡导入、资料编辑 | ✅ 基础（支持试聊与 AI 生成人设） |

### 被移除 / 明确不实现的功能

星伴 FOSS 不是原版的完整复制。以下能力被移除或明确不做，界面入口也已删除：

| 功能 | 处理 | 原因 |
|---|---|---|
| 语音合成 / 朗读（TTS） | 移除 | 摆脱对 OpenAI 兼容语音接口的依赖，坚持开源标准与供应商中立 |
| 手机与电脑局域网同步 | 移除 | 协议、鉴权、备份恢复链路过于复杂，且会带来额外的安全暴露面 |
| 录音发送（语音输入） | 移除 | 麦克风权限在各平台不一致；语音识别会重新引入对特定供应商的依赖 |
| 群文件 | 移除 | 实现复杂；文件内容会让模型的多模态输入变得混杂，偏离纯聊天定位 |
| Stapxs 经典客户端入口 | 移除 | 依赖 OneBot 与本地服务，与纯本地 IPC 架构冲突 |
| OneBot / QQ 协议、本地 HTTP/WS、访问令牌 | 移除 | 单机陪伴应用不需要协议层；去掉后故障面更小、也不监听端口 |
| 表情包缩略图 | 移除 | 始终显示原图，避免缩略图与原图不一致 |
| Windows 7 / 8 / 8.1 | 不支持 | WebView2 与 Tauri 2 要求 Windows 10 及以上，启动时会给出兼容性警告 |

这些取舍的原则是：**宁可少一个功能，也不把供应商依赖、复杂协议或安全风险带回来。**

---

## 技术栈

| 层次 | 选型 |
|---|---|
| 外壳 | Tauri 2（单进程，运行时不需要 Node / Python） |
| 后端 | Rust 2021，最低 1.77.2 |
| 前端 | 原版原生 HTML / CSS / JavaScript，无框架、无打包器 |
| 异步运行时 | Tokio |
| 存储 | SQLite（`rusqlite` bundled），配置用 YAML |
| 模型调用 | `reqwest` + rustls，对接 OpenAI 兼容接口 |
| 序列化 | `serde`、`serde_json`、`serde_yaml` |
| 媒体包 | `zstd` + `tar`，首次启动解包并缓存 |
| 日志 | `tracing` + logcat 风格格式，落盘与轮转 |
| Web 引擎 | WebView2（Windows）、WebKitGTK（Linux）、WKWebView（macOS） |
| 构建 | Makefile + npm + Tauri CLI；MSI 用 `wixl`（msitools） |

没有 BaaS、没有数据库服务、没有消息队列、不依赖容器运行时。

---
## 支持平台

| 平台 | 状态 |
|---|---|
| Linux x86_64 | ✅ 当前开发与验证平台 |
| Windows x86_64 | ✅ 支持 |
| Android 10+ | 🚧 计划中 |
| FreeBSD x86_64 | 🚧 计划中 |

---

## 开始使用

### 直接运行

从 Releases 下载对应平台的安装包，打开即可。

首次启动会准备角色、人格和表情资源，需要等一会儿；加载页会显示真实进度。

### 配置模型

星伴需要一个 OpenAI 兼容的大模型服务：

1. 打开「设置 → 高级与开发者配置」
2. 填写接口地址、模型名和 API Key
3. 保存后开始聊天

默认面向 DeepSeek：

```yaml
base_url: https://api.deepseek.com
model: deepseek-chat
reasoning_effort: low
```

API Key 只保存在本机配置文件里，不上传，也不会写进日志。

### 从源码构建

媒体包（约 759 MB）不放在 Git 仓库里，发布时作为 Release 附件。构建前先准备媒体文件：

```bash
make media
```

脚本会按 `resources/media/manifest.json` 检查并校验每个媒体包：

- 本地已有完整文件 → 直接使用
- 只有 `.zst.000` 分片 → 自动合并并校验
- 都没有 → 从 `manifest.json` 的 `base_url` 下载；也可用环境变量指定：

```bash
STARLIT_MEDIA_URL=https://你的地址 make media
```

准备完成后：

```bash
make deps      # npm install
make dev       # 开发
make build     # 打包
```

Linux 需要 `webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0`；Windows 需要 WebView2。

`make help` 可以列出所有目标。

### Windows MSI

Windows 安装包用 GNU `msitools` 的 `wixl` 在 Linux 上直接生成，不需要 Windows 或 Wine：

```bash
sudo apt install msitools
make windows-msi     # 交叉编译 exe，再打包 MSI
```

产物：`dist/StarlitFOSS-<version>-x64.msi`（约 772 MB，含媒体包）。

只想要轻量安装包（不含媒体）：

```bash
make msi INCLUDE_MEDIA=0
```

### FreeBSD 包

在 FreeBSD 里原生构建并打包（交叉编译不可行，见上文说明）：

```bash
make pkg
```

生成 `dist/starlit-foss-<version>.pkg`，依赖声明为 `webkit2-gtk_41`、`gtk3`、`libsoup3`、`dbus`，安装时由 `pkg` 自动补齐。文件装到 `/usr/local/lib/starlit-foss`，启动器在 `/usr/local/bin/starlit-foss`。

### Linux 便携包

```bash
make tar
```

生成 `dist/StarlitFOSS-<version>-linux-x86_64.tar.gz` 与对应的 `.sha256`。

### 打包目录结构

所有包格式都是同一个入口下的插件：

```text
packaging/
├─ package.sh        分发入口：./packaging/package.sh <format>
├─ lib/common.sh     暂存目录、版本号、启动器
└─ pm/
   ├─ freebsd.sh     .pkg
   ├─ msi.sh         .msi
   └─ tar.sh         Linux 便携 tar.gz
```

新增包管理器只需放一个 `packaging/pm/<名称>.sh`，`make package` 会自动列出。每次打包都使用独立临时目录，退出（含失败和 Ctrl-C）时清理，不会在源码树里留残余。

---

## TUI 工具

仓库自带两个命令行工具，功能一致：

```bash
# Linux / macOS
make tui

# Windows PowerShell
powershell -ExecutionPolicy Bypass -File tools\tui.ps1
```

菜单提供：

1. **导出日志文件夹** — Linux/macOS 导出 `.tar.zst`（有 zstd 时），Windows 导出 `.zip`
2. **安装 WebView2** — Windows 检测并静默安装 Evergreen Runtime；其他平台提示不需要
3. **检查文件完整性** — 校验可执行文件、媒体包 MD5、数据库 `integrity_check`、日志目录

也支持非交互调用：

```bash
./tools/tui.sh --export
./tools/tui.sh --check
```

```powershell
.\tools\tui.ps1 -Action export
.\tools\tui.ps1 -Action check
```

`tui.ps1` 也支持 PowerShell 7+（`pwsh`）在 Linux / macOS 上运行：

- 数据目录按平台解析：Linux `~/.local/share/starlit-desktop`，macOS `~/Library/Application Support/starlit-desktop`，Windows `%APPDATA%\starlit-desktop`
- 导出日志优先用 `tar` + `zstd` 生成 `.tar.zst`，没有 zstd 时退回 `.zip`
- 安装 WebView2 会直接提示当前平台不需要
- 完整性检查会额外查找 `src-tauri/target/{release,debug}/starlit-foss`

可用 `STARLIT_APP_ROOT` / `STARLIT_DATA_ROOT` 覆盖应用目录与数据目录。

## 你的数据在哪

- Linux：`~/.local/share/starlit-desktop`
- Windows：`%APPDATA%\starlit-desktop`

里面是聊天记录、长期记忆、人格、头像、表情包和模型配置。没有统计、没有埋点、没有云端账号；唯一的外部请求是你自己配置的模型接口。

「设置 → 数据备份」可以导出 zip（数据库、人格、角色/群资料、表情索引，不含 API Key）；恢复后重启生效。

应用运行时会在数据目录放置锁文件，防止两个实例同时改同一份数据。看到「系统文件正在被占用」时，说明还有另一个星伴没有退出。

---

## 设计想法

星伴想做的不是「一个能聊天的模型」，而是一个**记得你、有自己的生活、会主动靠近**的伙伴。

所以：

- 回复会分成几条，中间有停顿
- 深夜不会突然发消息
- 你很久没回复，她会慢慢安静下来
- 记住的事会随聊天积累，而不是每次从头开始

她仍然是一个本地程序。人设是设定，记忆是数据，情感体验由你自己定义。

---

## 接下来

1. 软件更新器
2. Android 10+ 与 FreeBSD x86_64
3. 更多的自动化测试
4. 备份的定时/自动策略
5. 群聊的更多细节（等级权限、群内分享）

---

## 许可证

本项目以 **GNU Affero General Public License v3.0（AGPL-3.0）** 发布，完整条款见 [LICENSE](LICENSE)。

- 修改、分发或通过网络提供服务时，需要以同一许可证开放对应源代码
- 原版角色、人格与美术资源的版权仍归原作者及相应权利人，不因本项目许可证而改变

## 关于这个项目

星伴 FOSS 是个人主导的本地优先项目，没有账号体系，也没有商业化计划。

它建立在对原版客户端的逆向分析之上：前端界面与交互沿用原版，后端用 Rust 重新实现。这样既能保留原有体验，又能把协议层和冗余进程彻底拿掉。

当前版本由 AI 辅助开发完成（DeepSeek V4.1 Flash，2026-11-06），代码仍在持续整理中；这不代表开发者个人的代码水平或能力。

欢迎提交 Issue 或 Pull Request。
