use std::sync::Mutex;

use rusqlite::Connection;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("打开数据库失败: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| e.to_string())?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute_batch(SCHEMA).map_err(|e| format!("建表失败: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS shared_diaries_v2 (
               peer_id TEXT NOT NULL DEFAULT '',
               day TEXT NOT NULL,
               title TEXT NOT NULL,
               content TEXT NOT NULL,
               source_turn_ids TEXT NOT NULL DEFAULT '[]',
               message_count INTEGER NOT NULL DEFAULT 0,
               generated_at REAL NOT NULL,
               PRIMARY KEY(peer_id, day)
             );",
        )
        .map_err(|e| e.to_string())?;
        for table in ["shared_agreements", "shared_anniversaries"] {
            let has_peer = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .and_then(|mut stmt| {
                    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
                    let mut found = false;
                    for row in rows {
                        if row? == "peer_id" {
                            found = true;
                            break;
                        }
                    }
                    Ok(found)
                })
                .unwrap_or(false);
            if !has_peer {
                let _ = conn.execute_batch(&format!(
                    "ALTER TABLE {table} ADD COLUMN peer_id TEXT NOT NULL DEFAULT ''"
                ));
            }
        }
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|e| e.to_string())?;
        if version < 1 {
            conn.pragma_update(None, "user_version", 1)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        f(&conn).map_err(|e| e.to_string())
    }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_kind TEXT NOT NULL CHECK(conversation_kind IN ('Private','Group')),
  conversation_id TEXT NOT NULL,
  direction TEXT NOT NULL CHECK(direction IN ('Incoming','Outgoing')),
  author_id TEXT NOT NULL DEFAULT '',
  author_name TEXT NOT NULL DEFAULT '',
  parts TEXT NOT NULL DEFAULT '[]',
  text TEXT NOT NULL DEFAULT '',
  reply_to INTEGER,
  sent_at INTEGER NOT NULL,
  client_id TEXT,
  status TEXT NOT NULL DEFAULT 'Sent'
);
CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_kind, conversation_id, sent_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_client ON messages(conversation_kind, conversation_id, client_id)
  WHERE client_id IS NOT NULL AND client_id <> '';

CREATE TABLE IF NOT EXISTS read_state (
  conversation_kind TEXT NOT NULL,
  conversation_id TEXT NOT NULL,
  read_ts INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(conversation_kind, conversation_id)
);

CREATE TABLE IF NOT EXISTS schema_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','closed')),
  topic TEXT NOT NULL DEFAULT '',
  summary TEXT NOT NULL DEFAULT '',
  started_at REAL NOT NULL DEFAULT 0,
  ended_at REAL
);

CREATE TABLE IF NOT EXISTS transcript (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts REAL NOT NULL,
  role TEXT NOT NULL,
  text TEXT NOT NULL,
  reply_mid TEXT DEFAULT '',
  media TEXT DEFAULT '',
  client_msg_id TEXT,
  session_id INTEGER
);
CREATE INDEX IF NOT EXISTS idx_transcript_ts ON transcript(ts);
CREATE INDEX IF NOT EXISTS idx_transcript_session ON transcript(session_id);

CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts REAL NOT NULL,
  kind TEXT NOT NULL,
  text TEXT NOT NULL,
  emotion TEXT,
  importance REAL DEFAULT 0.5,
  flagged INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS memories (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts REAL NOT NULL,
  kind TEXT NOT NULL,
  text TEXT NOT NULL,
  vec TEXT NOT NULL,
  emotion TEXT,
  weight REAL DEFAULT 1.0,
  text_hash TEXT,
  source_turn_id INTEGER,
  updated_at REAL,
  root_id INTEGER,
  previous_id INTEGER,
  version INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'active',
  superseded_at REAL
);
CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);

CREATE TABLE IF NOT EXISTS memory_meta (
  source TEXT NOT NULL,
  local_id TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  category TEXT NOT NULL DEFAULT 'auto',
  updated_at REAL NOT NULL,
  corrected_at REAL,
  PRIMARY KEY(source, local_id)
);

CREATE TABLE IF NOT EXISTS profile (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS kv (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS counters (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS self_fact_records (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  key TEXT NOT NULL,
  value TEXT NOT NULL,
  value_hash TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'candidate'
    CHECK(status IN ('candidate','confirmed','conflict','superseded')),
  created_at REAL NOT NULL DEFAULT 0,
  updated_at REAL NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS gmsg (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts REAL NOT NULL,
  kind TEXT NOT NULL,
  sender_id TEXT DEFAULT '',
  sender_name TEXT DEFAULT '',
  text TEXT DEFAULT '',
  segs TEXT DEFAULT '',
  reply_to INTEGER DEFAULT 0,
  mid TEXT DEFAULT '',
  revoked INTEGER DEFAULT 0,
  client_msg_id TEXT
);
CREATE INDEX IF NOT EXISTS idx_gmsg_ts ON gmsg(ts);

CREATE TABLE IF NOT EXISTS gres (
  uid TEXT PRIMARY KEY,
  last_gid INTEGER DEFAULT 0,
  last_ts REAL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS shared_agreements (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  details TEXT NOT NULL DEFAULT '',
  due_at REAL NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK(status IN ('pending','confirmed','followed_up'))
);

CREATE TABLE IF NOT EXISTS shared_anniversaries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  date TEXT NOT NULL,
  yearly INTEGER NOT NULL DEFAULT 0,
  note TEXT NOT NULL DEFAULT '',
  created_at REAL NOT NULL,
  updated_at REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS shared_diaries (
  day TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  content TEXT NOT NULL,
  source_turn_ids TEXT NOT NULL,
  message_count INTEGER NOT NULL,
  generated_at REAL NOT NULL
);
"#;
