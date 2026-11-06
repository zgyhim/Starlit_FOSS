/* Tauri IPC 前端入口。所有数据都走命令，不存在 HTTP/WS 传输。 */
(() => {
  "use strict";

  const core = window.__TAURI__ && window.__TAURI__.core;
  const event = window.__TAURI__ && window.__TAURI__.event;
  if (!core) {
    console.error("Tauri API 不可用，请检查 withGlobalTauri 配置");
    return;
  }
  const invoke = core.invoke;

  const emitToApp = (payload) => {
    if (typeof window.__starlitOnEvent === "function") window.__starlitOnEvent(payload);
  };

  window.api = {
    identity: () => invoke("identity"),
    listCharacters: () => invoke("list_characters"),
    listGroups: () => invoke("list_groups"),
    listMessages: (conversation, beforeId, limit) =>
      invoke("list_messages", { conversation, beforeId, limit }),
    sendMessage: (payload) => invoke("send_message", { payload }),
    recallMessage: (conversation, messageId) =>
      invoke("recall_message", { conversation, messageId }),
    markRead: (conversation, readTs) => invoke("mark_read", { conversation, readTs }),
    retryReply: (conversation, clientId) => invoke("retry_reply", { conversation, clientId }),
    listMemories: (query, offset, limit) =>
      invoke("list_memories", { query, offset, limit }),
    createMemory: ({ text, category, pinned }) =>
      invoke("create_memory", { text, category, pinned }),
    updateMemory: ({ id, text, category, pinned, corrected }) =>
      invoke("update_memory", {
        id: String(id),
        text: text ?? null,
        category: category ?? null,
        pinned: pinned ?? null,
        corrected: corrected ?? false,
      }),
    deleteMemory: (id) => invoke("delete_memory", { id: String(id) }),
    characterDraftChat: (draftId, revision, text) =>
      invoke("character_draft_chat", { draftId, revision, text }),
    characterDraftGenerate: (name, avatar) =>
      invoke("character_draft_generate", { name, avatar }),
    diagnosticsTest: (target) => invoke("diagnostics_test", { target }),
    createBackup: () => invoke("create_backup"),
    restoreBackup: (data) => invoke("restore_backup", { data }),
    generateDiary: (peerId, day) => invoke("generate_diary", { peerId, day }),
    ping: () => invoke("ping"),
    mediaStatus: () => invoke("media_status"),
    mediaCacheInfo: () => invoke("media_cache_info"),
    setMediaAutoClear: (enabled) => invoke("set_media_auto_clear", { enabled }),
    clearMediaCache: () => invoke("clear_media_cache"),
    request: (url, options = {}) =>
      invoke("api_request", {
        url,
        method: options.method || "GET",
        body: options.body || null,
      }),
  };

  window.starlitDesktop = {
    windowControl: (action) => invoke("window_control", { action }),
    windowState: () => invoke("window_state"),
    onWindowState: (cb) => {
      if (event) event.listen("window:state", (e) => cb(e.payload));
    },
    openDataDir: () => invoke("open_data_dir"),
    readLogTail: (lines) => invoke("read_log_tail", { lines }),
  };

  window.addEventListener("error", (e) => {
    invoke("log_message", { message: `[ui] ${e.message} @ ${e.filename}:${e.lineno}` });
  });
  window.addEventListener("unhandledrejection", (e) => {
    invoke("log_message", { message: `[ui] unhandled: ${String(e.reason)}` });
  });

  if (event) {
    for (const name of ["message:new", "message:recalled", "message:status", "read:state"]) {
      event.listen(name, (e) => emitToApp(e.payload));
    }
  }
})();
