//! Single-instance lock and the control-socket wakeup path.
//!
//! A second `dev-console` process must never open a second set of windows or a second vault
//! handle. It forwards its launch intent to the resident process over a private Unix socket and
//! exits, and the resident process wakes and shows the requested window. This mirrors the design
//! already documented in `src-tauri/src/control.rs`: a private socket in the user's runtime
//! directory, one JSON request per connection, one JSON reply line, access control that is simply
//! the filesystem (a 0700 directory, a 0600 socket), no dependency on a bus daemon, and a hard
//! timeout. Nothing secret crosses it - only window intents.
//!
//! ## Lock
//!
//! Two mechanisms, because they fail differently:
//!
//! - `InstanceLock` is an `flock(LOCK_EX | LOCK_NB)` on a regular file. The kernel drops it when
//!   the holder dies, so a killed process cannot leave the app unstartable, which a pid file does.
//! - The socket name itself is the second lock: `AF_UNIX` bind is exclusive, so two live listeners
//!   cannot own the same path.
//!
//! ## Not the Tauri control socket
//!
//! This socket is `rune-instance.sock`, deliberately *not* `control.sock`. The Tauri app and this
//! binary run side by side for the whole migration (plan §4 keeps the old app building until the
//! new one passes acceptance), and both use the same runtime directory; sharing `control.sock`
//! would let a Rune launch drive the Tauri instance, or make either one believe the other was
//! itself. When plan milestone 2 moves `src-tauri/src/control.rs` into `dev-console-core`, this
//! module is replaced by that protocol - same socket path then, because only one product will own
//! it - and is not extended beyond window intents in the meantime.

use std::fs::{DirBuilder, File, OpenOptions, Permissions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Name of this shell's instance socket inside the runtime directory. See the module docs for why
/// it is not `control.sock`.
pub const SOCKET_NAME: &str = "rune-instance.sock";

/// Name of the `flock` file that makes the instance lock survive a socket being renamed away.
pub const LOCK_NAME: &str = "rune-instance.lock";

/// Environment override for the runtime directory, shared with `src-tauri/src/control.rs` so one
/// setting points both shells at the same directory in a test or a sandboxed session.
pub const ENV_RUNTIME_DIR: &str = "DEV_CONSOLE_RUNTIME_DIR";

/// The private directory a request may not exceed. Intents are tiny; the cap keeps a confused or
/// hostile local writer from growing the buffer.
const MAX_REQUEST_BYTES: u64 = 4096;

/// How long a forwarder waits for the resident process. The Tauri control channel allows 120s for
/// a vault operation; a window intent is answered from the event loop, so this is far shorter and
/// a slow answer means the resident process is not going to answer at all.
const FORWARD_TIMEOUT: Duration = Duration::from_secs(5);

/// What the second process asked the resident one to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Intent {
    /// Show and focus the Library window.
    Library,
    /// Show and focus the Finder: the behaviour the compositor's global shortcut drives with
    /// `dev-console --finder`.
    Finder,
}

impl Intent {
    /// Every window this shell is allowed to be asked to show. Kept as a list so a capability that
    /// is not a window name has nowhere to be added by accident.
    pub const ALL: [Intent; 2] = [Intent::Library, Intent::Finder];

    /// The intent a `--flag` names, if any.
    pub fn from_flag(flag: &str) -> Option<Intent> {
        match flag {
            "--finder" | "finder" => Some(Intent::Finder),
            "--library" | "library" => Some(Intent::Library),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    intent: Intent,
}

#[derive(Debug, Serialize, Deserialize)]
struct Reply {
    ok: bool,
}

/// The private directory for this user's session, by the same rule as `src-tauri/src/control.rs`:
/// an explicit override, then `$XDG_RUNTIME_DIR/dev-console`, then a per-uid directory under the
/// temp directory for sessions with no runtime dir (a bare `ssh -t`, a container).
pub fn runtime_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(ENV_RUNTIME_DIR) {
        if !dir.to_string_lossy().trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        if !dir.to_string_lossy().trim().is_empty() {
            return PathBuf::from(dir).join("dev-console");
        }
    }
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir().join(format!("dev-console-{uid}"))
}

/// Path of this shell's instance socket.
pub fn socket_path() -> PathBuf {
    runtime_dir().join(SOCKET_NAME)
}

/// Path of the `flock` file backing the instance lock.
pub fn lock_path() -> PathBuf {
    runtime_dir().join(LOCK_NAME)
}

/// Creates the runtime directory if needed, and forces 0700 on it even if it already existed with
/// looser permissions: the socket inside is the app's only access control.
fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    if !dir.as_os_str().is_empty() && !dir.exists() {
        DirBuilder::new().mode(0o700).recursive(true).create(dir)?;
    }
    std::fs::set_permissions(dir, Permissions::from_mode(0o700))
}

