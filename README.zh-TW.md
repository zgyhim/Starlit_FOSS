# 星伴 Starlit FOSS

**一個本地執行、專注聊天的 AI 夥伴。**

[English](README.md) | [簡體中文](README.zh-CN.md) | 繁體中文

星伴 FOSS 是對「星伴」桌面應用的獨立重實現。它基於原版**逆向分析得到的前端原始碼**，保留了你熟悉的介面、角色、人格和表情包，但把底層那套複雜的 Electron + Python + OneBot 架構整個換掉了。

沒有 QQ 協議，沒有本地 HTTP/WebSocket 服務，沒有令牌，沒有背景 Python 程式。開啟就是聊天。

> **重要注意事項：安裝包沒有互動精靈。** 雙擊後會直接開始安裝：沒有授權條款頁、沒有安裝目錄選擇、也沒有確認步驟，直接裝到 `%ProgramFiles%\Starlit FOSS`。想清楚再開啟。解除安裝請走 **設定 → 應用程式 → 已安裝的應用程式**。

---

## 為什麼要重寫

原版星伴其實是一個「用 QQ 機器人協議驅動聊天介面」的應用：Electron 外殼負責啟動 Python 後端，後端再跑 OneBot 服務端，前端通過 WebSocket 收發訊息。

這套架構能工作，但對一個**單機陪伴應用**來說太重了：

- 要維護 Python 執行時和打包產物
- 要開本地埠，還要處理訪問令牌
- 訊息要先轉成 OneBot 的協議格式，再轉回來
- 任何一環出問題，聊天就打不開

星伴 FOSS 把這些全部去掉：

| 原版 | 現在 |
|---|---|
| Electron + Python 雙程序 | Tauri 2 單程序 |
| OneBot 11 服務端 / WebSocket | 直接呼叫 Rust 命令 |
| 本地 HTTP `/api/*` + 訪問令牌 | 無埠、無令牌 |
| 訊息段 / CQ 碼 / 事件推送 | 中性的訊息結構 |
| 需要等後端健康檢查 | 開啟即用 |

### 換來了什麼

- **更純淨的聊天**：沒有協議層，訊息直接進資料庫、直接上屏
- **更輕**：不需要 Python，不需要多開一個後端
- **更穩**：少了程序間通訊和端口占用，退出不會殘留後臺
- **更安全**：不監聽任何埠，區域網裡掃不到
- **更好維護**：前端沿用原版，後端是清晰的 Rust 模組
- **仍然保留原味**：原版介面、10 位角色、人格設定、表情包、頭像

---

## 她是什麼樣的夥伴

- **一對一私聊**：文字、表情包、引用、撤回、轉發
- **有自己的人設**：每位角色有獨立的說話方式、性格和邊界
- **會記住你**：偏好、約定和發生過的事會被記下來，之後自然提起
- **會主動找你**：上線問候、想念、分享心情、回顧聊過的事
- **有自己的節奏**：不會秒回轟炸，也不會在你忙時打擾，深夜會安靜下來；回覆速度可調（瞬間 / 快 / 標準 / 慢）
- **會接著聊**：連發幾條會合併成一次回應；你不回時她會過一會兒輕輕補一句
- **本地面貌**：原版頭像、表情包合集、聊天氣泡和背景

## 現在能做什麼

| 能力 | 狀態 |
|---|---|
| 私聊對話與模型回覆 | ✅ 支援回覆速度調節 |
| 長期記憶（抽取、糾正、置頂、召回） | ✅ |
| 主動陪伴（問候、想念、心情、摘要） | ✅ |
| 角色頭像與表情包 | ✅ |
| 聊天曆史、引用、撤回、轉發 | ✅ |
| 多角色切換 | ✅ |
| 群聊 | ✅ 基礎（接話、@ 補全、管理、ambient） |
| 好感 / 羈絆 | ✅ 基礎（每日成長、繫結） |
| 表情包自動搭配 | ✅ |
| 備份 | ✅ 匯出 / 恢復（重啟生效） |
| 共同生活 | ✅ 約定到期跟進、紀念日、日記 |
| 搜尋 / 診斷 | ✅ |
| 角色卡匯入、資料編輯 | ✅ 基礎（支援試聊與 AI 生成人設） |

### 被移除 / 明確不實現的功能

星伴 FOSS 不是原版的完整複製。以下能力被移除或明確不做，介面入口也已刪除：

| 功能 | 處理 | 原因 |
|---|---|---|
| 語音合成 / 朗讀（TTS） | 移除 | 擺脫對 OpenAI 相容語音介面的依賴，堅持開源標準與供應商中立 |
| 手機與電腦區域網同步 | 移除 | 協議、鑑權、備份恢復鏈路過於複雜，且會帶來額外的安全暴露面 |
| 錄音傳送（語音輸入） | 移除 | 麥克風許可權在各平臺不一致；語音識別會重新引入對特定供應商的依賴 |
| 群檔案 | 移除 | 實現複雜；檔案內容會讓模型的多模態輸入變得混雜，偏離純聊天定位 |
| Stapxs 經典客戶端入口 | 移除 | 依賴 OneBot 與本地服務，與純本地 IPC 架構衝突 |
| OneBot / QQ 協議、本地 HTTP/WS、訪問令牌 | 移除 | 單機陪伴應用不需要協議層；去掉後故障面更小、也不監聽埠 |
| 表情包縮圖 | 移除 | 始終顯示原圖，避免縮圖與原圖不一致 |
| Windows 7 / 8 / 8.1 | 不支援 | WebView2 與 Tauri 2 要求 Windows 10 及以上，啟動時會給出相容性警告 |

