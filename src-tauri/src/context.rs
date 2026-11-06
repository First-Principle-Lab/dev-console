//! Which project the terminal the finder was opened from belongs to.
//!
//! The context is worked out from scratch on every finder open and then lives in memory for the
//! length of that session only: nothing here is written to disk, and everything read is local to
//! the user's own session (the compositor's focused-window answer and `/proc`).
//!
//! The chain is
//!
//! ```text
//! focused window pid -> a shell that terminal started -> its cwd -> the project that directory is in
//! ```
//!
//! and every step that fails means "no context", which leaves ranking exactly as it was before
//! this existed. Two limits are deliberate:
//!
//! * Only Hyprland answers "which window is focused". Other compositors would need
//!   `_NET_ACTIVE_WINDOW` over X11, and the Wayland protocols deliberately do not expose focus to
//!   clients, so off Hyprland the answer is `None` rather than a guess.
//! * The shell is found by walking the terminal's children, so a terminal that hands its shells to
//!   a server process (tmux, or `fish` under a supervisor) has no shell under its own pid and
//!   detection stops there. Ghostty, Kitty, Foot and Alacritty all parent their shells directly.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

/// How long the compositor is given to answer, over its socket or through `hyprctl`.
const FOCUS_TIMEOUT: Duration = Duration::from_millis(200);
/// How far below the window process the shell search looks. A terminal's own children are depth 1,
/// which is where a shell normally sits; going deeper only ever finds what that shell launched.
const MAX_DEPTH: usize = 3;
/// Shells recognised as "a place a project is open in". Login shells appear in `comm` with a
/// leading `-`, which [`shell_name`] strips.
const SHELLS: [&str; 4] = ["zsh", "bash", "fish", "nu"];
/// A directory with one of these in it is a project even when it is not a repository.
const PROJECT_FILES: [&str; 5] = [
    "package.json",
    "Makefile",
    "justfile",
    "compose.yaml",
    "Cargo.toml",
];

/// The project the finder was opened from. `root` is absolute because it comes from a `cwd`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinderContext {
    pub root: PathBuf,
    pub name: String,
}

impl FinderContext {
    fn from_root(root: &Path) -> Option<FinderContext> {
        let name = root.file_name()?.to_string_lossy().trim().to_string();
        if name.is_empty() {
            return None;
        }
        Some(FinderContext {
            root: root.to_path_buf(),
            name,
        })
    }
}

/// The window the compositor says has focus. `class` is what the app calls itself, and is only
/// used to notice that the focused window is Dev Console's own palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusedWindow {
    pub pid: u32,
    pub class: String,
}

/// Where the focused window comes from. A trait so the walker can be tested with any pid, on a
/// machine with no compositor at all.
pub trait FocusReader {
    fn focused(&self) -> Option<FocusedWindow>;
}

/// Reads the focused window from Hyprland, preferring its IPC socket over spawning `hyprctl`.
pub struct HyprlandFocus {
    socket: Option<PathBuf>,
}

impl HyprlandFocus {
    /// `None` socket is not an error: it just means the `hyprctl` fallback will be used.
    pub fn from_env() -> HyprlandFocus {
        HyprlandFocus {
            socket: socket_path(),
        }
    }
}

impl FocusReader for HyprlandFocus {
    fn focused(&self) -> Option<FocusedWindow> {
        let reply = self
            .socket
            .as_deref()
            .and_then(query_socket)
            .or_else(query_hyprctl)?;
        parse_active_window(&reply)
    }
}

/// Detects the project behind the window that had focus when the finder was asked to open.
///
/// Runs on a background thread (see [`crate::windows::show_finder`]) because the palette must not
/// wait for a compositor, and returns `None` - "rank like before" - whenever any step is missing.
pub fn detect() -> Option<FinderContext> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    detect_with(
        &HyprlandFocus::from_env(),
        Path::new("/proc"),
        home.as_deref(),
    )
}

/// The whole chain, with every outside input supplied by the caller.
pub fn detect_with(
    reader: &dyn FocusReader,
    proc_root: &Path,
    home: Option<&Path>,
) -> Option<FinderContext> {
    let window = reader.focused()?;
    // Re-opening the palette from the palette itself says nothing about a project.
    if window.class.to_lowercase().contains("dev-console") {
        return None;
    }
    let cwd = shell_cwd(proc_root, window.pid)?;
    project_root(&cwd, home).and_then(|root| FinderContext::from_root(&root))
}

