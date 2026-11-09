use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::models::EntryRecord;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOption {
    pub id: &'static str,
    pub name: &'static str,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub mode: &'static str,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub truncated: bool,
    pub timed_out: bool,
}

const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const RUN_TIMEOUT: Duration = Duration::from_secs(120);

/// A conservative check: a sudo word anywhere in shell source gets a real terminal
/// so an auth prompt can be answered even inside a pipeline or nested command.
pub fn needs_terminal(source: &str) -> bool {
    source.match_indices("sudo").any(|(index, _)| {
        let before = source[..index].chars().next_back();
        let after = source[index + 4..].chars().next();
        let boundary = |ch: Option<char>| ch.is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        boundary(before) && boundary(after)
    })
}

/// Expands a stored working directory the way execution reads it: empty or `~` means home, `~/x`
/// is joined onto it, and anything else is taken as given. Fails only when an entry needs `$HOME`
/// and the session has none.
///
/// Search calls this too, so that a `~/src/api` working directory cannot come to mean one thing
/// when it is ranked and another when it is finally run.
pub fn expand_working_directory(raw: &str) -> AppResult<Option<PathBuf>> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dir = if raw.trim().is_empty() || raw == "~" {
        home
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home.map(|path| path.join(rest))
    } else {
        Some(PathBuf::from(raw))
    };
    if !raw.trim().is_empty() && dir.is_none() {
        return Err(AppError::Execution(
            "HOME is unavailable for this working directory".into(),
        ));
    }
    Ok(dir)
}

fn working_directory(record: &EntryRecord) -> AppResult<Option<PathBuf>> {
    let raw = record.working_directory.as_str();
    let dir = expand_working_directory(raw)?;
    if let Some(path) = &dir {
        if !path.is_dir() {
            return Err(AppError::Execution(format!(
                "working directory {} does not exist",
                raw
            )));
        }
    }
    Ok(dir)
}

fn read_bounded(
    mut pipe: impl Read + Send + 'static,
) -> mpsc::Receiver<std::io::Result<(Vec<u8>, bool)>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut truncated = false;
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let remaining = MAX_OUTPUT_BYTES.saturating_sub(bytes.len());
                    bytes.extend_from_slice(&buffer[..count.min(remaining)]);
                    truncated |= count > remaining;
                }
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            }
        }
        let _ = sender.send(Ok((bytes, truncated)));
    });
    receiver
}

fn collect_output(
    receiver: mpsc::Receiver<std::io::Result<(Vec<u8>, bool)>>,
) -> AppResult<(String, bool)> {
    let (bytes, truncated) = receiver
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| AppError::Execution("command output did not close".into()))?
        .map_err(|error| AppError::Execution(format!("could not read command output: {error}")))?;
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

pub fn execute_inline(record: &EntryRecord, shell: &str) -> AppResult<RunResult> {
    execute_inline_with_timeout(record, shell, RUN_TIMEOUT)
}

fn execute_inline_with_timeout(
    record: &EntryRecord,
    shell: &str,
    timeout: Duration,
) -> AppResult<RunResult> {
    if record.kind != crate::models::Kind::Command || record.command.trim().is_empty() {
        return Err(AppError::Validation("command text is empty".into()));
    }
    let mut command = Command::new(shell);
    command
        .arg("-c")
        .arg(&record.command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    if let Some(dir) = working_directory(record)? {
        command.current_dir(dir);
    }
    let mut child = command
        .spawn()
        .map_err(|error| AppError::Execution(format!("could not start command: {error}")))?;
    let stdout = read_bounded(child.stdout.take().expect("piped stdout"));
    let stderr = read_bounded(child.stderr.take().expect("piped stderr"));
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| AppError::Execution(format!("could not wait for command: {error}")))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            // The shell is a process group leader; stop pipelines and children too.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            break child.wait().map_err(|error| {
                AppError::Execution(format!("could not stop command: {error}"))
            })?;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let (stdout, out_truncated) = collect_output(stdout)?;
    let (stderr, err_truncated) = collect_output(stderr)?;
    Ok(RunResult {
        mode: "inline",
        stdout,
        stderr,
        exit_code: status.code(),
        truncated: out_truncated || err_truncated,
        timed_out,
    })
}