/// The held exclusive instance lock. Dropping it (including on a crash) releases it.
#[derive(Debug)]
pub struct InstanceLock {
    /// Never read: the `File` is what the `flock` is attached to, so holding it is the whole
    /// purpose, and dropping it closes the descriptor and releases the lock.
    #[allow(
        dead_code,
        reason = "kept for its Drop: closing the descriptor releases the flock"
    )]
    file: File,
}

impl InstanceLock {
    /// Takes `flock(LOCK_EX | LOCK_NB)` on `path`. `Ok(None)` means another live process holds it.
    pub fn acquire(path: &Path) -> io::Result<Option<InstanceLock>> {
        if let Some(parent) = path.parent() {
            ensure_private_dir(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)?;
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc == 0 {
            return Ok(Some(InstanceLock { file }));
        }
        let error = io::Error::last_os_error();
        // EWOULDBLOCK means contention, which is the normal second-launch outcome. Anything else
        // (an EPERM on an odd filesystem, say) is a real failure and must be reported as one.
        if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
            return Ok(None);
        }
        Err(error)
    }

    /// Convenience over the default path.
    pub fn acquire_default() -> io::Result<Option<InstanceLock>> {
        Self::acquire(&lock_path())
    }
}

/// The resident process's listener. Dropping it removes the socket file, so a later launch is not
/// blocked by a name nothing listens on.
#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}

impl Server {
    /// Binds `path`, replacing a socket file left behind by a killed instance.
    pub fn bind(path: &Path) -> io::Result<Server> {
        if let Some(parent) = path.parent() {
            ensure_private_dir(parent)?;
        }
        match UnixListener::bind(path) {
            Ok(listener) => Self::finish(listener, path),
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                if instance_running(path) {
                    return Err(error);
                }
                // A socket file whose listener is gone is inert: nothing is queued on it and the
                // same rule already applies in the Tauri control server.
                std::fs::remove_file(path)?;
                Self::finish(UnixListener::bind(path)?, path)
            }
            Err(error) => Err(error),
        }
    }

    fn finish(listener: UnixListener, path: &Path) -> io::Result<Server> {
        // The directory is already 0700; the socket mode is belt and braces for a shared runtime dir.
        std::fs::set_permissions(path, Permissions::from_mode(0o600))?;
        Ok(Server {
            listener,
            path: path.to_path_buf(),
        })
    }

    /// Where this server listens.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Blocks for the next intent. A malformed, unknown-shaped or over-long request is skipped and
    /// the next connection is read, which keeps one confused caller from ending the wakeup path.
    pub fn accept(&self) -> io::Result<Intent> {
        loop {
            let (stream, _) = self.listener.accept()?;
            match read_intent(stream) {
                Ok(intent) => return Ok(intent),
                Err(error) if error.kind() == io::ErrorKind::InvalidData => continue,
                Err(error) => return Err(error),
            }
        }
    }

    /// Non-blocking [`Server::accept`]: `Ok(None)` means nothing arrived. For tests and for a host
    /// that would rather poll than block a thread.
    pub fn try_accept(&self) -> io::Result<Option<Intent>> {
        self.listener.set_nonblocking(true)?;
        match self.listener.accept() {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
            Ok((stream, _)) => match read_intent(stream) {
                Ok(intent) => Ok(Some(intent)),
                Err(error) if error.kind() == io::ErrorKind::InvalidData => Ok(None),
                Err(error) => Err(error),
            },
        }
    }

    /// Blocks serving intents until the listener fails. See `app::run` for the thread this runs on.
    pub fn serve(&self, mut on_intent: impl FnMut(Intent)) -> io::Result<()> {
        loop {
            on_intent(self.accept()?);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path).ok();
    }
}

