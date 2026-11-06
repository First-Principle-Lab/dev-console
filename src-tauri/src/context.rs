//! Project context: which project the focused terminal is in when the Finder opens.
//!
//! The whole pipeline is fail-safe by design. Every step either returns what it found or
//! `None`, and `None` simply means "rank like before". Nothing is ever persisted, and the only
//! sources touched are the compositor's own IPC socket and `/proc` (both local, both the
//! user's own processes).
//!
//! Shape of the detection chain:
//!
//! 1. Under Hyprland (`$HYPRLAND_INSTANCE_SIGNATURE` set) ask the compositor socket
//!    (`$XDG_RUNTIME_DIR/hypr/<sig>/.socket.sock`) for the currently active window's pid with
//!    a `j/activewindow` request. No child process is spawned; this is a plain `UnixStream`.
//!    On every other desktop the context is `None` (v1 deliberately does not talk to X11).
//! 2. Walk the window process's children breadth-first through
//!    `/proc/<pid>/task/<tid>/children` (at most [`MAX_SHELL_DEPTH`] levels) looking for a
//!    known shell via `/proc/<pid>/comm`. Terminals run several shells once tabs and splits
//!    exist, so the youngest (highest pid) wins. The shell's own `/proc/<pid>/cwd` is the
//!    directory the user is looking at.
//! 3. The directory becomes a project: the nearest ancestor containing `.git` (never above
//!    `$HOME`), or the directory itself when it directly holds a project file
//!    ([`PROJECT_FILES`]). Otherwise there is no project to promote.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The project the Finder should promote for one showing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinderContext {
    pub root: PathBuf,
    pub name: String,
}

impl FinderContext {
    fn for_root(root: PathBuf) -> FinderContext {
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string_lossy().into_owned());
        FinderContext { root, name }
    }
}

/// Shells recognised as "the thing the user is typing into", matched against `/proc/<pid>/comm`.
const SHELL_NAMES: [&str; 4] = ["zsh", "bash", "fish", "nu"];

/// Files that mark a plain directory as a project root when no `.git` was found.
const PROJECT_FILES: [&str; 5] = [
    "package.json",
    "Makefile",
    "justfile",
    "compose.yaml",
    "Cargo.toml",
];

/// How far below the window process the shell hunt descends (tabs nest only a level deeper).
const MAX_SHELL_DEPTH: usize = 3;

/// The compositor is local; if it does not answer this fast something is wrong and we give up.
const IPC_TIMEOUT: Duration = Duration::from_millis(200);

/// Detects the focused project, or `None` anywhere the chain breaks (no Hyprland, no shell
/// under the focused window, no recognisable project). Bounded by [`IPC_TIMEOUT`]: the only
/// step that can wait is the compositor round-trip, everything else is local file I/O.
pub fn detect() -> Option<FinderContext> {
    let socket = compositor_socket()?;
    let window_pid = active_window_pid(&socket)?;
    let shell_pid = find_shell(Path::new("/proc"), window_pid)?;
    let cwd = process_cwd(Path::new("/proc"), shell_pid)?;
    project_at(&cwd, home_dir().as_deref())
}

/// The Hyprland IPC socket, when this session runs under Hyprland.
fn compositor_socket() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
    if signature.trim().is_empty() || runtime.trim().is_empty() {
        return None;
    }
    let socket = Path::new(&runtime)
        .join("hypr")
        .join(signature.trim())
        .join(".socket.sock");
    // Do not follow symlinks and do not distinguish the type: if the entry is there the
    // connect attempt below either works or fails fast.
    fs::symlink_metadata(&socket).is_ok().then_some(socket)
}

