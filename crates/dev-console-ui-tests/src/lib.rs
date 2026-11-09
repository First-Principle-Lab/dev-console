//! Synthetic fixtures and scripted interactions for the desktop shell's headless tests.
//!
//! Each test builds the real Library or Finder over a fresh temporary profile
//! (`dev_console_core::testing::Fixture`: its own database, vault and settings, with a generated
//! vault key and no keyring), runs it in a `rune::Runtime` with no display, and drives it the way a
//! person would: presses on keyed elements, typed text and key chords. Blocking calls run inline
//! ([`Exec::Inline`]), so a press and its answer land in the same step.
//!
//! What a headless runtime cannot settle - pixels, focus rings, the compositor's placement of the
//! windows - is checked on the owner's desktop and recorded in
//! `docs/project/native-feasibility-ledger.md`.
//!
//! ## Isolation
//!
//! [`isolate`] runs once per test process before any profile exists: `HOME` points at a temporary
//! directory, so shell history and the zsh hook are never the real ones, and `wl-copy` resolves
//! to a stub that writes [`clipboard_file`] instead of the session clipboard. Every value in these
//! tests is synthetic.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use dev_console_core::models::Settings;
use dev_console_core::testing::Fixture;
use dev_console_desktop::app::{finder_spec, library_spec};
use dev_console_desktop::finder::Finder;
use dev_console_desktop::hub::{Exec, Hub};
use dev_console_desktop::library::Library;
use rune::{AppSpec, Cx, El, Env, Key, KeyEvent, Mods, Runtime, Tx};

pub const FINDER_VIEWPORT: (f32, f32) = (680.0, 420.0);
pub const LIBRARY_VIEWPORT: (f32, f32) = (1040.0, 700.0);

/// A secret value no test may ever find in an element tree, a log or a key.
pub const SYNTHETIC_SECRET: &str = "synthetic-vault-value-never-displayed";

pub struct Isolation {
    _home: tempfile::TempDir,
    clipboard: PathBuf,
}

static ISOLATION: OnceLock<Isolation> = OnceLock::new();

/// Points `HOME` and the clipboard provider at temporary stand-ins, once per process.
pub fn isolate() -> &'static Isolation {
    ISOLATION.get_or_init(|| {
        let home = tempfile::tempdir().expect("temporary home");
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).expect("stub bin");
        let clipboard = home.path().join("clipboard");
        let stub = bin.join("wl-copy");
        std::fs::write(
            &stub,
            format!("#!/bin/sh\ncat > '{}'\n", clipboard.display()),
        )
        .expect("wl-copy stub");
        let paste = bin.join("wl-paste");
        std::fs::write(
            &paste,
            format!("#!/bin/sh\ncat '{}' 2>/dev/null\n", clipboard.display()),
        )
        .expect("wl-paste stub");
        for script in [&stub, &paste] {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(script, std::fs::Permissions::from_mode(0o755))
                .expect("chmod stub");
        }
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        std::env::set_var("PATH", path);
        std::env::set_var("HOME", home.path());
        // Desktop sessions usually export XDG paths. Changing HOME alone does not isolate them.
        std::env::set_var("XDG_DATA_HOME", home.path().join("data"));
        std::env::set_var("XDG_CONFIG_HOME", home.path().join("config"));
        std::env::set_var("DEV_CONSOLE_DATA_DIR", home.path().join("data/dev-console"));
        std::env::set_var(
            "DEV_CONSOLE_CONFIG_DIR",
            home.path().join("config/dev-console"),
        );
        std::env::set_var("DEV_CONSOLE_RUNTIME_DIR", home.path().join("runtime"));
        std::env::remove_var("HISTFILE");
        std::env::set_var("WAYLAND_DISPLAY", "dev-console-test");
        std::env::remove_var("DISPLAY");
        Isolation {
            _home: home,
            clipboard,
        }
    })
}

/// Where the stub clipboard writes.
pub fn clipboard_file() -> PathBuf {
    isolate().clipboard.clone()
}

/// What the stub clipboard holds, if anything was copied.
pub fn clipboard_text() -> Option<String> {
    std::fs::read_to_string(clipboard_file())
        .ok()
        .filter(|text| !text.is_empty())
}

static CLIPBOARD: Mutex<()> = Mutex::new(());

/// Empties the stub clipboard and holds it for the caller: tests run in parallel and share one
/// stub, so a test that asserts what was copied keeps the guard until it is done.
pub fn clear_clipboard() -> MutexGuard<'static, ()> {
    let guard = CLIPBOARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _ = std::fs::remove_file(clipboard_file());
    guard
}

/// A temporary profile with the services both windows share.
pub struct Profile {
    pub fixture: Fixture,
    pub hub: Hub,
}

impl Profile {
    pub fn new(tag: &str) -> Profile {
        isolate();
        let fixture = Fixture::new(tag);
        // No history suggestions unless a test asks: the panel would otherwise read a history file.
        let settings = fixture.state.settings.get();
        fixture
            .state
            .settings
            .save(Settings {
                history_suggestions_enabled: false,
                ..settings
            })
            .expect("settings");
        let hub = Hub::new(Arc::clone(&fixture.state), "0.0.0-test");
        Profile { fixture, hub }
    }

    pub fn library(&self) -> Stage<Library> {
        Stage::from_spec(
            Library::new(self.hub.clone(), Exec::Inline),
            &library_spec(),
            LIBRARY_VIEWPORT,
        )
    }