fn read_intent(mut stream: UnixStream) -> io::Result<Intent> {
    stream.set_read_timeout(Some(FORWARD_TIMEOUT))?;
    stream.set_write_timeout(Some(FORWARD_TIMEOUT))?;

    let mut line = String::new();
    // `take` bounds the buffer: a caller that streams without a newline cannot grow this process.
    let read = BufReader::new((&mut stream).take(MAX_REQUEST_BYTES)).read_line(&mut line)?;
    if read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the caller closed the connection",
        ));
    }
    let request: Request = serde_json::from_str(line.trim())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let reply = Reply { ok: true };
    stream.write_all(serde_json::to_string(&reply)?.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    Ok(request.intent)
}

/// Forwards `intent` to a resident instance. An error means "no usable resident process", which is
/// how the first launch tells it is the first.
pub fn send_intent(path: &Path, intent: Intent) -> io::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(FORWARD_TIMEOUT))?;
    stream.set_write_timeout(Some(FORWARD_TIMEOUT))?;
    let request = Request { intent };
    stream.write_all(serde_json::to_string(&request)?.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut line = String::new();
    let read = BufReader::new((&mut stream).take(MAX_REQUEST_BYTES)).read_line(&mut line)?;
    if read == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the resident instance answered nothing",
        ));
    }
    let reply: Reply = serde_json::from_str(line.trim())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if reply.ok {
        Ok(())
    } else {
        Err(io::Error::other("the resident instance refused the intent"))
    }
}

