# Starlit FOSS

**A local-first AI companion built for conversation.**

English | [简体中文](README.zh-CN.md) | [繁體中文](README.zh-TW.md)

Starlit FOSS is an independent reimplementation of the Starlit desktop app. It keeps the interface, characters, personas and sticker packs you already know, taken from the **frontend source recovered by reverse engineering**, while replacing the whole Electron + Python + OneBot stack underneath.

No QQ protocol, no local HTTP/WebSocket server, no access tokens, no background Python process. Open it and talk.

> **Important notice: the Windows installer has no interactive wizard.** Double-clicking starts installing immediately: no license page, no install-directory picker, no confirmation step. It installs to `%ProgramFiles%\Starlit FOSS`. Make sure you want to install before opening the file. To uninstall, use **Settings → Apps → Installed apps**.

---

## Why rewrite it

The original app drives a chat UI through a QQ bot protocol: an Electron shell starts a Python backend, the backend runs an OneBot server, and the frontend exchanges messages over WebSocket.

That works, but for a **single-machine companion app** it is far too heavy:

- a Python runtime has to be shipped and maintained
- a local port and an access token are required
- every message is converted to OneBot format and back
- if any link in the chain fails, chat does not start

Starlit FOSS removes all of it:

| Original | Now |
|---|---|
| Electron + Python, two processes | Tauri 2, one process |
| OneBot 11 server / WebSocket | Direct Rust commands |
| Local HTTP `/api/*` + token | No port, no token |
| Message segments / CQ codes / event push | Neutral message model |
| Waits for backend health check | Usable immediately |

### What you get

- **Cleaner chat**: no protocol layer, messages go straight to the database and the screen
- **Lighter**: no Python, no second backend
- **Sturdier**: no IPC or port contention, no leftover processes after exit
- **Safer**: listens on nothing, invisible to LAN scans
- **More maintainable**: original frontend, clear Rust modules
- **Still familiar**: original UI, 10 characters, persona settings, stickers, avatars

---

## What kind of companion she is

- **One-on-one chat**: text, stickers, quotes, recall, forwarding
- **Has a persona**: each character has her own voice, temperament and boundaries
- **Remembers you**: preferences, promises and past events are stored and recalled naturally
- **Reaches out first**: greetings, missing you, sharing moods, revisiting old topics
- **Has her own rhythm**: no instant bombardment, no interruptions when you are busy, quiet at night; reply speed is adjustable (instant / fast / normal / slow)
- **Keeps the thread**: several messages in a row get one combined reply; if you go silent she adds a gentle follow-up later
- **Local look**: original avatars, sticker collections, chat bubbles and backgrounds

## Features

| Capability | Status |
|---|---|
| Direct chat with model replies | ✅ adjustable reply speed |
| Long-term memory (extract, correct, pin, recall) | ✅ |
| Proactive companionship (greeting, missing, mood, summary) | ✅ |
| Character avatars and stickers | ✅ |
| History, quotes, recall, forwarding | ✅ |
| Multiple characters | ✅ |
| Group chat | ✅ basic (interjections, @ completion, admin, ambient) |
| Bond / affinity | ✅ basic (daily growth, binding) |
| Automatic sticker matching | ✅ |
| Backup | ✅ export / restore (restart required) |
| Shared life | ✅ deadline follow-up, anniversaries, diary |
| Search / diagnostics | ✅ |
| Character card import, profile editing | ✅ basic (trial chat and AI persona generation) |

### Removed / explicitly not implemented

Starlit FOSS is not a complete clone. The following are removed, including their UI entries:

| Feature | Disposition | Reason |
|---|---|---|
| Text-to-speech / reading aloud | Removed | Drops the dependency on OpenAI-compatible voice APIs; keeps the project vendor-neutral |
| LAN sync between phone and desktop | Removed | Protocol, auth and backup chains were too complex and added attack surface |
| Voice message recording | Removed | Microphone permissions differ per platform; speech recognition reintroduces vendor lock-in |
| Group files | Removed | Complex, and file content muddles multimodal model input away from pure chat |
| Stapxs classic client entry | Removed | Depends on OneBot and a local service, conflicts with local IPC |
| OneBot / QQ protocol, local HTTP/WS, tokens | Removed | A single-machine app needs no protocol layer; fewer failures, no open ports |
| Sticker thumbnails | Removed | Always show the original, avoiding mismatch with thumbnails |
| Windows 7 / 8 / 8.1 | Unsupported | WebView2 and Tauri 2 require Windows 10+; a compatibility warning is shown at startup |

The rule behind these trade-offs: **rather lose a feature than bring back a vendor dependency, a complex protocol or extra risk.**

---

## Tech stack

| Layer | Choice |
|---|---|
| Shell | Tauri 2 (single process, no Node/Python at runtime) |
| Backend | Rust 2021, MSRV 1.77.2 |
| Frontend | Original vanilla HTML / CSS / JavaScript, no framework, no bundler |
| Async runtime | Tokio |
| Storage | SQLite via `rusqlite` (bundled), YAML for configuration |
| Model access | `reqwest` + rustls against OpenAI-compatible endpoints |
| Serialization | `serde`, `serde_json`, `serde_yaml` |
| Media packs | `zstd` + `tar`, cached on first launch |
| Logging | `tracing` + logcat-style formatting, file output and rotation |
| Web engine | WebView2 (Windows), WebKitGTK (Linux), WKWebView (macOS) |
| Build | Makefile + npm + Tauri CLI; MSI via `wixl` (msitools) |

