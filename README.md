# Starlit FOSS（星伴）

Tauri 2 重实现。单进程、纯本地：**无 Python 后端、无 HTTP/WS 服务、无 OneBot**，前端全部经 Tauri IPC 取数。

## 运行

```bash
npm install
npm run dev      # tauri dev
npm run build    # 打包
```

Linux 需要 `webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0`。

## 结构

```
src/                     原版 webclient（保留 UI），数据层已改为 IPC
├─ index.html / app.css
├─ app.js                界面与业务逻辑
└─ api.js                window.api：Tauri 命令封装 + 事件订阅
src-tauri/src/
├─ lib.rs                启动装配（单实例 → 路径 → 配置 → DB → IPC）
├─ api.rs                领域命令：身份/角色/群/消息发送与历史
├─ config.rs             config.yaml 配置（旧文件多余键自动忽略）
├─ db.rs                 SQLite（messages / read_state + 旧版表结构）
├─ paths.rs              dataRoot / resourceRoot / 首次运行初始化
├─ state.rs              AppState、desktop.log
├─ ipc.rs                窗口控制、日志、诊断
└─ logging.rs
resources/template/      首次运行种子（人格、角色、群、表情索引）
```

## 数据流

```
界面事件 → api.* → invoke → Rust 命令 → SQLite
Rust → emit("message:new" 等) → api.js → window.__starlitOnEvent → 界面
```

消息模型（中性）：

```ts
Message {
  id, conversation: { kind: "Private" | "Group", id },
  direction: "Incoming" | "Outgoing",
  author: { id, name, role },
  parts: Part[],           // text | image | sticker | mention | quote | voice | file | share | forward
  text, replyTo, sentAt, clientId, status, system
}
```

## 媒体资源包

`resources/media/` 存放 tar.zst 合集（默认不进 git，由脚本重建）：

```bash
./scripts/build-media.sh      # 从原版模板重新生成
```

| 包 | 内容 | 解压时机 |
|---|---|---|
| `sticker-thumbs.tar.zst` | 表情缩略图 | 启动同步 |
| `avatars.tar.zst` | 用户/角色头像 | 启动同步 |
| `groups.tar.zst` | 群资料 | 启动同步 |
| `stickers-*.tar.zst` | 各表情包合集 | 后台线程 |

解压到 `dataRoot`，以 `<sha256>.done` 作为缓存标记；已解压过则直接跳过。

## 当前进度

- [x] 窗口壳 / 单实例 / 无边框标题栏
- [x] 配置、数据目录、SQLite、模板种子
- [x] UI 数据层 IPC 化（WebSocket、token、OneBot 段落全部移除）
- [x] 消息发送 / 历史加载 / 会话预览 / 已发送持久化
- [] LLM 管线（合并窗口、切分、延迟）
- [] 记忆 / 好感 / 主动性
- [] 群聊策略 / 表情包 / 共同生活 / 备份
- [] 更新器 / LAN 同步

> 本项目由 AI 辅助生成（DeepSeek V4.1 Flash，2026-11-06），不代表开发者个人代码水平。
