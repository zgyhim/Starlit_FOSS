# Starlit FOSS（星伴）

星伴的 Tauri 2 重实现：**单进程、纯本地桌面应用**。没有 Python 后端，没有 HTTP/WebSocket 服务，没有 OneBot 协议，前端全部通过 Tauri IPC 取数。

当前阶段：核心聊天闭环可用（发送 → LLM 回复 → 记忆 → 主动陪伴），群聊、共同生活、备份、更新器等外围功能仍在建设中。

---

## 功能状态

### 已完成

- **聊天**：私聊发送、历史加载、分条回复、引用、撤回、已读、转发、草稿、未读
- **LLM**：OpenAI 兼容接口（默认 DeepSeek）、人设提示词、最近 24 条上下文、28 字切分、5–20 秒延迟、思考强度 `reasoning_effort`
- **记忆**：每轮抽取、文本哈希去重、相似度版本链、置顶、20 类分类、top-6 召回、回忆中心（新增/编辑/纠正/置顶/删除）
- **主动性**：上线问候、权重节奏、能量值、连续未回复退避、沉默间隔增长、每日摘要、签到类型轮换、免打扰、每日上限
- **媒体**：头像与表情包原图、`starlit://` 自定义协议、tar.zst 资源包、启动解压进度、缓存清理
- **基础**：原生窗口、单实例、配置、SQLite、君子锁、WAL checkpoint、日志

### 未实现

- 消息合并窗口（9s / 30s / 10 条）
- 对话延续（followup / reopen / 深聊）
- 群聊生成策略、成员管理、入群审批、群文件
- 好感 / 羁绊、共同生活（约定、纪念日、日记）
- 备份导出与恢复
- 角色卡导入、角色创建/删除
- 历史搜索、诊断中心、资料与头像上传
- 软件更新器、Windows 打包发布

### 明确不实现（黑名单）

- **TTS / 语音合成**：前端入口与设置已移除，后端不实现
- **LAN 手机同步**：前端入口已移除，后端不实现

---

## 技术栈

- Tauri 2 + Rust
- 原生 webclient 前端（vanilla JS，无构建链）
- SQLite（`rusqlite` bundled，WAL）
- `reqwest`（rustls）+ OpenAI 兼容 API
- `zstd` + `tar` 媒体资源包

## 环境要求

- Node.js 18+ / npm（仅用于 Tauri CLI）
- Rust stable
- Linux：`webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0`

```bash
npm install
npm run dev      # tauri dev
npm run build    # 打包
```

首次运行会在用户数据目录创建配置、数据库、人格与媒体缓存。

---

## 目录结构

```
src/                      前端（原版 webclient，数据层已改为 IPC）
├─ index.html / app.css
├─ app.js                 界面与业务逻辑
└─ api.js                 window.api：invoke 封装 + 事件订阅
src-tauri/src/
├─ lib.rs                 启动装配、窗口、退出流程
├─ api.rs                 领域命令：身份/角色/群/消息/记忆
├─ llm.rs                 LLM 请求、人设提示、切分与延迟
├─ memory.rs              向量、抽取、召回、版本链
├─ autonomy.rs            主动消息状态机
├─ media.rs               tar.zst 解压、进度、缓存清理
├─ lock.rs                运行时君子锁
├─ config.rs              config.yaml（默认值 + 旧键兼容）
├─ db.rs                  SQLite 建表与迁移
├─ paths.rs               数据目录 / 资源目录 / 种子
├─ state.rs               AppState、日志
└─ ipc.rs                 窗口控制、日志、诊断
resources/template/       首次运行种子（人格、角色、群、表情索引）
resources/media/          tar.zst 资源包（默认不进 git）
scripts/build-media.sh    媒体包重建脚本
```

## 数据流

```
界面 → api.* → invoke → Rust 命令 → SQLite
Rust → emit("message:new" / "message:status" / "media:ready")
     → api.js → window.__starlitOnEvent → 界面
```

消息模型（中性，不含任何 OneBot 字段）：

