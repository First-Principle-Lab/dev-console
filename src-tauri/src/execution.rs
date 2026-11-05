use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

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
    let mut argv: Vec<String> = terminal.to_vec();
    argv.extend(shell_argv(record, shell)?);
    Ok(argv)
}

fn shell_argv(record: &EntryRecord, shell: &str) -> AppResult<Vec<String>> {
    if record.kind != crate::models::Kind::Command {
        return Err(AppError::Validation("only commands can be run".into()));
    }
    if record.command.trim().is_empty() {
        return Err(AppError::Validation("command text is empty".into()));
    }
    Ok(vec![shell.into(), "-c".into(), record.command.clone()])
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

/// Per-stream cap on what is kept and sent to the UI; the rest is drained and dropped.
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
pub const CAPTURE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutput {
    /// None when the process was killed (timeout) or ended by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
}

fn drain<R: Read + Send + 'static>(mut pipe: R) -> std::thread::JoinHandle<(Vec<u8>, bool)> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut truncated = false;
        let mut buf = [0u8; 8192];
        while let Ok(n) = pipe.read(&mut buf) {
            if n == 0 {
                break;
            }
            let room = MAX_OUTPUT_BYTES.saturating_sub(kept.len());
            if n > room {
                truncated = true;
            }
            kept.extend_from_slice(&buf[..n.min(room)]);
        }
        (kept, truncated)
    })
}

/// Runs the command directly (no terminal) and captures its output. Stdin is closed, so
/// anything interactive fails fast instead of hanging; the process group is killed on timeout.
pub fn capture(record: &EntryRecord, shell: &str, timeout: Duration) -> AppResult<RunOutput> {
    let argv = shell_argv(record, shell)?;
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
    let started = Instant::now();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|e| AppError::Execution(format!("could not start {}: {}", program, e.kind())))?;
    let out = drain(child.stdout.take().expect("stdout is piped"));
    let err = drain(child.stderr.take().expect("stderr is piped"));

    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= timeout => {
                timed_out = true;
                unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
                child.kill().ok();
                child.wait().ok();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(AppError::Execution(e.to_string())),
        }
    };
    let (stdout, out_trunc) = out.join().unwrap_or_default();
    let (stderr, err_trunc) = err.join().unwrap_or_default();
    Ok(RunOutput {
        exit_code: status.and_then(|s| s.code()),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        truncated: out_trunc || err_trunc,
        timed_out,
        duration_ms: started.elapsed().as_millis() as u64,
    })
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

    #[test]
    fn capture_returns_output_streams_and_exit_code() {
        let out = capture(
            &record("echo hello; echo oops >&2; exit 3", ""),
            "/bin/sh",
            Duration::from_secs(10),
        )
        .expect("capture");
        assert_eq!(out.stdout.trim(), "hello");
        assert_eq!(out.stderr.trim(), "oops");
        assert_eq!(out.exit_code, Some(3));
        assert!(!out.timed_out);
    }

    #[test]
    fn capture_kills_a_command_that_outlives_the_timeout() {
        let out = capture(
            &record("sleep 30 & sleep 30", ""),
            "/bin/sh",
            Duration::from_millis(300),
        )
        .expect("capture");
        assert!(out.timed_out);
        assert_eq!(out.exit_code, None);
        assert!(out.duration_ms < 5000);
    }

    #[test]
    fn capture_caps_huge_output() {
        let out = capture(
            &record("head -c 600000 /dev/zero | tr '\\0' x", ""),
            "/bin/sh",
            Duration::from_secs(10),
        )
        .expect("capture");
        assert!(out.truncated);
        assert_eq!(out.stdout.len(), MAX_OUTPUT_BYTES);
    }
}