這些取捨的原則是：**寧可少一個功能，也不把供應商依賴、複雜協議或安全風險帶回來。**

---

## 技術棧

| 層次 | 選型 |
|---|---|
| 外殼 | Tauri 2（單一程序，執行時不需要 Node / Python） |
| 後端 | Rust 2021，最低 1.77.2 |
| 前端 | 原版原生 HTML / CSS / JavaScript，無框架、無打包器 |
| 非同步執行環境 | Tokio |
| 儲存 | SQLite（`rusqlite` bundled），設定用 YAML |
| 模型呼叫 | `reqwest` + rustls，對接 OpenAI 相容介面 |
| 序列化 | `serde`、`serde_json`、`serde_yaml` |
| 媒體包 | `zstd` + `tar`，首次啟動解包並快取 |
| 日誌 | `tracing` + logcat 風格格式，落盤與輪轉 |
| Web 引擎 | WebView2（Windows）、WebKitGTK（Linux）、WKWebView（macOS） |
| 建置 | Makefile + npm + Tauri CLI；MSI 用 `wixl`（msitools） |

沒有 BaaS、沒有資料庫服務、沒有訊息佇列、不依賴容器執行環境。

---
## 支援平臺

| 平臺 | 狀態 |
|---|---|
| Linux x86_64 | ✅ 當前開發與驗證平臺 |
| Windows x86_64 | ✅ 支援 |
| Android 10+ | 🚧 計劃中 |
| FreeBSD x86_64 | ✅ 支援 |

---

## 開始使用

### 直接執行

從 Releases 下載對應平臺的安裝包，開啟即可。

首次啟動會準備角色、人格和表情資源，需要等一會兒；載入頁會顯示真實進度。

### 配置模型

星伴需要一個 OpenAI 相容的大模型服務：

1. 開啟「設定 → 高階與開發者配置」
2. 填寫介面地址、模型名和 API Key
3. 儲存後開始聊天

預設面向 DeepSeek：

```yaml
base_url: https://api.deepseek.com
model: deepseek-chat
reasoning_effort: low
```

API Key 只儲存在本機配置檔案裡，不上傳，也不會寫進日誌。

### 從原始碼構建

媒體包（約 759 MB）不放在 Git 倉庫裡，釋出時作為 Release 附件。構建前先準備媒體檔案：

```bash
make media
```

指令碼會按 `resources/media/manifest.json` 檢查並校驗每個媒體包：

- 本地已有完整檔案 → 直接使用
- 只有 `.zst.000` 分片 → 自動合併並校驗
- 都沒有 → 從 `manifest.json` 的 `base_url` 下載；也可用環境變數指定：

```bash
STARLIT_MEDIA_URL=https://你的地址 make media
```

準備完成後：

```bash
make deps      # npm install
make dev       # 開發
make build     # 打包
```

Linux 需要 `webkit2gtk-4.1`、`gtk+-3.0`、`libsoup-3.0`；Windows 需要 WebView2。

`make help` 可以列出所有目標。

### Windows MSI

Windows 安裝包用 GNU `msitools` 的 `wixl` 在 Linux 上直接生成，不需要 Windows 或 Wine：

```bash
sudo apt install msitools
make windows-msi     # 交叉編譯 exe，再打包 MSI
```

產物：`dist/StarlitFOSS-<version>-x64.msi`（約 772 MB，含媒體包）。

只想要輕量安裝包（不含媒體）：

```bash
make msi INCLUDE_MEDIA=0
```

### FreeBSD

FreeBSD 必須原生建置：從 Linux 交叉編譯 Tauri 應用並不現實，因為 WebKitGTK 與 GTK 需要連結 FreeBSD 的函式庫，且 WebKit 建置時要執行 gobject-introspection。請在 FreeBSD 15.x 的機器、虛擬機或 jail 裡建置。

依賴：

```sh
sudo pkg install -y rust gtk3 webkit2-gtk_41 libsoup3 dbus pkgconf
```

建置並打包：

```bash
make pkg
```

產生 `dist/starlit-foss-<version>.pkg`。依賴宣告為 `webkit2-gtk_41`、`gtk3`、`libsoup3`、`dbus`，安裝時由 `pkg` 自動補齊。檔案裝到 `/usr/local/lib/starlit-foss`，啟動器在 `/usr/local/bin/starlit-foss`。

安裝：

```sh
sudo pkg add dist/starlit-foss-<version>.pkg
```

