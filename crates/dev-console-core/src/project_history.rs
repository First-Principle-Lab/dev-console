//! Project-aware shell activity. Only filtered commands reach this private store.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::error::{AppError, AppResult};
use crate::history::{self, Suggestion};
use crate::models::EntryRecord;

pub const MAX_PROJECTS: usize = 5;
pub const COMMANDS_PER_PROJECT: usize = 5;

pub fn project_for_directory(directory: &str) -> Option<crate::context::FinderContext> {
    let directory = crate::execution::expand_working_directory(directory).ok()??;
    let directory = std::fs::canonicalize(directory).ok()?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let root = crate::context::project_root(&directory, home.as_deref())?;
    let name = root.file_name()?.to_string_lossy().into_owned();
    Some(crate::context::FinderContext { root, name })
}

pub fn useful(command: &str) -> bool {
    if command.starts_with(char::is_whitespace) || command.len() > 20_000 {
        return false;
    }
    let command = command.trim();
    let words: Vec<_> = command.split_whitespace().collect();
    let Some(first) = words.first() else {
        return false;
    };
    let executable = first.rsplit('/').next().unwrap_or(first);
    // Routine navigation, editors and launchers add no useful project-specific task.
    if [
        "code",
        "code-insiders",
        "codium",
        "cursor",
        "open",
        "xdg-open",
        "gio",
        "cd",
        "ls",
        "pwd",
        "clear",
        "exit",
        "history",
        "dev-console",
    ]
    .contains(&executable)
    {
        return false;
    }
    if matches!(
        command,
        "git status" | "git status --short" | "git status -sb"
    ) {
        return false;
    }
    !history::looks_like_secret(command) && command.chars().count() >= 6
}

pub fn database_path() -> AppResult<PathBuf> {
    Ok(crate::paths::ensure_dir(&crate::paths::data_dir()?)?.join("project-history.sqlite3"))
}

pub struct ProjectHistory(Connection);

impl ProjectHistory {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path).map_err(AppError::storage)?;
        crate::paths::set_file_private(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(AppError::storage)?;
        conn.execute_batch(
            "PRAGMA journal_mode=DELETE;
            CREATE TABLE IF NOT EXISTS project_history (
                root TEXT NOT NULL, name TEXT NOT NULL, cwd TEXT NOT NULL,
                command TEXT NOT NULL, count INTEGER NOT NULL, last_seen INTEGER NOT NULL,
                PRIMARY KEY(root, cwd, command)
            );",
        )
        .map_err(AppError::storage)?;
        Ok(Self(conn))
    }

    pub fn record(&self, directory: &str, line: &str) -> AppResult<bool> {
        if !useful(line) {
            return Ok(false);
        }
        let Some(project) = project_for_directory(directory) else {
            return Ok(false);
        };
        let cwd = std::fs::canonicalize(directory)?;
        self.0
            .execute(
                "INSERT INTO project_history(root,name,cwd,command,count,last_seen)
            VALUES(?1,?2,?3,?4,1,?5) ON CONFLICT(root,cwd,command)
            DO UPDATE SET count=count+1,last_seen=excluded.last_seen,name=excluded.name",
                params![
                    project.root.to_string_lossy(),
                    project.name,
                    cwd.to_string_lossy(),
                    line.trim(),
                    crate::repository::unix_ms()
                ],
            )
            .map_err(AppError::storage)?;
        // Bound retained command/folder pairs. Aggregate counts, never an event log.
        self.0.execute("DELETE FROM project_history WHERE rowid IN (
            SELECT rowid FROM project_history ORDER BY last_seen DESC,root,cwd,command LIMIT -1 OFFSET 10000)",[]).map_err(AppError::storage)?;
        Ok(true)
    }

    pub fn suggest(
        &self,
        saved: &[EntryRecord],
        dismissed: &HashSet<String>,
        per_project: usize,
    ) -> AppResult<Vec<Suggestion>> {
        let mut statement = self
            .0
            .prepare(
                "SELECT root,name,SUM(count),MAX(last_seen) FROM project_history
            GROUP BY root ORDER BY SUM(count) DESC,MAX(last_seen) DESC,root LIMIT ?1",
            )
            .map_err(AppError::storage)?;
        let projects = statement
            .query_map([MAX_PROJECTS as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as usize,
                ))
            })
            .map_err(AppError::storage)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::storage)?;
        let mut saved_paths = HashMap::new();
        for record in saved {
            let path = crate::execution::expand_working_directory(&record.working_directory)
                .ok()
                .flatten()
                .and_then(|p| std::fs::canonicalize(p).ok());
            if let Some(path) = path {
                saved_paths.insert(
                    (
                        path.to_string_lossy().into_owned(),
                        record.command.trim().to_string(),
                    ),
                    (),
                );
            }
        }
        let mut out = Vec::new();
        for (root, name, total) in projects {
            let mut statement = self
                .0
                .prepare(
                    "SELECT cwd,command,count FROM project_history
                WHERE root=?1 ORDER BY count DESC,last_seen DESC,command,cwd",
                )
                .map_err(AppError::storage)?;
            let rows = statement
                .query_map([&root], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? as usize,
                    ))
                })
                .map_err(AppError::storage)?;
            let mut taken = 0;
            for row in rows {
                let (cwd, command, count) = row.map_err(AppError::storage)?;
                let key = history::key_for(&format!("{root}\0{cwd}\0{command}"));
                if !useful(&command)
                    || dismissed.contains(&key)
                    || saved_paths.contains_key(&(cwd.clone(), command.clone()))
                {
                    continue;
                }
                out.push(Suggestion {
                    key,
                    suggested_name: history::suggested_name(&command),
                    command,
                    count,
                    project: Some(name.clone()),
                    project_root: Some(root.clone()),
                    working_directory: Some(cwd),
                    project_count: total,
                });
                taken += 1;
                if taken >= per_project.clamp(1, COMMANDS_PER_PROJECT) {
                    break;
                }
            }
        }
        Ok(out)
    }
}