/// The cwd of the youngest shell the given window process started, or `None` when it has none
/// within [`MAX_DEPTH`] levels. "Youngest" matters because every tab and split is a sibling shell
/// under the same terminal, and the one just used is the newest.
pub fn shell_cwd(proc_root: &Path, window_pid: u32) -> Option<PathBuf> {
    let mut found: Option<(u64, u32)> = None;
    let mut level = children(proc_root, window_pid);
    for _ in 0..MAX_DEPTH {
        if level.is_empty() {
            break;
        }
        for pid in &level {
            if !is_shell(proc_root, *pid) {
                continue;
            }
            let candidate = (start_time(proc_root, *pid), *pid);
            if found.is_none_or(|best| candidate > best) {
                found = Some(candidate);
            }
        }
        level = level
            .iter()
            .flat_map(|pid| children(proc_root, *pid))
            .collect();
    }
    let (_, pid) = found?;
    fs::read_link(proc_root.join(pid.to_string()).join("cwd")).ok()
}

/// The project `directory` belongs to: the nearest ancestor that is a repository, else the
/// directory itself when it holds a project file, else nothing.
///
/// `$HOME` is the stop line and is never itself the answer - a stray `.git` or `package.json` in
/// the home folder would otherwise claim every terminal in the session.
pub fn project_root(directory: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let mut current = Some(directory.to_path_buf());
    while let Some(dir) = current {
        if home.is_some_and(|home| home == dir) {
            return None;
        }
        if dir.join(".git").exists() {
            return Some(dir);
        }
        current = dir.parent().map(|parent| parent.to_path_buf());
    }
    PROJECT_FILES
        .iter()
        .any(|marker| directory.join(marker).exists())
        .then(|| directory.to_path_buf())
}

/// Direct children of a process, read the way `ps --ppid` does: the union of every thread's
/// `children` list. A missing file just means the kernel has no `CONFIG_PROC_CHILDREN` or the
/// process is gone.
fn children(proc_root: &Path, pid: u32) -> Vec<u32> {
    let task = proc_root.join(pid.to_string()).join("task");
    let Ok(threads) = fs::read_dir(&task) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for thread in threads.flatten() {
        let Ok(list) = fs::read_to_string(thread.path().join("children")) else {
            continue;
        };
        out.extend(
            list.split_whitespace()
                .filter_map(|token| token.parse::<u32>().ok()),
        );
    }
    out
}

fn process_name(proc_root: &Path, pid: u32) -> Option<String> {
    let comm = fs::read_to_string(proc_root.join(pid.to_string()).join("comm")).ok()?;
    let name = comm.trim().trim_start_matches('-');
    (!name.is_empty()).then(|| name.to_string())
}

fn is_shell(proc_root: &Path, pid: u32) -> bool {
    process_name(proc_root, pid).is_some_and(|name| SHELLS.contains(&name.as_str()))
}

/// Field 22 of `/proc/<pid>/stat`: process start time in clock ticks, which orders shells opened
/// in the same terminal. `0` when it cannot be read, leaving the pid to break the tie.
fn start_time(proc_root: &Path, pid: u32) -> u64 {
    let Ok(stat) = fs::read_to_string(proc_root.join(pid.to_string()).join("stat")) else {
        return 0;
    };
    // `<pid> (comm) <state> ...`: the command name may hold spaces and parentheses, so the numeric
    // fields are taken after the last ')'. Everything from `<state>` on is field 3 upwards, so the
    // start time (field 22) is the 20th of what is left.
    let Some(end) = stat.rfind(')') else {
        return 0;
    };
    stat[end + 1..]
        .split_whitespace()
        .nth(19)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

/// The socket Hyprland listens on for this session, when the environment names one that exists.
fn socket_path() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .ok()
        .filter(|value| !value.trim().is_empty())?;
    let runtime = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())?;
    let socket = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    socket.exists().then_some(socket)
}

/// `j/activewindow` over the IPC socket. Cheaper than `hyprctl` because nothing is spawned, which
/// is the whole reason the capture can run while the palette is being shown.
fn query_socket(socket: &Path) -> Option<String> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream.set_read_timeout(Some(FOCUS_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(FOCUS_TIMEOUT)).ok()?;
    stream.write_all(b"j/activewindow").ok()?;
    stream.flush().ok()?;
    stream.shutdown(std::net::Shutdown::Write).ok()?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => bytes.extend_from_slice(&chunk[..count]),
            // A read timeout with bytes in hand is a usable answer; without any it is not.
            Err(_) => break,
        }
    }
    (!bytes.is_empty()).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// The same question through `hyprctl`, used when the session does not expose the socket. The
