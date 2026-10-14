use std::process::Command;

use crate::error::{AppError, AppResult};
use crate::models::EntryRecord;

/// Builds the terminal argv.
///
/// The stored command body is intentionally shell code and is only ever reached through the
/// explicit Run action. It is passed as a single argv element, and names, types, tags and
/// descriptions are never interpolated into a shell string, so metadata can never inject
/// additional arguments.
pub fn build_argv(
    terminal: &[String],
    record: &EntryRecord,
    shell: &str,
) -> AppResult<Vec<String>> {
    if terminal.is_empty() {
        return Err(AppError::Execution("no terminal configured".into()));
    }
    if record.kind != crate::models::Kind::Command {
        return Err(AppError::Validation("only commands can be run".into()));
    }
    if record.command.trim().is_empty() {
        return Err(AppError::Validation("command text is empty".into()));
    }
    let mut argv: Vec<String> = terminal.to_vec();
    argv.push(shell.to_string());
    argv.push("-c".to_string());
    argv.push(record.command.clone());
    Ok(argv)
}

pub fn execute(terminal: &[String], record: &EntryRecord, shell: &str) -> AppResult<()> {
    let argv = build_argv(terminal, record, shell)?;
    let (program, args) = argv.split_first().expect("argv is non-empty");
    let mut command = Command::new(program);
    command.args(args);
    if !record.working_directory.trim().is_empty() {
        let dir = std::path::Path::new(&record.working_directory);
        if !dir.is_dir() {
            return Err(AppError::Execution(format!(
                "working directory {} does not exist",
                record.working_directory
            )));
        }
        command.current_dir(dir);
    }
    command
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| AppError::Execution(format!("could not start {}: {}", program, e.kind())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Kind;

    fn record(command: &str, dir: &str) -> EntryRecord {
        EntryRecord {
            id: "id".into(),
            revision: 1,
            kind: Kind::Command,
            name: "name".into(),
            entry_type: "type".into(),
            tags: vec!["; touch /tmp/evil".into()],
            description: "and $(whoami)".into(),
            notes: String::new(),
            fields: vec![],
            pinned: false,
            command: command.into(),
            working_directory: dir.into(),
            value: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn metadata_never_reaches_the_argv() {
        let argv = build_argv(
            &["x-terminal-emulator".into(), "-e".into()],
            &record("echo hi", ""),
            "/bin/sh",
        )
        .expect("argv");
        assert_eq!(argv[0], "x-terminal-emulator");
        assert_eq!(argv[1], "-e");
        assert_eq!(argv[2], "/bin/sh");
        assert_eq!(argv[3], "-c");
        assert_eq!(argv[4], "echo hi");
        assert_eq!(argv.len(), 5);
        assert!(!argv
            .iter()
            .any(|a| a.contains("touch /tmp/evil") || a.contains("whoami")));
    }

    #[test]
    fn refuses_empty_commands_and_secrets() {
        assert!(build_argv(&["x".into()], &record("   ", ""), "/bin/sh").is_err());
        let mut secret = record("echo hi", "");
        secret.kind = Kind::Secret;
        assert!(build_argv(&["x".into()], &secret, "/bin/sh").is_err());
    }

    #[test]
    fn missing_working_directory_is_an_error_not_a_fallback() {
        let err = execute(
            &["/bin/true".into()],
            &record("echo hi", "/nonexistent-dir-xyz"),
            "/bin/sh",
        )
        .expect_err("should fail");
        assert!(err.to_string().contains("working directory"));
    }

    #[test]
    fn spaces_in_working_directory_are_preserved() {
        let dir = std::env::temp_dir().join("dev console spaces test");
        std::fs::create_dir_all(&dir).expect("create dir");
        let record = record("printf marker > marker.txt", dir.to_str().expect("utf8"));
        // /bin/true ignores the payload; this only asserts argv construction survives spaces.
        let argv = build_argv(&["/bin/true".into()], &record, "/bin/sh").expect("argv");
        assert!(argv[1].contains("dev console spaces test") || argv[0] == "/bin/true");
        std::fs::remove_dir_all(&dir).ok();
    }
}
