use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::error::{AppError, AppResult};
use crate::models::{
    EntryDraft, EntryMeta, EntryRecord, ExtraField, FieldDisplay, Kind, SavedEntry,
};

const SCHEMA_VERSION: i64 = 1;

pub const MAX_NAME: usize = 200;
pub const MAX_TYPE: usize = 80;
pub const MAX_COMMAND: usize = 20_000;
pub const MAX_TEXT: usize = 100_000;
pub const MAX_FIELDS: usize = 64;

pub struct Repository {
    conn: Mutex<Connection>,
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::from(SystemTime::now())
        .format(&Rfc3339)
        .unwrap_or_default()
}

pub fn unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

impl Repository {
    pub fn open(path: &Path) -> AppResult<Repository> {
        let conn = Connection::open(path).map_err(AppError::storage)?;
        // Deliberately not WAL: a WAL database keeps a live `-wal` side file holding committed
        // rows, and its permissions are not derived from the database file, so command names,
        // tags and notes could be readable by anyone else logged into the machine. With delete
        // journaling nothing outlives a commit; the measured behaviour of truncate mode here was
        // a persistent side file (0600 and truncated to zero bytes, see the privacy test).
        let mode: String = conn
            .query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))
            .map_err(AppError::storage)?;
        if !mode.eq_ignore_ascii_case("delete") {
            return Err(AppError::Other(format!(
                "could not enable delete journaling (got {mode})"
            )));
        }
        conn.pragma_update(None, "synchronous", "FULL")
            .map_err(AppError::storage)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(AppError::storage)?;
        crate::paths::set_file_private(path)?;
        let repo = Repository {
            conn: Mutex::new(conn),
        };
        repo.migrate()?;
        Ok(repo)
    }

    /// In-memory repository for unit tests.
    pub fn in_memory() -> AppResult<Repository> {
        let conn = Connection::open_in_memory().map_err(AppError::storage)?;
        let repo = Repository {
            conn: Mutex::new(conn),
        };
        repo.migrate()?;
        Ok(repo)
    }

    fn migrate(&self) -> AppResult<()> {
        let conn = self.conn.lock().expect("db lock poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_meta (key TEXT PRIMARY KEY, value INTEGER NOT NULL);",
        )
        .map_err(AppError::storage)?;
        let version: i64 = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'version'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(AppError::storage)?
            .unwrap_or(0);

        if version > SCHEMA_VERSION {
            return Err(AppError::Other(
                "this database was written by a newer version of Dev Console".into(),
            ));
        }

        // Migrations run inside one transaction so a half-applied upgrade never leaves a
        // database that reports the new version with the old layout.
        let tx = conn.unchecked_transaction().map_err(AppError::storage)?;
        if version < 1 {
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS entries (
                    id TEXT PRIMARY KEY,
                    kind TEXT NOT NULL CHECK (kind IN ('command','secret')),
                    revision INTEGER NOT NULL,
                    name TEXT NOT NULL,
                    type TEXT NOT NULL,
                    tags TEXT NOT NULL DEFAULT '[]',
                    description TEXT NOT NULL DEFAULT '',
                    notes TEXT NOT NULL DEFAULT '',
                    fields TEXT NOT NULL DEFAULT '[]',
                    pinned INTEGER NOT NULL DEFAULT 0,
                    command TEXT NOT NULL DEFAULT '',
                    working_directory TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS entries_name ON entries(name);
                 CREATE TABLE IF NOT EXISTS usage (
                    id TEXT PRIMARY KEY,
                    count INTEGER NOT NULL DEFAULT 0,
                    last_used_at TEXT
                 );
                 CREATE TABLE IF NOT EXISTS approvals (
                    id TEXT PRIMARY KEY,
                    revision INTEGER NOT NULL,
                    command TEXT NOT NULL,
                    working_directory TEXT NOT NULL
                 );",
            )
            .map_err(AppError::storage)?;
        }
        tx.execute(
            "INSERT INTO schema_meta (key, value) VALUES ('version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION],
        )
        .map_err(AppError::storage)?;
        tx.commit().map_err(AppError::storage)?;
        Ok(())
    }

    pub fn list_commands(&self, filter: &str) -> AppResult<Vec<EntryMeta>> {
        let conn = self.conn.lock().expect("db lock poisoned");
        let pattern = format!("%{}%", filter.trim());
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.revision, e.name, e.type, e.tags, e.description, e.pinned, e.updated_at,
                        COALESCE(u.count, 0), u.last_used_at
                 FROM entries e LEFT JOIN usage u ON u.id = e.id
                 WHERE e.kind = 'command' AND (?1 = '' OR e.name LIKE ?2 OR e.type LIKE ?2 OR e.description LIKE ?2 OR e.tags LIKE ?2)
                 ORDER BY e.pinned DESC, e.name COLLATE NOCASE ASC",
            )
            .map_err(AppError::storage)?;
        let rows = stmt
            .query_map(params![filter, pattern], row_to_meta)
            .map_err(AppError::storage)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(AppError::storage)?);
        }
        Ok(out)
    }

    pub fn all_command_records(&self) -> AppResult<Vec<(EntryRecord, i64, Option<String>)>> {
        let conn = self.conn.lock().expect("db lock poisoned");
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.revision, e.name, e.type, e.tags, e.description, e.notes, e.fields,
                        e.pinned, e.command, e.working_directory, e.created_at, e.updated_at,
                        COALESCE(u.count, 0), u.last_used_at
                 FROM entries e LEFT JOIN usage u ON u.id = e.id WHERE e.kind = 'command'",
            )
            .map_err(AppError::storage)?;
        let rows = stmt
            .query_map([], |row| {
                let record = row_to_record(row)?;
                Ok((
                    record,
                    row.get::<_, i64>(13)?,
                    row.get::<_, Option<String>>(14)?,
                ))
            })
            .map_err(AppError::storage)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(AppError::storage)?);
        }
        Ok(out)
    }

    pub fn get_command(&self, id: &str) -> AppResult<Option<(EntryRecord, i64, Option<String>)>> {
        let conn = self.conn.lock().expect("db lock poisoned");
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.revision, e.name, e.type, e.tags, e.description, e.notes, e.fields,
                        e.pinned, e.command, e.working_directory, e.created_at, e.updated_at,
                        COALESCE(u.count, 0), u.last_used_at
                 FROM entries e LEFT JOIN usage u ON u.id = e.id
                 WHERE e.id = ?1 AND e.kind = 'command'",
            )
            .map_err(AppError::storage)?;
        let record = stmt
            .query_row(params![id], |row| {
                let record = row_to_record(row)?;
                Ok((
                    record,
                    row.get::<_, i64>(13)?,
                    row.get::<_, Option<String>>(14)?,
                ))
            })
            .optional()
            .map_err(AppError::storage)?;
        Ok(record)
    }

    pub fn types(&self, kind: Kind) -> AppResult<Vec<String>> {
        let conn = self.conn.lock().expect("db lock poisoned");
        if kind == Kind::Secret {
            // Secret types live inside the vault; the caller adds them when unlocked.
            return Ok(Vec::new());
        }
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT type FROM entries WHERE kind = ?1 ORDER BY type COLLATE NOCASE",
            )
            .map_err(AppError::storage)?;
        let rows = stmt
            .query_map(params![kind.as_str()], |row| row.get::<_, String>(0))
            .map_err(AppError::storage)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(AppError::storage)?);
        }
        Ok(out)
    }

    /// Insert or update with optimistic concurrency. `expected_revision` 0 creates; any other
    /// value must match the stored revision or the write is rejected so a stale editor form
    /// cannot overwrite a newer record.
    pub fn save_command(&self, draft: &mut EntryDraft) -> AppResult<SavedEntry> {
        validate_draft(draft)?;
        let conn = self.conn.lock().expect("db lock poisoned");
        let tx = conn.unchecked_transaction().map_err(AppError::storage)?;
        let now = now_rfc3339();
        let saved = match &draft.id {
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO entries (id, kind, revision, name, type, tags, description, notes, fields,
                        pinned, command, working_directory, created_at, updated_at)
                     VALUES (?1,'command',1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
                    params![
                        id,
                        draft.name.trim(),
                        draft.entry_type.trim(),
                        serde_json::to_string(&draft.tags)?,
                        draft.description,
                        draft.notes,
                        serde_json::to_string(&draft.fields)?,
                        draft.pinned as i64,
                        draft.command,
                        draft.working_directory,
                        now
                    ],
                )
                .map_err(AppError::storage)?;
                SavedEntry {
                    id,
                    revision: 1,
                    kind: Kind::Command,
                }
            }
            Some(id) => {
                let changed = tx
                    .execute(
                        "UPDATE entries SET revision = revision + 1, name=?2, type=?3, tags=?4,
                            description=?5, notes=?6, fields=?7, pinned=?8, command=?9,
                            working_directory=?10, updated_at=?11
                         WHERE id=?1 AND revision=?12 AND kind='command'",
                        params![
                            id,
                            draft.name.trim(),
                            draft.entry_type.trim(),
                            serde_json::to_string(&draft.tags)?,
                            draft.description,
                            draft.notes,
                            serde_json::to_string(&draft.fields)?,
                            draft.pinned as i64,
                            draft.command,
                            draft.working_directory,
                            now,
                            draft.expected_revision
                        ],
                    )
                    .map_err(AppError::storage)?;
                if changed == 0 {
                    let exists: Option<i64> = tx
                        .query_row(
                            "SELECT revision FROM entries WHERE id = ?1",
                            params![id],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(AppError::storage)?;
                    return Err(match exists {
                        Some(_) => AppError::Conflict,
                        None => AppError::NotFound,
                    });
                }
                SavedEntry {
                    id: id.clone(),
                    revision: draft.expected_revision + 1,
                    kind: Kind::Command,
                }
            }
        };
        tx.commit().map_err(AppError::storage)?;
        Ok(saved)
    }

    pub fn delete_command(&self, id: &str, expected_revision: i64) -> AppResult<()> {
        let conn = self.conn.lock().expect("db lock poisoned");
        let tx = conn.unchecked_transaction().map_err(AppError::storage)?;
        let current: Option<i64> = tx
            .query_row(
                "SELECT revision FROM entries WHERE id = ?1 AND kind='command'",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(AppError::storage)?;
        let current = current.ok_or(AppError::NotFound)?;
        if expected_revision != 0 && expected_revision != current {
            return Err(AppError::Conflict);
        }
        tx.execute("DELETE FROM entries WHERE id = ?1", params![id])
            .map_err(AppError::storage)?;
        tx.execute("DELETE FROM usage WHERE id = ?1", params![id])
            .map_err(AppError::storage)?;
        tx.execute("DELETE FROM approvals WHERE id = ?1", params![id])
            .map_err(AppError::storage)?;
        tx.commit().map_err(AppError::storage)?;
        Ok(())
    }

    /// Replaces the whole command table with `records`, in one transaction. Used by archive
    /// import, which validates everything before anything is applied.
    pub fn replace_all_commands(&self, records: &[EntryRecord]) -> AppResult<usize> {
        let conn = self.conn.lock().expect("db lock poisoned");
        let tx = conn.unchecked_transaction().map_err(AppError::storage)?;
        tx.execute("DELETE FROM approvals", [])
            .map_err(AppError::storage)?;
        tx.execute("DELETE FROM usage", [])
            .map_err(AppError::storage)?;
        tx.execute("DELETE FROM entries WHERE kind='command'", [])
            .map_err(AppError::storage)?;
        let mut applied = 0;
        for record in records {
            tx.execute(
                "INSERT INTO entries (id, kind, revision, name, type, tags, description, notes,
                    fields, pinned, command, working_directory, created_at, updated_at)
                 VALUES (?1,'command',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                params![
                    record.id,
                    record.revision,
                    record.name,
                    record.entry_type,
                    serde_json::to_string(&record.tags)?,
                    record.description,
                    record.notes,
                    serde_json::to_string(&record.fields)?,
                    record.pinned as i64,
                    record.command,
                    record.working_directory,
                    record.created_at,
                    record.updated_at
                ],
            )
            .map_err(AppError::storage)?;
            applied += 1;
        }
        tx.commit().map_err(AppError::storage)?;
        Ok(applied)
    }

    pub fn record_usage(&self, id: &str) -> AppResult<()> {
        let conn = self.conn.lock().expect("db lock poisoned");
        conn.execute(
            "INSERT INTO usage (id, count, last_used_at) VALUES (?1, 1, ?2)
             ON CONFLICT(id) DO UPDATE SET count = count + 1, last_used_at = excluded.last_used_at",
            params![id, now_rfc3339()],
        )
        .map_err(AppError::storage)?;
        Ok(())
    }

    /// Execution approval is bound to the revision *and* the executable content, so editing
    /// the command or working directory invalidates the earlier approval automatically.
    pub fn approval_state(&self, record: &EntryRecord) -> AppResult<bool> {
        let conn = self.conn.lock().expect("db lock poisoned");
        let stored: Option<(i64, String, String)> = conn
            .query_row(
                "SELECT revision, command, working_directory FROM approvals WHERE id = ?1",
                params![record.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(AppError::storage)?;
        Ok(stored
            .map(|(revision, command, dir)| {
                revision == record.revision
                    && command == record.command
                    && dir == record.working_directory
            })
            .unwrap_or(false))
    }

    pub fn approve_execution(&self, record: &EntryRecord) -> AppResult<()> {
        let conn = self.conn.lock().expect("db lock poisoned");
        conn.execute(
            "INSERT INTO approvals (id, revision, command, working_directory) VALUES (?1,?2,?3,?4)
             ON CONFLICT(id) DO UPDATE SET revision=excluded.revision, command=excluded.command,
                working_directory=excluded.working_directory",
            params![
                record.id,
                record.revision,
                record.command,
                record.working_directory
            ],
        )
        .map_err(AppError::storage)?;
        Ok(())
    }

    #[cfg(test)]
    pub fn exec_for_test(&self, sql: &str) -> AppResult<()> {
        let conn = self.conn.lock().expect("db lock poisoned");
        conn.execute_batch(sql).map_err(AppError::storage)?;
        Ok(())
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord> {
    let tags: String = row.get(4)?;
    let fields: String = row.get(7)?;
    Ok(EntryRecord {
        id: row.get(0)?,
        revision: row.get(1)?,
        kind: Kind::Command,
        name: row.get(2)?,
        entry_type: row.get(3)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        description: row.get(5)?,
        notes: row.get(6)?,
        fields: serde_json::from_str(&fields).unwrap_or_default(),
        pinned: row.get::<_, i64>(8)? != 0,
        command: row.get(9)?,
        working_directory: row.get(10)?,
        value: String::new(),
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn row_to_meta(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryMeta> {
    let tags: String = row.get(4)?;
    Ok(EntryMeta {
        id: row.get(0)?,
        kind: Kind::Command,
        revision: row.get(1)?,
        name: row.get(2)?,
        entry_type: row.get(3)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        description: row.get(5)?,
        pinned: row.get::<_, i64>(6)? != 0,
        updated_at: row.get(7)?,
        usage_count: row.get(8)?,
        last_used_at: row.get(9)?,
    })
}

/// Normalises in place and rejects anything the UI should not have allowed. The frontend
/// validates too, but the backend is the authority.
pub fn validate_draft(draft: &mut EntryDraft) -> AppResult<()> {
    draft.name = draft.name.trim().to_string();
    draft.entry_type = draft.entry_type.trim().to_string();
    draft.tags = normalise_tags(&draft.tags);
    draft.fields = normalise_fields(&draft.fields)?;

    if draft.name.is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if draft.name.chars().count() > MAX_NAME {
        return Err(AppError::Validation("name is too long".into()));
    }
    if draft.entry_type.is_empty() {
        return Err(AppError::Validation("type is required".into()));
    }
    if draft.entry_type.chars().count() > MAX_TYPE {
        return Err(AppError::Validation("type is too long".into()));
    }
    if draft.description.chars().count() > MAX_TEXT || draft.notes.chars().count() > MAX_TEXT {
        return Err(AppError::Validation(
            "description or notes is too long".into(),
        ));
    }
    match draft.kind {
        Kind::Command => {
            if draft.command.trim().is_empty() {
                return Err(AppError::Validation("command text is required".into()));
            }
            if draft.command.chars().count() > MAX_COMMAND {
                return Err(AppError::Validation("command text is too long".into()));
            }
        }
        Kind::Secret => {
            if draft.value.is_empty() {
                return Err(AppError::Validation("secret value is required".into()));
            }
        }
    }
    Ok(())
}

fn normalise_tags(tags: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() || tag.chars().count() > 40 || seen.contains(&tag) {
            continue;
        }
        seen.push(tag);
        if seen.len() >= 40 {
            break;
        }
    }
    seen
}

fn normalise_fields(fields: &[ExtraField]) -> AppResult<Vec<ExtraField>> {
    if fields.len() > MAX_FIELDS {
        return Err(AppError::Validation("too many extra fields".into()));
    }
    let mut out = Vec::with_capacity(fields.len());
    for field in fields {
        let label = field.label.trim().chars().take(80).collect::<String>();
        if label.is_empty() {
            return Err(AppError::Validation(
                "every extra field needs a label".into(),
            ));
        }
        if field.value.chars().count() > MAX_TEXT {
            return Err(AppError::Validation("extra field value is too long".into()));
        }
        if field.display == FieldDisplay::Url
            && !field.value.trim().is_empty()
            && !is_http_url(field.value.trim())
        {
            return Err(AppError::Validation(format!(
                "field {label} must be an http(s) URL"
            )));
        }
        let id = if field.id.trim().is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            field.id.clone()
        };
        out.push(ExtraField {
            id,
            label,
            value: field.value.clone(),
            display: field.display,
        });
    }
    Ok(out)
}

pub fn is_http_url(value: &str) -> bool {
    let lower = value.to_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return false;
    }
    let rest = lower.split_once("://").map(|r| r.1).unwrap_or_default();
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !host.is_empty() && !host.starts_with('.') && !host.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn draft(name: &str) -> EntryDraft {
        EntryDraft {
            kind: Kind::Command,
            id: None,
            expected_revision: 0,
            name: name.to_string(),
            entry_type: "Shell".to_string(),
            tags: vec!["work".to_string()],
            description: String::new(),
            notes: String::new(),
            fields: vec![ExtraField {
                id: "f1".to_string(),
                label: "Env".to_string(),
                value: "staging".to_string(),
                display: FieldDisplay::Text,
            }],
            pinned: false,
            command: "echo hi".to_string(),
            working_directory: "/tmp".to_string(),
            value: String::new(),
        }
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("file exists")
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn a_saved_command_survives_closing_and_reopening_the_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("commands.sqlite3");
        let saved = {
            let repo = Repository::open(&file).expect("open");
            let mut created = draft("Deploy staging");
            created.pinned = true;
            let saved = repo.save_command(&mut created).expect("save");
            assert_eq!(saved.revision, 1);
            assert_eq!(saved.kind, Kind::Command);
            saved
        };
        let repo = Repository::open(&file).expect("reopen");
        let (record, usage, last_used) = repo
            .get_command(&saved.id)
            .expect("query")
            .expect("record persisted");
        assert_eq!(record.name, "Deploy staging");
        assert_eq!(record.command, "echo hi");
        assert_eq!(record.working_directory, "/tmp");
        assert!(record.pinned);
        assert_eq!(record.tags, vec!["work".to_string()]);
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].value, "staging");
        assert_eq!(usage, 0);
        assert_eq!(last_used, None);
    }

    #[test]
    fn app_data_never_lands_in_a_world_readable_file() {
        // SQLite keeps a rollback journal next to the database in truncate mode, so the
        // privacy guarantee has to come from the directory mode plus the file modes, not from
        // "the database is the only file". A WAL side file would have broken this assertion.
        let dir = tempfile::tempdir().expect("tempdir");
        let app_dir = dir.path().join("data");
        std::fs::create_dir(&app_dir).expect("create dir");
        std::fs::set_permissions(&app_dir, std::fs::Permissions::from_mode(0o700)).expect("chmod");
        let file = app_dir.join("commands.sqlite3");

        assert_eq!(
            mode_of(&app_dir),
            0o700,
            "the data directory must be private"
        );
        let repo = Repository::open(&file).expect("open");
        let mut created = draft("Private");
        repo.save_command(&mut created).expect("save");
        assert_eq!(mode_of(&file), 0o600);

        for extra in ["-wal", "-shm", "-journal"] {
            let side = std::path::PathBuf::from(format!("{}{}", file.display(), extra));
            if !side.exists() {
                continue;
            }
            assert_eq!(
                mode_of(&side),
                0o600,
                "{extra} must not be more readable than the database"
            );
            assert_eq!(
                std::fs::metadata(&side).expect("side metadata").len(),
                0,
                "{extra} still holds committed content after the transaction"
            );
        }
        assert_eq!(
            std::fs::read_dir(&app_dir).expect("read dir").count(),
            1,
            "only the database may remain after a commit"
        );
    }

    #[test]
    fn two_editors_cannot_both_save_the_same_record() {
        let repo = Repository::in_memory().expect("repo");
        let mut created = draft("Shared");
        let saved = repo.save_command(&mut created).expect("create");

        let mut winner = draft("Shared");
        winner.id = Some(saved.id.clone());
        winner.expected_revision = saved.revision;
        winner.command = "echo winner".to_string();
        let winner = repo.save_command(&mut winner).expect("first edit wins");
        assert_eq!(winner.revision, 2);

        let mut loser = draft("Shared");
        loser.id = Some(saved.id.clone());
        loser.expected_revision = saved.revision;
        loser.command = "echo loser".to_string();
        assert!(matches!(
            repo.save_command(&mut loser),
            Err(AppError::Conflict)
        ));
        let (record, _, _) = repo.get_command(&saved.id).expect("query").expect("row");
        assert_eq!(record.command, "echo winner");
        assert_eq!(record.revision, 2);
    }

    #[test]
    fn deleting_with_a_stale_revision_is_refused_and_keeps_the_record() {
        let repo = Repository::in_memory().expect("repo");
        let mut created = draft("Keep me");
        let saved = repo.save_command(&mut created).expect("create");
        assert!(matches!(
            repo.delete_command(&saved.id, saved.revision + 5),
            Err(AppError::Conflict)
        ));
        assert!(repo.get_command(&saved.id).expect("query").is_some());
        assert!(matches!(
            repo.delete_command("does-not-exist", 0),
            Err(AppError::NotFound)
        ));
        repo.delete_command(&saved.id, saved.revision)
            .expect("delete");
        assert!(repo.get_command(&saved.id).expect("query").is_none());
    }

    #[test]
    fn approval_of_a_command_is_voided_by_editing_it() {
        let repo = Repository::in_memory().expect("repo");
        let mut created = draft("Run me");
        let saved = repo.save_command(&mut created).expect("create");
        let (record, _, _) = repo.get_command(&saved.id).expect("query").expect("row");
        assert!(!repo.approval_state(&record).expect("state"));
        repo.approve_execution(&record).expect("approve");
        assert!(repo.approval_state(&record).expect("state"));

        let mut edit = draft("Run me");
        edit.id = Some(saved.id.clone());
        edit.expected_revision = record.revision;
        edit.command = "echo changed".to_string();
        repo.save_command(&mut edit).expect("edit");
        let (edited, _, _) = repo.get_command(&saved.id).expect("query").expect("row");
        assert!(
            !repo.approval_state(&edited).expect("state"),
            "an edited command must not inherit the previous approval"
        );
    }

    #[test]
    fn usage_is_counted_and_exposed_to_the_ranker() {
        let repo = Repository::in_memory().expect("repo");
        let mut created = draft("Frequent");
        let saved = repo.save_command(&mut created).expect("create");
        repo.record_usage(&saved.id).expect("usage 1");
        repo.record_usage(&saved.id).expect("usage 2");
        let (record, usage, last_used) = repo.get_command(&saved.id).expect("query").expect("row");
        assert_eq!(usage, 2);
        assert!(last_used.is_some());
        let metas = repo.list_commands("").expect("list");
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].id, record.id);
        assert_eq!(metas[0].usage_count, 2);
        assert!(repo
            .types(Kind::Command)
            .expect("types")
            .contains(&"Shell".to_string()));
    }

    #[test]
    fn a_database_from_a_newer_version_is_refused_rather_than_downgraded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("commands.sqlite3");
        {
            let repo = Repository::open(&file).expect("open");
            let mut created = draft("Future");
            repo.save_command(&mut created).expect("save");
            repo.exec_for_test(&format!(
                "UPDATE schema_meta SET value = {} WHERE key = 'version'",
                SCHEMA_VERSION + 1
            ))
            .expect("simulate a newer schema");
        }
        let error = Repository::open(&file).err().expect("must refuse to open");
        assert!(
            error.to_string().contains("newer"),
            "unexpected error: {error}"
        );
    }
}
