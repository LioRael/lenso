use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

use lenso_app::{ActionError, CallRecord, ErrorCode, Principal};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub id: String,
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct File {
    #[serde(flatten)]
    pub info: FileInfo,
    #[serde(serialize_with = "serialize_bytes")]
    pub content: Vec<u8>,
}

fn serialize_bytes<S: serde::Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    use base64::Engine;
    serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub struct Credentials {
    pub id: String,
    pub salt: String,
    pub hash: String,
}

#[derive(Debug, Clone)]
pub struct Store {
    db: Arc<Mutex<Connection>>,
    console_subject: Option<String>,
}

pub fn internal(error: impl std::error::Error + Send + Sync + 'static) -> ActionError {
    ActionError::new(ErrorCode::Internal, "Operation failed").with_source(error)
}

pub fn random_hex(length: usize) -> Result<String, ActionError> {
    let mut bytes = vec![0; length];
    getrandom::fill(&mut bytes).map_err(|error| {
        ActionError::new(ErrorCode::Internal, format!("Entropy unavailable: {error}"))
    })?;
    Ok(hex(&bytes))
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn now_ms() -> Result<i64, ActionError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(internal)?
            .as_millis(),
    )
    .map_err(internal)
}

impl Store {
    pub fn open(filename: &Path, console_subject: Option<String>) -> Result<Self, ActionError> {
        let db = Connection::open(filename).map_err(internal)?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(internal)?;
        db.execute_batch("
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
                salt TEXT NOT NULL, password_hash TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS sessions (
                token_hash TEXT PRIMARY KEY, subject TEXT NOT NULL REFERENCES users(id), expires_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY, owner TEXT NOT NULL REFERENCES users(id), name TEXT NOT NULL, content BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS calls (
                seq INTEGER PRIMARY KEY, call_id TEXT NOT NULL, action TEXT NOT NULL,
                subject TEXT, outcome TEXT NOT NULL, duration_ms REAL NOT NULL
            );
        ").map_err(internal)?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            console_subject,
        })
    }

    fn db(&self) -> Result<MutexGuard<'_, Connection>, ActionError> {
        self.db
            .lock()
            .map_err(|_| ActionError::new(ErrorCode::Internal, "Database lock poisoned"))
    }

    pub fn add_user(&self, name: String, salt: String, hash: String) -> Result<User, ActionError> {
        let id = random_hex(16)?;
        let changed = self.db()?.execute(
            "INSERT INTO users (id, name, salt, password_hash) VALUES (?, ?, ?, ?) ON CONFLICT(name) DO NOTHING",
            params![id, name, salt, hash],
        ).map_err(internal)?;
        if changed == 0 {
            return Err(ActionError::new(ErrorCode::Conflict, "User already exists"));
        }
        Ok(User { id, name })
    }

    pub fn credentials(&self, name: &str) -> Result<Option<Credentials>, ActionError> {
        self.db()?
            .query_row(
                "SELECT id, salt, password_hash FROM users WHERE name = ?",
                [name],
                |row| {
                    Ok(Credentials {
                        id: row.get(0)?,
                        salt: row.get(1)?,
                        hash: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(internal)
    }

    pub fn session(&self, subject: &str) -> Result<String, ActionError> {
        let token = random_hex(32)?;
        self.db()?
            .execute(
                "INSERT INTO sessions (token_hash, subject, expires_ms) VALUES (?, ?, ?)",
                params![
                    hex(&Sha256::digest(token.as_bytes())),
                    subject,
                    now_ms()? + 86_400_000
                ],
            )
            .map_err(internal)?;
        Ok(token)
    }

    pub fn authenticate(&self, token: Option<&str>) -> Result<Option<Principal>, ActionError> {
        let Some(token) = token else {
            return Ok(None);
        };
        let subject = self
            .db()?
            .query_row(
                "SELECT subject FROM sessions WHERE token_hash = ? AND expires_ms > ?",
                params![hex(&Sha256::digest(token.as_bytes())), now_ms()?],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        Ok(subject.map(|subject| {
            let mut scopes = vec!["files:read".into(), "files:write".into()];
            if self.console_subject.as_ref() == Some(&subject) {
                scopes.push("console:read".into());
            }
            Principal { subject, scopes }
        }))
    }

    pub fn upload(
        &self,
        owner: &str,
        name: String,
        content: Vec<u8>,
    ) -> Result<FileInfo, ActionError> {
        let id = random_hex(16)?;
        self.db()?
            .execute(
                "INSERT INTO files (id, owner, name, content) VALUES (?, ?, ?, ?)",
                params![id, owner, name, content],
            )
            .map_err(internal)?;
        Ok(FileInfo {
            id,
            owner: owner.to_owned(),
            name,
        })
    }

    pub fn read(&self, id: &str) -> Result<File, ActionError> {
        self.db()?
            .query_row(
                "SELECT id, owner, name, content FROM files WHERE id = ?",
                [id],
                |row| {
                    Ok(File {
                        info: FileInfo {
                            id: row.get(0)?,
                            owner: row.get(1)?,
                            name: row.get(2)?,
                        },
                        content: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(internal)?
            .ok_or_else(|| ActionError::new(ErrorCode::NotFound, "File not found"))
    }

    pub fn list(&self, owner: &str) -> Result<Vec<FileInfo>, ActionError> {
        self.db()?
            .prepare("SELECT id, owner, name FROM files WHERE owner = ? ORDER BY id")
            .map_err(internal)?
            .query_map([owner], |row| {
                Ok(FileInfo {
                    id: row.get(0)?,
                    owner: row.get(1)?,
                    name: row.get(2)?,
                })
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)
    }

    pub fn record(&self, record: CallRecord) -> Result<(), ActionError> {
        self.db()?.execute("INSERT INTO calls (call_id, action, subject, outcome, duration_ms) VALUES (?, ?, ?, ?, ?)",
            params![record.call_id, record.action, record.subject,
                record.outcome.map(|_| "ok").unwrap_or_else(|code| code.as_str()), record.duration.as_secs_f64() * 1000.0],
        ).map_err(internal)?;
        Ok(())
    }

    pub fn users(&self) -> Result<Vec<User>, ActionError> {
        self.db()?
            .prepare("SELECT id, name FROM users ORDER BY id LIMIT 100")
            .map_err(internal)?
            .query_map([], |row| {
                Ok(User {
                    id: row.get(0)?,
                    name: row.get(1)?,
                })
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)
    }

    pub fn files(&self) -> Result<Vec<FileInfo>, ActionError> {
        self.db()?
            .prepare("SELECT id, owner, name FROM files ORDER BY id LIMIT 100")
            .map_err(internal)?
            .query_map([], |row| {
                Ok(FileInfo {
                    id: row.get(0)?,
                    owner: row.get(1)?,
                    name: row.get(2)?,
                })
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)
    }

    pub fn requests(&self) -> Result<Vec<serde_json::Value>, ActionError> {
        self.db()?.prepare("SELECT call_id, action, subject, outcome, duration_ms FROM calls ORDER BY seq DESC LIMIT 100").map_err(internal)?
            .query_map([], |row| Ok(serde_json::json!({
                "call_id": row.get::<_, String>(0)?, "action": row.get::<_, String>(1)?,
                "subject": row.get::<_, Option<String>>(2)?, "outcome": row.get::<_, String>(3)?, "duration_ms": row.get::<_, f64>(4)?,
            }))).map_err(internal)?.collect::<Result<Vec<_>, _>>().map_err(internal)
    }
}