```ts
Message {
  id: number
  conversation: { kind: "Private" | "Group", id: string }
  direction: "Incoming" | "Outgoing"
  author: { id: string, name: string, role: string }
  parts: Part[]      // text | image | sticker | mention | quote | voice | file | share | forward
  text: string
  replyTo, replyPreview, sentAt, clientId, status, system
}
```

---

## 数据目录

默认：

```
Linux   ~/.local/share/starlit-desktop
Windows %APPDATA%\starlit-desktop
macOS   ~/Library/Application Support/starlit-desktop
```

可用 `STARLIT_DATA_ROOT` 覆盖；`STARLIT_RESOURCE_ROOT` 可覆盖资源目录。

```
<dataRoot>/
├─ config.yaml            LLM / 行为 / 主动性 / 记忆配置
├─ data/
│  ├─ companion.db        SQLite（消息、记忆、kv、旧版表）
│  ├─ desktop.log
│  ├─ stickers/           表情包（index.json + 合集目录）
│  ├─ avatars/            头像
│  ├─ groups/             群资料
│  └─ ...
├─ persona/               人格资料
├─ backups/
├─ run/
└─ .cache/media/          解压标记 <sha256>.done 与产物清单 <sha256>.files
```

运行时会在上述位置创建 `.starlit.lock`、`data/companion.db.lock`、`data/media.lock`。**只要锁文件存在，启动即拒绝并弹窗提示**；正常退出时自动删除。

## LLM 配置

设置页可填写 Base URL、模型、API Key、思考强度（默认 `low`）。Key 只写入本机 `config.yaml`，不会进入日志或仓库。

```yaml
llm:
  base_url: https://api.deepseek.com
  api_key: ''
  model: deepseek-chat
  timeout: 90
  temperature: 0.9
  reasoning_effort: low
```

## 记忆召回

1. 每轮回复后调用 LLM 抽取 0–2 条长期事实，写入 `memories`
2. 文本转 128 维本地哈希向量（字符二元组 + SHA-256 分桶 + L2 归一化）
3. 与新消息计算余弦相似度；相似度 ≥ 0.82 时建立版本链（旧版本 `superseded`）
4. 回复前按 `相似度 0.7 + 权重 0.2 + 时间衰减 0.1 + 置顶 0.15` 取 `recall_top_k=6`，注入系统提示

> 这是词形相似度，不是语义 embedding；同义改写可能召不回。

## 主动陪伴

- 能量：默认 60，每分钟 −0.6，用户发言 +15，主动发言 −10；低于阈值不开口
- 节奏：instant / near / normal / long 四档按权重抽取
- 退避：连续 2 次无回应后，沉默间隔 ×1.2 增长，封顶 60 分钟
- 摘要：每 90 分钟检查一次，期间消息 ≥6 条才生成
- 免打扰、最小间隔、每日上限、跳过概率均读取 `config.yaml`

## 媒体资源包

`resources/media/` 下的 tar.zst（默认不提交）：

| 包 | 内容 |
|---|---|
| `avatars.tar.zst` | 用户 / 角色头像 |
| `groups.tar.zst` | 群资料 |
| `stickers-mygo.tar.zst` 等 | 各表情包合集 |

```bash
./scripts/build-media.sh     # 从原版模板重新生成
```

启动时解压全部资源包，`Loading...` 页面显示真实字节进度；完成后才进入主界面。失败会显示错误与退出按钮。设置中可立即安排清理或开启「退出时自动删除解压缓存」。

## 安全与数据

- 无遥测、无账号、无云端同步
- 出网仅限用户配置的 LLM 接口
- 自定义协议只允许读取 `dataRoot` 内文件，防目录穿越
- 资源包解压拒绝绝对路径与 `..`
- 退出时 `wal_checkpoint(TRUNCATE)` 合并 WAL 并释放锁

## 路线图

1. 消息合并窗口与对话延续
2. 群聊生成链路
3. 备份 / 恢复
4. 角色卡导入与资料管理
5. 更新器与发布流程

---

> 本项目由 AI 辅助生成（DeepSeek V4.1 Flash，2026-11-06），不代表开发者个人代码水平或能力。