pub fn refresh_installed_zsh_hook() -> AppResult<()> {
    if crate::paths::config_dir()?
        .join("shell-history.zsh")
        .exists()
    {
        write_zsh_hook()?;
    }
    Ok(())
}

fn write_zsh_hook() -> AppResult<PathBuf> {
    let config = crate::paths::ensure_dir(&crate::paths::config_dir()?)?;
    let client = config.join("history-client.py");
    crate::settings::atomic_write(
        &client,
        include_bytes!("../../../scripts/history-client.py"),
    )?;
    let hook = config.join("shell-history.zsh");
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\"'\"'"));
    let source = include_str!("../../../scripts/shell-history.zsh")
        .replace("__DEV_CONSOLE_CLIENT__", &quote(&client.to_string_lossy()));
    crate::settings::atomic_write(&hook, source.as_bytes())?;
    Ok(hook)
}

pub fn install_zsh_hook() -> AppResult<String> {
    let shell = std::env::var("SHELL").unwrap_or_default();
    if !shell.ends_with("/zsh") {
        return Err(AppError::Validation("Automatic project tracking currently supports zsh. Set up the zsh hook from a zsh session.".into()));
    }
    let hook = write_zsh_hook()?;
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\"'\"'"));
    let home = std::env::var_os("ZDOTDIR")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| AppError::Validation("HOME is not set".into()))?;
    let rc = PathBuf::from(home).join(".zshrc");
    let rc = if rc.exists() {
        std::fs::canonicalize(&rc)?
    } else {
        rc
    };
    let previous = if rc.exists() {
        std::fs::read_to_string(&rc)?
    } else {
        String::new()
    };
    let marker = "# dev-console-project-history";
    if !previous.lines().any(|line| line.ends_with(marker)) {
        if rc.exists() {
            let backup = rc.with_file_name(format!(
                ".zshrc.dev-console-backup-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::copy(&rc, &backup)?;
            crate::paths::set_file_private(&backup)?;
        }
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&rc)?;
        let path = quote(&hook.to_string_lossy());
        write!(file, "\n[[ -r {path} ]] && source {path} {marker}\n")?;
        file.sync_all()?;
    }
    Ok("Tracking is ready. Open a new terminal to start project counts.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project(base: &Path, name: &str) -> PathBuf {
        let p = base.join(name);
        std::fs::create_dir_all(p.join(".git")).unwrap();
        p
    }
    #[test]
    fn counts_and_dismissals_are_project_and_folder_specific() {
        let temp = tempfile::tempdir().unwrap();
        let a = project(temp.path(), "a");
        let b = project(temp.path(), "b");
        let db = ProjectHistory::open(&temp.path().join("history.db")).unwrap();
        for _ in 0..3 {
            assert!(db.record(a.to_str().unwrap(), "bun run dev").unwrap());
        }
        db.record(b.to_str().unwrap(), "bun run dev").unwrap();
        let rows = db.suggest(&[], &HashSet::new(), 5).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].count, 3);
        assert_eq!(rows[1].count, 1);
        assert_ne!(rows[0].key, rows[1].key);
        let dismissed = HashSet::from([rows[0].key.clone()]);
        assert_eq!(
            db.suggest(&[], &dismissed, 5).unwrap()[0]
                .project
                .as_deref(),
            Some("b")
        );
        drop(db);
        let db = ProjectHistory::open(&temp.path().join("history.db")).unwrap();
        assert_eq!(db.suggest(&[], &HashSet::new(), 5).unwrap()[0].count, 3);
    }
    #[test]
    fn nested_folders_group_by_root_but_keep_execution_and_saved_identity() {
        let temp = tempfile::tempdir().unwrap();
        let a = project(temp.path(), "a");
        let b = project(temp.path(), "b");
        let nested = a.join("server");
        std::fs::create_dir(&nested).unwrap();
        let db = ProjectHistory::open(&temp.path().join("history.db")).unwrap();
        for dir in [&a, &nested, &b] {
            db.record(dir.to_str().unwrap(), "bun run dev").unwrap();
        }
        let saved = EntryRecord {
            working_directory: nested.to_string_lossy().into_owned(),
            command: "bun run dev".into(),
            id: "saved".into(),
            revision: 1,
            kind: crate::models::Kind::Command,
            name: "Dev".into(),
            entry_type: "Shell".into(),
            tags: vec![],
            description: String::new(),
            notes: String::new(),
            fields: vec![],
            pinned: false,
            value: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let rows = db.suggest(&[saved], &HashSet::new(), 5).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows
            .iter()
            .any(|row| row.project_root.as_deref() == a.to_str()
                && row.working_directory.as_deref() == a.to_str()));
        assert!(rows
            .iter()
            .any(|row| row.project_root.as_deref() == b.to_str()));
    }

    #[test]
    fn only_five_busiest_projects_and_five_commands_each() {
        let temp = tempfile::tempdir().unwrap();
        let db = ProjectHistory::open(&temp.path().join("history.db")).unwrap();
        for index in 1..=7 {
            let p = project(temp.path(), &format!("project-{index}"));
            for n in 0..7 {
                for _ in 0..index {
                    db.record(p.to_str().unwrap(), &format!("make target-{n}"))
                        .unwrap();
                }
            }
        }
        let rows = db.suggest(&[], &HashSet::new(), 99).unwrap();
        assert_eq!(rows.len(), 25);
        assert_eq!(rows[0].project.as_deref(), Some("project-7"));
        assert_eq!(rows[24].project.as_deref(), Some("project-3"));
    }
    #[test]
    fn launchers_secrets_and_unassigned_directories_are_never_stored() {
        let temp = tempfile::tempdir().unwrap();
        let db = ProjectHistory::open(&temp.path().join("history.db")).unwrap();
        let p = project(temp.path(), "app");
        for cmd in [
            "code .",
            "code --new-window .",
            "open code",
            "xdg-open .",
            "cd app",
            "git status",
            " make private",
            "export API_TOKEN=synthetic",
            "curl -H 'Authorization: Bearer synthetic' example.test",
        ] {
            assert!(!db.record(p.to_str().unwrap(), cmd).unwrap(), "{cmd}");
        }
        assert!(!db
            .record(temp.path().to_str().unwrap(), "bun run test")
            .unwrap());
        assert!(db.suggest(&[], &HashSet::new(), 5).unwrap().is_empty());
    }
}