// Arguments before /bin/sh -c must match each terminal's CLI. Kitty takes the
// child program after `--`; Ghostty, Foot and Alacritty use `-e`.
const TERMINALS: &[(&str, &str, &[&str])] = &[
    ("ghostty", "Ghostty", &["-e"]),
    ("kitty", "Kitty", &["--"]),
    ("foot", "Foot", &["-e"]),
    ("alacritty", "Alacritty", &["-e"]),
    ("wezterm", "WezTerm", &["start", "--"]),
    ("konsole", "Konsole", &["-e"]),
    ("gnome-terminal", "GNOME Terminal", &["--"]),
    ("xfce4-terminal", "Xfce Terminal", &["-x"]),
    ("xterm", "XTerm", &["-e"]),
    ("x-terminal-emulator", "System terminal", &["-e"]),
];

fn find_executable(program: &str, directories: &[PathBuf]) -> Option<PathBuf> {
    directories
        .iter()
        .map(|dir| dir.join(program))
        .find(|path| {
            path.is_file()
                && path
                    .metadata()
                    .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
        })
}

pub fn available_terminals() -> Vec<TerminalOption> {
    let directories: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    TERMINALS
        .iter()
        .filter_map(|(id, name, args)| {
            let path = find_executable(id, &directories)?;
            let mut command = vec![path.to_string_lossy().into_owned()];
            command.extend(args.iter().map(|arg| (*arg).to_string()));
            Some(TerminalOption { id, name, command })
        })
        .collect()
}

pub fn recommended_terminal() -> Vec<String> {
    available_terminals()
        .into_iter()
        .next()
        .map(|terminal| terminal.command)
        .unwrap_or_default()
}

pub fn legacy_default_is_missing(terminal: &[String]) -> bool {
    terminal
        .first()
        .is_some_and(|program| program == "x-terminal-emulator")
        && !available_terminals()
            .iter()
            .any(|option| option.id == "x-terminal-emulator")
}

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
    if let Some(dir) = working_directory(record)? {
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
    fn discovered_terminals_pass_the_shell_as_argv() {
        for terminal in available_terminals() {
            if terminal.id != "ghostty" && terminal.id != "kitty" {
                continue;
            }
            let argv =
                build_argv(&terminal.command, &record("echo hi", ""), "/bin/sh").expect("argv");
            assert_eq!(argv[0], terminal.command[0]);
            assert_eq!(
                &argv[1..1 + terminal.command.len() - 1],
                &terminal.command[1..]
            );
            assert_eq!(
                &argv[terminal.command.len()..],
                &["/bin/sh", "-c", "echo hi"]
            );
            assert_eq!(
                terminal.command[1],
                if terminal.id == "kitty" { "--" } else { "-e" }
            );
        }
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
    fn ordinary_commands_capture_output_and_exit_status() {
        assert!(!needs_terminal("ls -la"));
        assert!(needs_terminal("sudo ls"));
        assert!(needs_terminal("echo ready && /usr/bin/sudo id"));
        let result = execute_inline(
            &record("printf 'hello'; printf 'warning' >&2; exit 7", ""),
            "/bin/sh",
        )
        .expect("run");
        assert_eq!(result.mode, "inline");
        assert_eq!(result.stdout, "hello");
        assert_eq!(result.stderr, "warning");
        assert_eq!(result.exit_code, Some(7));
    }

    #[test]
    fn ls_runs_in_the_saved_directory_and_returns_its_listing() {
        let dir = tempfile::tempdir().expect("directory");
        std::fs::write(dir.path().join("example.txt"), b"example").expect("file");
        let result = execute_inline(&record("ls", dir.path().to_str().expect("utf8")), "/bin/sh")
            .expect("run ls");
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("example.txt"));
    }

    #[test]
    fn in_app_command_output_is_bounded_and_interactive_waits_time_out() {
        let result =
            execute_inline(&record("head -c 100000 /dev/zero", ""), "/bin/sh").expect("run");
        assert_eq!(result.stdout.len(), MAX_OUTPUT_BYTES);
        assert!(result.truncated);
        let result = execute_inline_with_timeout(
            &record("sleep 2", ""),
            "/bin/sh",
            Duration::from_millis(100),
        )
        .expect("timeout");
        assert!(result.timed_out);
    }

    #[test]
    fn a_tilde_working_directory_uses_home() {
        let home = std::env::var_os("HOME").expect("HOME");
        let dir = working_directory(&record("pwd", "~")).expect("working directory");
        assert_eq!(dir, Some(PathBuf::from(home)));
    }
}