/// child is waited on with a deadline rather than with `output()`, so a hung compositor costs
/// [`FOCUS_TIMEOUT`] and not a blocked finder.
fn query_hyprctl() -> Option<String> {
    let mut child = Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + FOCUS_TIMEOUT;
    loop {
        if child.try_wait().ok()?.is_some() {
            let output = child.wait_with_output().ok()?;
            return (output.status.success() && !output.stdout.is_empty())
                .then(|| String::from_utf8_lossy(&output.stdout).into_owned());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Reads `pid` and `class` out of Hyprland's `activewindow` JSON. Anything that is not a window
/// with a usable pid - malformed JSON, a truncated reply, pid 0 - is `None`.
pub fn parse_active_window(reply: &str) -> Option<FocusedWindow> {
    let value: serde_json::Value = serde_json::from_str(reply.trim()).ok()?;
    let pid = value.get("pid")?.as_u64()?;
    if pid == 0 {
        return None;
    }
    let class = value
        .get("class")
        .and_then(|class| class.as_str())
        .unwrap_or_default()
        .to_string();
    Some(FocusedWindow {
        pid: pid as u32,
        class,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A focus source that always answers with the same pid, so the walk can be tested alone.
    struct FixedFocus {
        window: Option<FocusedWindow>,
    }

    impl FixedFocus {
        fn pid(pid: u32) -> FixedFocus {
            FixedFocus {
                window: Some(FocusedWindow {
                    pid,
                    class: "ghostty".into(),
                }),
            }
        }
    }

    impl FocusReader for FixedFocus {
        fn focused(&self) -> Option<FocusedWindow> {
            self.window.clone()
        }
    }

    /// A unique scratch directory name under $TMPDIR; callers create and remove it themselves.
    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dev-console-context-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    struct Tree {
        root: PathBuf,
    }

    impl Tree {
        /// A scratch `/proc`-shaped directory.
        fn new(tag: &str) -> Tree {
            let root = scratch(&format!("proc-{tag}"));
            fs::create_dir_all(&root).expect("proc root");
            Tree { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        /// Creates `<pid>/task/<pid>/{comm,children}` plus the process's own `comm`, `stat` and
        /// `cwd`, which is everything the walker reads.
        fn process(&self, pid: u32, comm: &str, children: &[u32], cwd: Option<&Path>) -> &Tree {
            let dir = self.root.join(pid.to_string());
            fs::create_dir_all(dir.join("task").join(pid.to_string())).expect("task dir");
            let task = dir.join("task").join(pid.to_string());
            fs::write(task.join("comm"), format!("{comm}\n")).expect("comm");
            fs::write(dir.join("comm"), format!("{comm}\n")).expect("comm");
            fs::write(
                task.join("children"),
                children
                    .iter()
                    .map(|child| child.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
                    + "\n",
            )
            .expect("children");
            fs::write(dir.join("stat"), format!("{pid} ({comm}) S 1 0 0 0 0\n")).expect("stat");
            if let Some(cwd) = cwd {
                symlink(cwd, dir.join("cwd")).expect("cwd link");
            }
            self
        }

        /// Same as [`Tree::process`] but with a chosen start time (field 22 of `stat`), for the
        /// "one terminal, two tabs" case where the later shell must win.
        fn process_started_at(
            &self,
            pid: u32,
            comm: &str,
            start_time: u64,
            cwd: Option<&Path>,
        ) -> &Tree {
            self.process(pid, comm, &[], cwd);
            // Fields 1-3 are pid, comm and state; 4..=21 are placeholders; 22 is the start time.
            let middle = (4..22)
                .map(|field| field.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            fs::write(
                self.root.join(pid.to_string()).join("stat"),
                format!("{pid} ({comm}) S {middle} {start_time} 0\n"),
            )
            .expect("stat");
            self
        }

        fn erase(&self) {
            fs::remove_dir_all(&self.root).ok();
        }
    }

    fn repo(directory: &Path) {
        fs::create_dir_all(directory.join(".git")).expect("git dir");
    }

    #[test]
    fn finds_the_shell_a_terminal_started() {
        let tree = Tree::new("shell");
        let project = scratch("project");
        repo(&project);
        tree.process(100, "ghostty", &[101], None);
        tree.process(101, "zsh", &[], Some(&project));
        assert_eq!(
            shell_cwd(tree.path(), 100).as_deref(),
            Some(project.as_path())
        );
        tree.erase();
        fs::remove_dir_all(&project).ok();
    }

    #[test]
    fn a_shell_started_two_forks_down_still_counts() {
        let tree = Tree::new("deep");
        let project = scratch("deep");
        repo(&project);
        tree.process(200, "kitty", &[201], None);
        // A wrapper the terminal execs (a login wrapper, an env launcher) is not itself a shell,
        // and the shell's own running child must never be mistaken for the shell.
        tree.process(201, "sh", &[202], None);
        tree.process(202, "bash", &[203], Some(&project));
        tree.process(203, "cargo", &[], None);
        assert_eq!(
            shell_cwd(tree.path(), 200).as_deref(),
            Some(project.as_path())
        );
        tree.erase();
        fs::remove_dir_all(&project).ok();
    }

    #[test]
    fn the_youngest_shell_wins_when_a_terminal_has_two_tabs() {
        let tree = Tree::new("tabs");
        let older = scratch("older");
        let younger = scratch("younger");
        repo(&older);
        repo(&younger);
        tree.process(300, "foot", &[301, 302], None);
        tree.process_started_at(301, "zsh", 500, Some(&older));
        tree.process_started_at(302, "zsh", 900, Some(&younger));
        assert_eq!(
            shell_cwd(tree.path(), 300).as_deref(),
            Some(younger.as_path()),
            "the tab opened last is the one the user was just looking at"
        );
        tree.erase();
        fs::remove_dir_all(&older).ok();
        fs::remove_dir_all(&younger).ok();
    }

    #[test]
    fn shells_below_the_depth_limit_are_not_found() {
        let tree = Tree::new("limit");
        let project = scratch("limit");
        repo(&project);
        tree.process(400, "alacritty", &[401], None);
        tree.process(401, "sh", &[402], None);
        tree.process(402, "sh", &[403], None);
        tree.process(403, "sh", &[404], None);
        // Depth 4: past MAX_DEPTH, and what a shell launched is not the terminal's own shell.
        tree.process(404, "zsh", &[], Some(&project));
        assert_eq!(shell_cwd(tree.path(), 400), None);
        tree.erase();
        fs::remove_dir_all(&project).ok();
    }

    #[test]
    fn processes_that_are_not_shells_are_skipped() {
        let tree = Tree::new("not-shell");
        let project = scratch("skip");
        repo(&project);
        tree.process(500, "ghostty", &[501, 502], None);
        tree.process(501, "vim", &[], Some(&project));
        tree.process(502, "tmux", &[], Some(&project));
        assert_eq!(
            shell_cwd(tree.path(), 500),
            None,
            "no known shell, so no context"
        );
        tree.erase();
        fs::remove_dir_all(&project).ok();
    }

    #[test]
    fn a_login_shell_is_still_recognised() {
        let tree = Tree::new("login");
        let project = scratch("login");
        repo(&project);
        tree.process(600, "kitty", &[601], None);
        tree.process(601, "-zsh", &[], Some(&project));
        assert_eq!(
            shell_cwd(tree.path(), 600).as_deref(),
            Some(project.as_path())
        );
        tree.erase();
        fs::remove_dir_all(&project).ok();
    }

    #[test]
    fn the_nearest_repository_above_a_directory_is_the_project() {
        let base = scratch("gitroot");
        let nested = base.join("app").join("src");
        fs::create_dir_all(&nested).expect("nested");
        fs::create_dir_all(base.join("app")).expect("app");
        repo(&base);
        let home = scratch("home");
        fs::create_dir_all(&home).expect("home");
        assert_eq!(
            project_root(&nested, Some(&home)).as_deref(),
            Some(base.as_path())
        );
        fs::remove_dir_all(&base).ok();
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_project_file_makes_a_directory_that_is_not_a_repository_count() {
        let base = scratch("manifest");
        fs::create_dir_all(&base).expect("base");
        fs::write(base.join("package.json"), b"{}").expect("manifest");
        assert_eq!(project_root(&base, None).as_deref(), Some(base.as_path()));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_plain_directory_is_no_project_at_all() {
        let base = scratch("plain");
        fs::create_dir_all(&base).expect("base");
        assert_eq!(project_root(&base, None), None);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn the_walk_up_stops_at_home_and_home_itself_is_never_the_project() {
        let home = scratch("stop");
        let nested = home.join("notes");
        fs::create_dir_all(&nested).expect("nested");
        repo(&home);
        fs::write(home.join("package.json"), b"{}").expect("manifest");
        assert_eq!(
            project_root(&nested, Some(&home)),
            None,
            "a stray .git or manifest in $HOME must not label every terminal in the session"
        );
        assert_eq!(project_root(&home, Some(&home)), None);
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn parses_the_activewindow_reply_of_a_real_session() {
        // Truncated to the keys used, as `hyprctl activewindow -j` returns it.
        let window = parse_active_window(
            r#"{"address":"0x1","class":"kitty","title":"zsh","pid":4242,"workspace":{"id":2,"name":"2:term"},"initialClass":"kitty","focused":true}"#,
        )
        .expect("window");
        assert_eq!(window.pid, 4242);
        assert_eq!(window.class, "kitty");
    }

    #[test]
    fn an_unusable_activewindow_answer_is_no_window() {
        assert_eq!(parse_active_window("not json"), None);
        assert_eq!(parse_active_window(r#"{"class":"kitty"}"#), None);
        assert_eq!(parse_active_window(r#"{"pid":0,"class":"kitty"}"#), None);
        assert_eq!(
            parse_active_window(r#"{"pid":"4242","class":"kitty"}"#),
            None
        );
        // A window with no class is still a window; only the pid is required.
        assert_eq!(
            parse_active_window(r#"{"pid":7}"#).map(|window| window.pid),
            Some(7)
        );
    }

    #[test]
    fn detection_runs_the_whole_chain_and_stops_where_a_link_is_missing() {
        let tree = Tree::new("chain");
        let project = scratch("chain-project");
        let home = scratch("chain-home");
        repo(&project);
        fs::create_dir_all(&home).expect("home");
        tree.process(700, "footclient", &[701], None);
        tree.process(701, "fish", &[], Some(&project));

        let context =
            detect_with(&FixedFocus::pid(700), tree.path(), Some(&home)).expect("context");
        assert_eq!(context.root, project);
        assert_eq!(
            context.name,
            project.file_name().expect("name").to_string_lossy()
        );

        struct NoWindow;
        impl FocusReader for NoWindow {
            fn focused(&self) -> Option<FocusedWindow> {
                None
            }
        }
        assert_eq!(
            detect_with(&NoWindow, tree.path(), Some(&home)),
            None,
            "no compositor answer, so ranking stays exactly as it was"
        );
        assert_eq!(
            detect_with(&FixedFocus::pid(999), tree.path(), Some(&home)),
            None,
            "a window with no shell under it, such as one owned by tmux"
        );

        struct OwnWindow;
        impl FocusReader for OwnWindow {
            fn focused(&self) -> Option<FocusedWindow> {
                Some(FocusedWindow {
                    pid: 700,
                    class: "dev-console".into(),
                })
            }
        }
        assert_eq!(
            detect_with(&OwnWindow, tree.path(), Some(&home)),
            None,
            "the palette's own window says nothing about a project"
        );

        tree.erase();
        fs::remove_dir_all(&project).ok();
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_context_needs_a_directory_that_can_be_named() {
        assert!(FinderContext::from_root(Path::new("/")).is_none());
        let context =
            FinderContext::from_root(Path::new("/home/dev/projects/dev-console")).expect("context");
        assert_eq!(context.name, "dev-console");
    }

    /// The 30 ms the plan budgets for detection is mostly the compositor's answer, so the number
    /// only means something on a machine that has one: `cargo test -- --ignored` there and note
    /// the printed timing. Off a desktop the socket is simply absent, which measures nothing.
    #[test]
    #[ignore = "only meaningful on a machine with a Hyprland session running"]
    fn detection_fits_inside_its_budget() {
        let started = Instant::now();
        let detected = detect();
        let elapsed = started.elapsed();
        println!(
            "detect() -> {} in {} us",
            match &detected {
                Some(context) => context.root.to_string_lossy().into_owned(),
                None => "no context".into(),
            },
            elapsed.as_micros()
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "detection is bounded by the socket read timeout, whatever the session looks like"
        );
    }
}