    pub fn finder(&self) -> Stage<Finder> {
        Stage::from_spec(
            Finder::new(self.hub.clone(), Exec::Inline),
            &finder_spec(),
            FINDER_VIEWPORT,
        )
    }

    /// Saves a command through the same service the editor uses and returns its id.
    pub fn command(&self, name: &str, command: &str) -> String {
        let mut effects = dev_console_core::Effects::default();
        self.hub
            .library
            .save_entry(
                dev_console_core::models::SaveRequest {
                    draft: self.fixture.draft(name, command),
                },
                &mut effects,
            )
            .expect("save command")
            .0
            .id
    }

    pub fn secret(&self, name: &str, value: &str) -> String {
        self.fixture.seed_secret(name, value)
    }
}

/// A window's state in a headless runtime.
pub struct Stage<S: 'static> {
    runtime: Runtime<S>,
}

impl<S: 'static> Stage<S> {
    pub fn new(
        state: S,
        view: fn(&S, &mut Cx<S>) -> El,
        on_key: Option<fn(&mut S, &KeyEvent, &mut Tx) -> bool>,
        viewport: (f32, f32),
    ) -> Self {
        let mut runtime = Runtime::new(state, view, viewport, Env::new(true, 1.0));
        runtime.on_key = on_key;
        runtime.set_full_keyboard_access(true);
        runtime.use_clipboard(&rune::clipboard::memory());
        runtime.frame(0.0);
        let mut stage = Stage { runtime };
        stage.settle();
        stage
    }

    pub fn from_spec(state: S, spec: &AppSpec<S>, viewport: (f32, f32)) -> Self {
        Stage::new(state, spec.view, spec.on_key, viewport)
    }

    pub fn state(&self) -> &S {
        &self.runtime.state
    }

    /// Changes the state directly, as a handler would, and draws the result.
    pub fn apply<R>(&mut self, f: impl FnOnce(&mut S) -> R) -> R {
        let outcome = f(&mut self.runtime.state);
        self.settle();
        outcome
    }

    /// Draws the change and lets every spring and fade finish, so what is asserted next is the
    /// resting tree rather than a frame of an animation.
    pub fn settle(&mut self) {
        self.runtime.invalidate();
        for _ in 0..8 {
            self.runtime.frame(0.25);
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.runtime.find(key).is_some()
    }

    pub fn box_of(&self, key: &str) -> Option<([f32; 4], f32)> {
        self.runtime.presented(key)
    }

    pub fn centre_of(&self, key: &str) -> Option<(f32, f32)> {
        let ([x, y, width, height], _) = self.runtime.presented(key)?;
        Some((x + width / 2.0, y + height / 2.0))
    }

    /// Presses the element keyed `key`. False when it is not on screen.
    pub fn press(&mut self, key: &str) -> bool {
        let Some((x, y)) = self.centre_of(key) else {
            return false;
        };
        self.runtime.pointer_down(x, y);
        self.runtime.pointer_up(x, y);
        self.settle();
        true
    }

    /// Presses `key` and draws exactly one frame: what is on screen the moment the press lands,
    /// before any animation has run.
    pub fn press_once(&mut self, key: &str) -> bool {
        let Some((x, y)) = self.centre_of(key) else {
            return false;
        };
        self.runtime.pointer_down(x, y);
        self.runtime.pointer_up(x, y);
        self.runtime.frame(1.0 / 60.0);
        true
    }

    pub fn focused_key(&self) -> Option<String> {
        self.runtime
            .elements()
            .into_iter()
            .find(|element| element.focused)
            .and_then(|element| element.key)
    }

    pub fn label_of(&self, key: &str) -> Option<String> {
        self.runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .and_then(|element| element.label)
    }

    pub fn text_of(&self, key: &str) -> Option<String> {
        self.runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key))
            .and_then(|element| element.text)
    }

    /// Every accessible name and visible string in the tree.
    pub fn strings(&self) -> Vec<String> {
        self.runtime
            .elements()
            .into_iter()
            .flat_map(|element| [element.label, element.text].into_iter().flatten())
            .collect()
    }

    pub fn shows(&self, needle: &str) -> bool {
        self.strings().iter().any(|shown| shown.contains(needle))
    }

    /// Keys of the elements whose key starts with `prefix`, in tree order.
    pub fn keys_starting_with(&self, prefix: &str) -> Vec<String> {
        self.runtime
            .elements()
            .into_iter()
            .filter_map(|element| element.key)
            .filter(|key| key.starts_with(prefix))
            .collect()
    }

    /// Whether `needle` appears anywhere a value could leak from the tree: a name, a text or a key.
    pub fn leaks(&self, needle: &str) -> bool {
        self.shows(needle)
            || self.runtime.elements().iter().any(|element| {
                element
                    .key
                    .as_deref()
                    .is_some_and(|key| key.contains(needle))
            })
    }

    pub fn send_key(&mut self, ev: KeyEvent) {
        self.runtime.key(ev.clone());
        self.runtime.key_up(ev);
        self.settle();
    }

    pub fn press_key(&mut self, key: Key) {
        self.send_key(KeyEvent {
            key,
            mods: Mods::default(),
        });
    }

    pub fn press_ctrl(&mut self, c: char) {
        self.send_key(KeyEvent {
            key: Key::Command(c),
            mods: Mods {
                cmd: true,
                ctrl: true,
                ..Mods::default()
            },
        });
    }

    pub fn type_text(&mut self, text: &str) {
        self.press_key(Key::Text(text.to_owned()));
    }
}