/// Asks the compositor for the focused window's pid over its plain-text IPC socket.
/// `j/activewindow` answers with pretty-printed JSON.
fn active_window_pid(socket: &Path) -> Option<u32> {
    let mut stream = std::os::unix::net::UnixStream::connect(socket).ok()?;
    stream.set_write_timeout(Some(IPC_TIMEOUT)).ok()?;
    stream.set_read_timeout(Some(IPC_TIMEOUT)).ok()?;
    stream.write_all(b"j/activewindow").ok()?;
    stream.flush().ok()?;
    let mut response = String::new();
    // Hyprland closes the connection after the reply, so EOF is the end marker; the read
    // timeout above is the backstop if the compositor stalls.
    stream.read_to_string(&mut response).ok()?;
    parse_active_window(&response)
}

fn parse_active_window(json: &str) -> Option<u32> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    Some(value.get("pid")?.as_u64()? as u32)
}

/// Breadth-first descent from the window process, at most [`MAX_SHELL_DEPTH`] levels deep,
/// looking for a known shell. Tabs and splits make several shells children of the same
/// terminal; the youngest (highest pid, modulo wraparound) is the one being typed into.
fn find_shell(proc_root: &Path, window_pid: u32) -> Option<u32> {
    let mut level = vec![window_pid];
    for _ in 0..MAX_SHELL_DEPTH {
        let mut shells = Vec::new();
        let mut next = Vec::new();
        for pid in level {
            for child in children_of(proc_root, pid) {
                if is_shell(proc_root, child) {
                    shells.push(child);
                } else {
                    next.push(child);
                }
            }
        }
        if let Some(youngest) = shells.into_iter().max() {
            return Some(youngest);
        }
        level = next;
    }
    None
}

/// Pids listed in every `/proc/<pid>/task/<tid>/children` file of a process.
fn children_of(proc_root: &Path, pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let Ok(tasks) = fs::read_dir(proc_root.join(pid.to_string()).join("task")) else {
        return out;
    };
    for task in tasks.flatten() {
        let Ok(children) = fs::read_to_string(task.path().join("children")) else {
            continue;
        };
        out.extend(
            children
                .split_whitespace()
                .filter_map(|c| c.parse::<u32>().ok()),
        );
    }
    out
}

fn is_shell(proc_root: &Path, pid: u32) -> bool {
    let Ok(comm) = fs::read_to_string(proc_root.join(pid.to_string()).join("comm")) else {
        return false;
    };
    // `/proc/<pid>/comm` holds the (15-byte truncated) process name, e.g. "zsh".
    SHELL_NAMES.contains(&comm.trim())
}

fn process_cwd(proc_root: &Path, pid: u32) -> Option<PathBuf> {
    fs::read_link(proc_root.join(pid.to_string()).join("cwd")).ok()
}

/// Turns a directory into the project it belongs to: the nearest ancestor holding `.git`
/// (never above `home`), else the directory itself if it directly holds a project file.
/// A missing `home` boundary only means the walk-up stops at the filesystem root; it never
/// runs wild because every step is a strict parent of the last.
fn project_at(directory: &Path, home: Option<&Path>) -> Option<FinderContext> {
    let mut current = Some(directory.to_path_buf());
    while let Some(path) = current {
        if path.join(".git").exists() {
            return Some(FinderContext::for_root(path));
        }
        if home == Some(path.as_path()) {
            // The walk-up stops at $HOME: a repository marker at or above the home folder
            // (bare dotfiles) would otherwise promote every directory the user ever `cd`s to.
            break;
        }
        current = path.parent().map(|p| p.to_path_buf());
    }
    if PROJECT_FILES
        .iter()
        .any(|marker| directory.join(marker).exists())
    {
        return Some(FinderContext::for_root(directory.to_path_buf()));
    }
    None
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| !home.as_os_str().is_empty())
}

/// Expands a stored `working_directory` the way `execution::working_directory` does when the
/// command runs, so membership compares like-for-like. Existence is deliberately not checked:
/// a command may point at a directory the user has since moved.
pub fn expand_working_directory(raw: &str, home: Option<&Path>) -> PathBuf {
    let raw = raw.trim();
    if raw == "~" {
        return home.map(PathBuf::from).unwrap_or_default();
    }
    if let Some((_, rest)) = raw.split_once("~/") {
        if let Some(home) = home {
            return home.join(rest);
        }
    }
    PathBuf::from(raw)
}