/// True when something is listening on `path`.
///
/// This is a liveness probe, not a handshake: `AF_UNIX` connect fails with `ECONNREFUSED` on a
/// socket file no one is listening to, and succeeds against a live listener without waiting for it
/// to accept. That distinction is all [`Server::bind`] needs to tell "a live instance owns this
/// name" apart from "a killed instance left the file behind"; [`InstanceLock`] is what actually
/// makes the single-instance guarantee, because a listener with a full accept backlog would also
/// refuse a connect and is still alive.
pub fn instance_running(path: &Path) -> bool {
    UnixStream::connect(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &Path) -> (PathBuf, PathBuf) {
        (dir.join(SOCKET_NAME), dir.join(LOCK_NAME))
    }

    #[test]
    fn an_intent_round_trips_over_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, _) = paths(dir.path());
        let server = Server::bind(&socket).unwrap();
        assert_eq!(server.path(), socket.as_path());

        // `send_intent` is a full request/reply round trip: it returns only once the listener has
        // read the request and written the reply. So `accept` must be answering on its own thread,
        // exactly as `app::run` drives it through `Server::serve`; a sequential accept on this
        // thread would only be reached after `send_intent` had already timed out.
        let accepted = std::thread::spawn(move || server.accept());
        send_intent(&socket, Intent::Finder).unwrap();

        assert_eq!(accepted.join().unwrap().unwrap(), Intent::Finder);
    }

    #[test]
    fn an_unknown_intent_is_rejected_without_ending_the_wakeup_path() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, _) = paths(dir.path());
        let server = Server::bind(&socket).unwrap();

        // The listener skips the malformed request and answers the next one, so it runs on its own
        // thread while this thread is inside `send_intent` waiting for its reply.
        let accepted = std::thread::spawn(move || server.accept());

        let mut caller = UnixStream::connect(&socket).unwrap();
        caller
            .write_all(b"{\"intent\":\"reveal-secret\"}\n")
            .unwrap();
        caller.shutdown(std::net::Shutdown::Write).unwrap();

        // The bad request is skipped and the next well-formed one still arrives.
        send_intent(&socket, Intent::Library).unwrap();
        assert_eq!(accepted.join().unwrap().unwrap(), Intent::Library);
    }

    /// A caller that streams without ever sending a newline must not be able to grow this process:
    /// the reader is capped, the truncated line fails to parse, and the wakeup path keeps working.
    #[test]
    fn an_oversized_request_is_capped_and_the_next_intent_still_arrives() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, _) = paths(dir.path());
        let server = Server::bind(&socket).unwrap();
        let accepted = std::thread::spawn(move || server.accept());

        let flood_socket = socket.clone();
        let flooder = std::thread::spawn(move || {
            let Ok(mut caller) = UnixStream::connect(&flood_socket) else {
                return;
            };
            // A prefix that looks like a request, then an unbounded run of bytes with no newline.
            let _ = caller.write_all(b"{\"intent\":\"library\",\"junk\":\"");
            let chunk = vec![b'a'; 64 * 1024];
            for _ in 0..16 {
                if caller.write_all(&chunk).is_err() {
                    break;
                }
            }
            let _ = caller.flush();
            std::thread::sleep(Duration::from_millis(400));
            drop(caller);
        });

        std::thread::sleep(Duration::from_millis(100));
        send_intent(&socket, Intent::Finder).unwrap();
        assert_eq!(
            accepted.join().unwrap().unwrap(),
            Intent::Finder,
            "the capped request is skipped, not queued ahead of a real one"
        );
        flooder.join().unwrap();
    }

    #[test]
    fn the_second_holder_of_the_lock_is_told_to_forward_instead() {
        let dir = tempfile::tempdir().unwrap();
        let (_, lock) = paths(dir.path());

        let first = InstanceLock::acquire(&lock).unwrap();
        assert!(first.is_some(), "the first process takes the lock");
        assert!(
            InstanceLock::acquire(&lock).unwrap().is_none(),
            "a second process is refused"
        );

        drop(first);
        assert!(
            InstanceLock::acquire(&lock).unwrap().is_some(),
            "the lock is gone with the holder, so a killed process cannot wedge the app"
        );
    }

    #[test]
    fn binding_a_socket_a_live_instance_listens_on_fails() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, _) = paths(dir.path());
        let _listening = Server::bind(&socket).unwrap();
        let error = Server::bind(&socket).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn a_socket_file_left_by_a_dead_instance_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (socket, _) = paths(dir.path());
        // Simulate a kill: the file stays, nothing listens on it.
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(&socket, b"").unwrap();
        assert!(!instance_running(&socket));

        let second = Server::bind(&socket).unwrap();
        let accepted = std::thread::spawn(move || second.accept());
        send_intent(&socket, Intent::Library).unwrap();
        assert_eq!(accepted.join().unwrap().unwrap(), Intent::Library);
    }

    #[test]
    fn the_runtime_directory_holds_the_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("sub").join(SOCKET_NAME);
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::set_permissions(dir.path().join("sub"), Permissions::from_mode(0o755)).unwrap();
        let server = Server::bind(&socket).unwrap();
        let mode = std::fs::metadata(dir.path().join("sub"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o700,
            "the directory is the socket's access control"
        );
        let mode = std::fs::metadata(server.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn flags_name_intents_and_only_the_known_ones() {
        assert_eq!(Intent::from_flag("--finder"), Some(Intent::Finder));
        assert_eq!(Intent::from_flag("--library"), Some(Intent::Library));
        assert_eq!(Intent::from_flag("--reveal-secret"), None);
    }

    #[test]
    fn the_wire_shape_is_a_stable_kebab_case_intent_and_nothing_else() {
        let encoded = serde_json::to_string(&Request {
            intent: Intent::Finder,
        })
        .unwrap();
        assert_eq!(encoded, r#"{"intent":"finder"}"#);
        // Two shape hazards, both refused loudly rather than misread: a capability the shell must
        // not act on, and a field the receiver does not know about.
        assert!(serde_json::from_str::<Request>(r#"{"intent":"vault.unlock"}"#).is_err());
        assert!(serde_json::from_str::<Request>(
            r#"{"intent":"library","path":"/home/me/.ssh/id_ed25519"}"#
        )
        .is_err());
    }

    /// The wakeup path never carries a value that could be a secret: a request is exactly one
    /// `intent` field, and the only accepted values are two window names.
    #[test]
    fn a_request_can_carry_nothing_but_a_window_name() {
        let round_trip: Request = serde_json::from_str(r#"{"intent":"finder"}"#).unwrap();
        assert!(matches!(round_trip.intent, Intent::Finder));
        assert_eq!(
            serde_json::to_string(&round_trip).unwrap(),
            r#"{"intent":"finder"}"#
        );
        assert_eq!(Intent::ALL.len(), 2);
    }
}