No BaaS, no database server, no message queue, no container runtime.

---
## Platforms

| Platform | Status |
|---|---|
| Linux x86_64 | ✅ development and verification platform |
| Windows x86_64 | ✅ supported |
| Android 10+ | 🚧 planned |
| FreeBSD x86_64 | 🚧 planned |

---

## Getting started

### Run a build

Download the package for your platform from Releases and open it.

The first launch prepares characters, personas and sticker resources, which takes a moment; the loading screen shows real progress.

### Configure a model

Starlit needs an OpenAI-compatible model service:

1. Open **Settings → Advanced & developer configuration**
2. Fill in the base URL, model name and API key
3. Save and start chatting

Defaults target DeepSeek:

```yaml
base_url: https://api.deepseek.com
model: deepseek-chat
reasoning_effort: low
```

The API key stays in the local config file. It is not uploaded and never written to logs.

### Build from source

Media packs (about 759 MB) are not stored in Git and ship as release assets. Fetch them first:

```bash
make media
```

The script checks `resources/media/manifest.json` and verifies each pack:

- complete local files → used as is
- only `.zst.000` parts → merged and verified
- nothing present → downloaded from `base_url`; override with:

```bash
STARLIT_MEDIA_URL=https://your-host make media
```

Then build:

```bash
make deps          # npm install
make dev           # development
make build         # tauri build
```

Linux needs `webkit2gtk-4.1`, `gtk+-3.0` and `libsoup-3.0`; Windows needs WebView2.

`make help` lists every target.

### Windows MSI

The Windows installer is produced on Linux with `wixl` from GNU `msitools`; no Windows or Wine required:

```bash
sudo apt install msitools
make windows-msi     # cross-compile the exe, then build the MSI
```

Output: `dist/StarlitFOSS-<version>-x64.msi` (about 772 MB with media).

A slim installer without media packs:

```bash
make msi INCLUDE_MEDIA=0
```

### Portable Linux package

```bash
make linux-package
```

Produces `dist/StarlitFOSS-<version>-linux-x86_64.tar.gz` plus a `.sha256` file.

---

## TUI tool

The repository ships two equivalent command-line tools:

```bash
# Linux / macOS
make tui

# Windows PowerShell
powershell -ExecutionPolicy Bypass -File tools\tui.ps1
```

Menu entries:

1. **Export logs** — `.tar.zst` on Linux/macOS when `zstd` exists, `.zip` on Windows
2. **Install WebView2** — detects and silently installs the Evergreen runtime on Windows; reports that it is not needed elsewhere
3. **Check file integrity** — executable, media pack MD5, database `integrity_check`, log directory

Non-interactive use:

```bash
./tools/tui.sh --export
./tools/tui.sh --check
```

```powershell
.\tools\tui.ps1 -Action export
.\tools\tui.ps1 -Action check
```

`tui.ps1` also runs under PowerShell 7+ (`pwsh`) on Linux and macOS:

- data root is resolved per platform: Linux `~/.local/share/starlit-desktop`, macOS `~/Library/Application Support/starlit-desktop`, Windows `%APPDATA%\starlit-desktop`
- log export prefers `tar` + `zstd`, falling back to `.zip`
- WebView2 installation reports that the current platform does not need it
- integrity check also looks in `src-tauri/target/{release,debug}/starlit-foss`

`STARLIT_APP_ROOT` and `STARLIT_DATA_ROOT` override the app and data directories.

## Where your data lives

- Linux: `~/.local/share/starlit-desktop`
- Windows: `%APPDATA%\starlit-desktop`

It holds chat history, long-term memory, personas, avatars, stickers and model configuration. No telemetry, no analytics, no cloud account; the only outbound request is the model endpoint you configure.

**Settings → Data backup** exports a zip with the database, personas, character/group data and sticker index (API key excluded). Restore takes effect after a restart.

A lock file prevents two instances from writing the same data. If you see "system file is in use", another Starlit instance is still running.

---

## Design notes

The goal is not "a model that can chat" but a companion that **remembers you, has her own life and reaches out**.

So:

- replies arrive in several bubbles with pauses
- no sudden messages late at night
- if you stop replying, she gradually quiets down
- memories accumulate with the conversation instead of resetting every time

She is still a local program. The persona is a setting, memory is data, and the emotional experience is yours to define.

---

## Roadmap

1. Software updater
2. Android 10+ and FreeBSD x86_64
3. More automated tests
4. Scheduled / automatic backups
5. More group chat details (levels, permissions, in-group sharing)

---

## License

Released under the **GNU Affero General Public License v3.0 (AGPL-3.0)**. See [LICENSE](LICENSE).

- Modifying, distributing or offering the software over a network requires publishing the corresponding source under the same license
- Copyright of the original characters, personas and artwork remains with their respective holders and is unaffected by this project's license

## About

Starlit FOSS is a personal, local-first project with no accounts and no commercial plans.

It builds on reverse engineering of the original client: the frontend and interaction model are reused, while the backend is reimplemented in Rust. That keeps the familiar experience while removing the protocol layer and redundant processes.

The current version was developed with AI assistance (DeepSeek V4.1 Flash, 2026-11-06). The code is still being cleaned up; this does not represent the developer's personal coding ability.

Issues and pull requests are welcome.