說明：

- FreeBSD 上沒有單一實例保護，`tauri-plugin-single-instance` 沒有 FreeBSD 實作
- 編譯已通過；在真實 FreeBSD 桌面（Xorg + DRM/KMS）上的啟動尚未驗證，無頭 Xvfb 會卡在 EGL/DRI 初始化
- 依賴走 `pkg`，不捆綁 GTK 與 WebKit

### Linux 便攜包

```bash
make tar
```

產生 `dist/StarlitFOSS-<version>-linux-x86_64.tar.gz` 與對應的 `.sha256`。

### 打包目錄結構

所有套件格式都是同一個入口下的外掛：

```text
packaging/
├─ package.sh        分發入口：./packaging/package.sh <format>
├─ lib/common.sh     暫存目錄、版本號、啟動器
└─ pm/
   ├─ freebsd.sh     .pkg
   ├─ msi.sh         .msi
   └─ tar.sh         Linux 便攜 tar.gz
```

新增套件管理器只需放一個 `packaging/pm/<名稱>.sh`，`make package` 會自動列出。每次打包都使用獨立暫存目錄，結束（含失敗與 Ctrl-C）時清理，不會在原始碼樹留下殘留。

---

## TUI 工具

倉庫自帶兩個命令列工具，功能一致：

```bash
# Linux / macOS
make tui

# Windows PowerShell
powershell -ExecutionPolicy Bypass -File tools\tui.ps1
```

選單提供：

1. **匯出日誌資料夾** — Linux/macOS 匯出 `.tar.zst`（有 zstd 時），Windows 匯出 `.zip`
2. **安裝 WebView2** — Windows 檢測並靜默安裝 Evergreen Runtime；其他平台提示不需要
3. **檢查檔案完整性** — 校驗執行檔、媒體包 MD5、資料庫 `integrity_check`、日誌目錄

也支援非互動呼叫：

```bash
./tools/tui.sh --export
./tools/tui.sh --check
```

```powershell
.\tools\tui.ps1 -Action export
.\tools\tui.ps1 -Action check
```

`tui.ps1` 也支援 PowerShell 7+（`pwsh`）在 Linux / macOS 上執行：

- 資料目錄按平臺解析：Linux `~/.local/share/starlit-desktop`，macOS `~/Library/Application Support/starlit-desktop`，Windows `%APPDATA%\starlit-desktop`
- 匯出日誌優先用 `tar` + `zstd` 生成 `.tar.zst`，沒有 zstd 時退回 `.zip`
- 安裝 WebView2 會直接提示當前平臺不需要
- 完整性檢查會額外查詢 `src-tauri/target/{release,debug}/starlit-foss`

可用 `STARLIT_APP_ROOT` / `STARLIT_DATA_ROOT` 覆蓋應用目錄與資料目錄。

## 你的資料在哪

- Linux：`~/.local/share/starlit-desktop`
- Windows：`%APPDATA%\starlit-desktop`

裡面是聊天記錄、長期記憶、人格、頭像、表情包和模型配置。沒有統計、沒有埋點、沒有云端賬號；唯一的外部請求是你自己配置的模型介面。

「設定 → 資料備份」可以匯出 zip（資料庫、人格、角色/群資料、表情索引，不含 API Key）；恢復後重啟生效。

應用執行時會在資料目錄放置鎖檔案，防止兩個例項同時改同一份資料。看到「系統檔案正在被佔用」時，說明還有另一個星伴沒有退出。

---

## 設計想法

星伴想做的不是「一個能聊天的模型」，而是一個**記得你、有自己的生活、會主動靠近**的夥伴。

所以：

- 回覆會分成幾條，中間有停頓
- 深夜不會突然發訊息
- 你很久沒回復，她會慢慢安靜下來
- 記住的事會隨聊天積累，而不是每次從頭開始

她仍然是一個本地程式。人設是設定，記憶是資料，情感體驗由你自己定義。

---

## 接下來

1. 軟體更新器
2. Android 10+
3. 更多的自動化測試
4. 備份的定時/自動策略
5. 群聊的更多細節（等級許可權、群內分享）

---

## 許可證

本專案以 **GNU Affero General Public License v3.0（AGPL-3.0）** 釋出，完整條款見 [LICENSE](LICENSE)。

- 修改、分發或通過網路提供服務時，需要以同一許可證開放對應原始碼
- 原版角色、人格與美術資源的版權仍歸原作者及相應權利人，不因本專案許可證而改變

## 關於這個專案

星伴 FOSS 是個人主導的本地優先專案，沒有賬號體系，也沒有商業化計劃。

它建立在對原版客戶端的逆向分析之上：前端介面與互動沿用原版，後端用 Rust 重新實現。這樣既能保留原有體驗，又能把協議層和冗餘程序徹底拿掉。

當前版本由 AI 輔助開發完成（DeepSeek V4.1 Flash，2026-11-06），程式碼仍在持續整理中；這不代表開發者個人的程式碼水平或能力。

歡迎提交 Issue 或 Pull Request。