/// True when `path` is `root` itself or lives underneath it. Component-wise, so
/// `/home/dev/repos/api` never counts as inside `/home/dev/repos/apitest`.
pub fn is_under(path: &Path, root: &Path) -> bool {
    // strip_prefix compares whole path components and ignores trailing separators, which is
    // exactly the containment question here (`Path::starts_with` is string-prefix based).
    path.strip_prefix(root).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    /// A throwaway directory tree, removed when the test ends.
    struct Fixture(PathBuf);

    impl Fixture {
        fn new(tag: &str) -> Fixture {
            let base = std::env::temp_dir().join(format!(
                "dev-console-context-{tag}-{}-{}",
                std::process::id(),
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&base).expect("fixture base");
            Fixture(base)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    /// Writes one process into a fake `/proc`: comm, one task, children, and a cwd link.
    fn fake_process(proc_root: &Path, pid: u32, comm: &str, children: &[u32], cwd: Option<&Path>) {
        let dir = proc_root.join(pid.to_string());
        fs::create_dir_all(dir.join("task").join(pid.to_string())).expect("fake pid dir");
        fs::write(dir.join("comm"), format!("{comm}\n")).expect("fake comm");
        fs::write(
            dir.join("task").join(pid.to_string()).join("children"),
            children
                .iter()
                .map(|child| child.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        )
        .expect("fake children");
        if let Some(cwd) = cwd {
            symlink(cwd, dir.join("cwd")).expect("fake cwd");
        }
    }

    #[test]
    fn finds_a_shell_directly_under_the_window_process() {
        let fixture = Fixture::new("tree");
        let proc = fixture.path().join("proc");
        fake_process(&proc, 100, "ghostty", &[200], None);
        fake_process(&proc, 200, "zsh", &[], None);
        assert_eq!(find_shell(&proc, 100), Some(200));
    }

    #[test]
    fn prefers_the_youngest_shell_when_a_terminal_has_several() {
        let fixture = Fixture::new("tabs");
        let proc = fixture.path().join("proc");
        fake_process(&proc, 100, "kitty", &[200, 201, 202], None);
        fake_process(&proc, 200, "zsh", &[], None);
        fake_process(&proc, 202, "bash", &[], None);
        fake_process(&proc, 201, "zsh", &[], None);
        assert_eq!(find_shell(&proc, 100), Some(202));
    }

    #[test]
    fn descends_through_helper_children_to_reach_the_shell() {
        // window → helper process → zsh is only two levels below.
        let fixture = Fixture::new("nested");
        let proc = fixture.path().join("proc");
        fake_process(&proc, 10, "kitty", &[20], None);
        fake_process(&proc, 20, "kitty-shim", &[30], None);
        fake_process(&proc, 30, "fish", &[], None);
        assert_eq!(find_shell(&proc, 10), Some(30));
    }

    #[test]
    fn stops_descending_after_three_levels() {
        let fixture = Fixture::new("depth");
        let proc = fixture.path().join("proc");
        fake_process(&proc, 1, "window", &[2], None);
        fake_process(&proc, 2, "a", &[3], None);
        fake_process(&proc, 3, "b", &[4], None);
        fake_process(&proc, 4, "c", &[5], None);
        fake_process(&proc, 5, "zsh", &[], None);
        assert_eq!(find_shell(&proc, 1), None);
    }

    #[test]
    fn unknown_processes_are_not_shells() {
        let fixture = Fixture::new("notshell");
        let proc = fixture.path().join("proc");
        fake_process(&proc, 100, "ghostty", &[200], None);
        fake_process(&proc, 200, "vim", &[], None);
        assert_eq!(find_shell(&proc, 100), None);
    }

    #[test]
    fn an_unreadable_proc_tree_yields_nothing() {
        let fixture = Fixture::new("missing");
        assert_eq!(find_shell(&fixture.path().join("proc"), 1), None);
        assert_eq!(process_cwd(&fixture.path().join("proc"), 1), None);
    }

    #[test]
    fn reads_the_cwd_link_of_a_process() {
        let fixture = Fixture::new("cwd");
        let proc = fixture.path().join("proc");
        let workdir = fixture.path().join("repo");
        fs::create_dir_all(&workdir).expect("workdir");
        fake_process(&proc, 7, "zsh", &[], Some(&workdir));
        assert_eq!(process_cwd(&proc, 7), Some(workdir));
    }

    #[test]
    fn parses_the_active_window_reply() {
        assert_eq!(
            parse_active_window(r#"{ "pid": 4242, "class": "kitty" }"#),
            Some(4242)
        );
        assert_eq!(parse_active_window(r#"{ "class": "firefox" }"#), None);
        assert_eq!(parse_active_window("not json"), None);
        assert_eq!(parse_active_window(r#"{ "pid": "42" }"#), None);
    }

    #[test]
    fn finds_the_nearest_git_root_ancestor() {
        let fixture = Fixture::new("gitroot");
        let home = fixture.path().join("home");
        let repository = home.join("projects").join("dev-console");
        let deep = repository.join("src-tauri").join("src");
        fs::create_dir_all(&deep).expect("deep");
        fs::create_dir_all(repository.join(".git")).expect("git dir");
        let context = project_at(&deep, Some(&home)).expect("context");
        assert_eq!(context.root, repository);
        assert_eq!(context.name, "dev-console");
    }

    #[test]
    fn a_worktree_dot_git_file_counts_as_a_repository() {
        let fixture = Fixture::new("dotgitfile");
        let repository = fixture.path().join("repo");
        fs::create_dir_all(repository.join("sub")).expect("repo");
        fs::write(repository.join(".git"), "gitdir: /elsewhere").expect("worktree pointer");
        let context = project_at(&repository.join("sub"), Some(fixture.path())).expect("context");
        assert_eq!(context.root, repository);
    }

    #[test]
    fn the_git_walk_up_stops_at_home() {
        let fixture = Fixture::new("homestop");
        let home = fixture.path().join("home");
        let projects = home.join("projects");
        fs::create_dir_all(projects.join("plain")).expect("projects");
        fs::create_dir_all(home.join(".git")).expect("home git");
        // A marker exactly at $HOME (bare dotfiles repo) is still inside the allowed walk-up.
        let context = project_at(&projects.join("plain"), Some(&home)).expect("context");
        assert_eq!(context.root, home);
        fs::remove_dir_all(home.join(".git")).expect("clean home");
        // With no marker inside the boundary there is no project, whatever sits above $HOME.
        assert!(
            project_at(&projects.join("plain"), Some(&home)).is_none(),
            "the walk-up must not climb above $HOME looking for markers"
        );
    }

    #[test]
    fn a_plain_directory_qualifies_only_through_a_project_file() {
        let fixture = Fixture::new("markers");
        let home = fixture.path().join("home");
        let project = home.join("scratch");
        fs::create_dir_all(&project).expect("project");
        assert!(
            project_at(&project, Some(&home)).is_none(),
            "a bare folder is not a project"
        );
        for marker in [
            "package.json",
            "Makefile",
            "justfile",
            "compose.yaml",
            "Cargo.toml",
        ] {
            fs::write(project.join(marker), "").expect("marker");
            let context = project_at(&project, Some(&home)).expect("project file");
            assert_eq!(context.root, project);
            fs::remove_file(project.join(marker)).expect("remove marker");
        }
        // Only the directory itself counts for the marker fallback, never an ancestor.
        let nested = project.join("nested");
        fs::create_dir_all(&nested).expect("nested");
        fs::write(project.join("Makefile"), "").expect("marker");
        assert!(project_at(&nested, Some(&home)).is_none());
    }

    #[test]
    fn project_names_come_from_the_directory_name() {
        let fixture = Fixture::new("names");
        let repository = fixture.path().join("repos").join("dev-console");
        fs::create_dir_all(repository.join(".git")).expect("git");
        let context = project_at(&repository, Some(fixture.path())).expect("context");
        assert_eq!(context.name, "dev-console");
    }

    #[test]
    fn expands_tildes_exactly_like_execution_does() {
        let home = Path::new("/home/dev");
        assert_eq!(
            expand_working_directory("~", Some(home)),
            PathBuf::from("/home/dev")
        );
        assert_eq!(
            expand_working_directory("~/repos/api", Some(home)),
            PathBuf::from("/home/dev/repos/api")
        );
        assert_eq!(
            expand_working_directory("/srv/app", Some(home)),
            PathBuf::from("/srv/app")
        );
        assert_eq!(expand_working_directory("", Some(home)), PathBuf::new());
        // Without a home to expand into, even tilde paths stay literal.
        assert_eq!(expand_working_directory("~/x", None), PathBuf::from("~/x"));
    }

    #[test]
    fn containment_is_per_path_component() {
        assert!(is_under(
            Path::new("/home/dev/repos/api"),
            Path::new("/home/dev/repos/api")
        ));
        assert!(is_under(
            Path::new("/home/dev/repos/api/src"),
            Path::new("/home/dev/repos/api")
        ));
        assert!(!is_under(
            Path::new("/home/dev/repos/apitest"),
            Path::new("/home/dev/repos/api")
        ));
        assert!(!is_under(
            Path::new("/home/dev"),
            Path::new("/home/dev/repos")
        ));
        // Trailing slashes on either side do not change the answer.
        assert!(is_under(
            Path::new("/home/dev/repos/api/"),
            Path::new("/home/dev/repos/api")
        ));
        assert!(is_under(
            Path::new("/home/dev/repos/api/src"),
            Path::new("/home/dev/repos/api//")
        ));
    }

    #[test]
    fn a_session_without_a_compositor_has_no_context() {
        // The detection entry point is environment-driven; with no compositor variables set
        // the chain must end immediately at None, without ever reading /proc.
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            return; // genuinely under Hyprland; nothing to assert either way
        }
        assert!(detect().is_none());
    }

    /// The plan's timing budget, measured on the target machine. Run with
    /// `cargo test -- --ignored detection_under_the_timing_budget` (the
    /// `scripts/test-project-context.sh` harness does this under Hyprland).
    #[test]
    #[ignore = "times a real detection pass; only meaningful on the target desktop"]
    fn detection_under_the_timing_budget() {
        let started = std::time::Instant::now();
        let found = detect();
        let elapsed = started.elapsed();
        println!("project-context detection: {found:?} in {elapsed:?}");
        // A healthy local chain (IPC round-trip + /proc walk) belongs well under 30 ms.
        assert!(
            elapsed < std::time::Duration::from_millis(30),
            "detection took {elapsed:?}, over the 30 ms budget"
        );
    }

    /// Used by `scripts/test-project-context.sh`: the harness opens a terminal running a
    /// known shell inside a fresh git repository, and Hyprland leaves that window focused.
    /// Detection must resolve exactly that repository through the real compositor socket
    /// and the real `/proc`. `PROJECT_CONTEXT_EXPECT_ROOT` is set only by the harness;
    /// without it (e.g. a CI-wide `--ignored` run) the test skips itself rather than
    /// failing the suite.
    #[test]
    #[ignore = "requires the project-context harness to focus a known terminal first"]
    fn resolves_the_focused_repository() {
        let Ok(expected_root) = std::env::var("PROJECT_CONTEXT_EXPECT_ROOT") else {
            eprintln!(
                "skipping resolves_the_focused_repository: PROJECT_CONTEXT_EXPECT_ROOT is set\n  \
                 only by scripts/test-project-context.sh, under a live Hyprland session"
            );
            return;
        };
        let expected = fs::canonicalize(&expected_root).expect("the harness repository exists");
        let found = detect().expect("detection must resolve the focused terminal's project");
        assert_eq!(
            found.root, expected,
            "detection resolved the wrong project root"
        );
        assert_eq!(
            found.name,
            expected
                .file_name()
                .expect("repo has a name")
                .to_string_lossy()
        );
    }
}
