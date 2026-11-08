//! Checkpoint C7 ("project fit and core principles") is written as a list of commands, not
//! adjectives, so this module *is* that list: every clause C7 names that can be decided from the
//! repository is one of the tests below, and each test reads the same file a reviewer would open.
//!
//! | C7 clause | where it is proven |
//! | --- | --- |
//! | `cargo fmt --all -- --check`, and the same for the `src-tauri` manifest | command, in the gate list |
//! | `cargo clippy --workspace --all-targets --locked -- -D warnings`, and the src-tauri one | command |
//! | both test suites green | command |
//! | no hard-coded colours in view code | [`crate::tests::no_view_source_hard_codes_a_colour`] (C5's gate, reused) |
//! | reuse existing modules instead of parallel bespoke helpers | this module reads through [`crate::source_scan`] and asserts against the shipped [`dev_console_core::history::looks_like_secret`] and the real [`dev_console_core::keyring`] constants; it defines no second rule for anything an existing module already decides |
//! | synthetic secrets only | [`no_native_source_carries_a_value_the_shipped_secret_rule_would_mask`] |
//! | buttons inside forms typed | [`every_button_inside_a_frontend_form_declares_its_type`] plus the detector's own fixture test |
//! | Finder stays opaque, `dev-console-finder`, 680x420, borderless | [`the_shipped_finder_window_is_still_opaque_borderless_and_680x420`] |
//! | Library stays a normal, tiled window | [`the_shipped_library_window_is_still_a_normal_titled_resizable_window`] |
//! | `WEBKIT_DISABLE_DMABUF_RENDERER` removed only with the Tauri runtime | [`the_webkit_compositor_default_is_still_installed_while_the_tauri_runtime_ships`] |
//! | unchanged XDG names, keyring pair, 120 s timeout, 64 KiB output cap | [`the_profile_layout_the_keyring_pair_and_the_execution_limits_are_the_shipped_ones`] |
//! | no `tauri::` in the new path | [`no_workspace_crate_names_a_tauri_type_or_declares_a_tauri_dependency`] |
//! | `## Unreleased` in `CHANGELOG.md` | [`the_release_notes_open_with_a_non_empty_unreleased_section`] |
//! | the toolkit stays pinned to the agreed rev | [`the_lockfile_still_pins_the_toolkit_to_the_agreed_revision`] |
//!
//! Two clauses cannot be repo gates and are reported by command instead of asserted here. The branch
//! stays `feat/rune-migration` cut from `dev`: a test that reads `.git/HEAD` passes in a detached
//! worktree and fails during a rebase, so it proves less than the `git status` a reviewer runs. And
//! "an `## Unreleased` line **per user-visible change**" needs a judgement about which diffs a user
//! can see; the test below only proves the section exists and holds an entry, and the judgement is
//! stated in the iteration report.

use std::path::{Path, PathBuf};

use crate::source_scan::{files_named, read, rust_sources, sources_with_extension};

/// The `rune` / `rune-kit` revision C1's clause pins and `Cargo.lock` has to keep carrying.
const RUNE_REVISION: &str = "250597b0a591881b5c0138370cbd5f405ed710db";

/// The three workspace crates, spelled from their directories so a new one cannot be forgotten
/// without the non-vacuity assertions below going red.
const WORKSPACE_CRATES: [&str; 3] = [
    "dev-console-core",
    "dev-console-desktop",
    "dev-console-ui-tests",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .expect("this crate lives at <repo>/crates/<name>, so the repo root is two levels up")
        .to_path_buf()
}

/// Reads a repository file by root-relative path, because every gate here has to read the file a
/// reviewer would open rather than a copy of it.
fn repo_file(relative: &str) -> String {
    read(&repo_root().join(relative))
}

fn crate_root(name: &str) -> String {
    repo_root().join("crates").join(name).display().to_string()
}

/// The builder chain that creates a window: the anchor line plus every following line that continues
/// the chain (a `.option(…)` call, or the comment that explains one). It stops at the first line
/// that does not continue, which in `windows.rs` is where the match arm closes - so the set is the
/// option list, not the whole file, and an option added somewhere else cannot satisfy the assertion.
fn option_set(text: &str, anchor: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !inside {
            if trimmed.starts_with(anchor) {
                inside = true;
                lines.push(trimmed.to_owned());
            }
            continue;
        }
        if trimmed.starts_with('.') || trimmed.starts_with("//") {
            lines.push(trimmed.to_owned());
        } else {
            break;
        }
    }
    let set = lines.join("\n");
    let options = set.matches("\n.").count();
    assert!(
        options >= 4,
        "the option set anchored at {anchor:?} was not walked properly ({options} continuation \
         lines read):\n{set}"
    );
    set
}

/// The comment-free part of a source file. An invariant about *code* must not be judged on prose:
/// the honest way to record a permitted exception is a comment that names it, and a gate that read
/// comments would fail the explanation rather than the violation. String and character literals are
/// copied verbatim, so a `"http://…"` inside one is not read as a comment start - and the secret
/// scan below still sees the literal.
fn code_only(text: &str) -> String {
    let bytes = text.as_bytes();
    // Bytes, not chars: the file is UTF-8 and the copy below is byte-at-a-time, so a `char`
    // accumulator would split every non-ASCII byte into mojibake.
    let mut out: Vec<u8> = Vec::with_capacity(text.len());
    let mut index = 0usize;
    let mut block_depth = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if block_depth > 0 {
            if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
                block_depth += 1;
                index += 2;
            } else if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                block_depth -= 1;
                index += 2;
            } else {
                if byte == b'\n' {
                    out.push(b'\n');
                }
                index += 1;
            }
            continue;
        }
        if byte == b'"' {
            let end = string_run_end(bytes, index);
            out.extend_from_slice(&text.as_bytes()[index..end]);
            index = end;
            continue;
        }
        if byte == b'\'' {
            let end = char_run_end(bytes, index);
            out.extend_from_slice(&text.as_bytes()[index..end]);
            index = end;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/'
            && bytes.get(index + 1) == Some(&b'!')
            && bytes.get(index + 2) == Some(&b'/')
            && at_line_start(text, index)
        {
            // A module doc line: the two slashes are split by the bang, so the test above cannot
            // see it. Reading it as code would let a doc sentence name an API and trip the very
            // gate the sentence documents.
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            block_depth = 1;
            index += 2;
            continue;
        }
        out.push(byte);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// True when only whitespace separates `index` from the start of its line.
fn at_line_start(text: &str, index: usize) -> bool {
    text[..index]
        .bytes()
        .rev()
        .find(|byte| !matches!(byte, b' ' | b'\t'))
        .is_none_or(|byte| byte == b'\n')
}

/// The byte index just past a `"`-delimited literal opened at `open`. Runs out at the closing quote,
/// or at the end of the line for an unterminated one (a raw string, whose body is data anyway).
fn string_run_end(bytes: &[u8], open: usize) -> usize {
    let mut index = open + 1;
    let mut escaped = false;
    while index < bytes.len() {
        let byte = bytes[index];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return index + 1;
        } else if byte == b'\n' {
            return index;
        }
        index += 1;
    }
    index
}

/// The byte index just past a `'…'` character literal opened at `open`, or past the apostrophe alone
/// when it does not open one (Rust has no bare apostrophes in code, so this is about not mistaking
/// the closing quote of one literal for the start of another).
fn char_run_end(bytes: &[u8], open: usize) -> usize {
    if bytes.get(open + 1) == Some(&b'\\') {
        if let Some(cut) = bytes
            .get(open + 2..)
            .and_then(|tail| tail.iter().position(|byte| *byte == b'\''))
        {
            return open + 2 + cut + 1;
        }
    } else if bytes.get(open + 2) == Some(&b'\'') {
        return open + 3;
    }
    open + 1
}

/// The string literals in a slice of comment-free code, with escape sequences resolved: ordinary
/// `"…"`, raw `r"…"` and `r#"…"#`, and byte strings. This is the shape a checked-in credential would
/// actually take, which is what the secret scan below needs - walking whole lines instead would read
/// a `key = ` in a comment as a candidate and miss one split across two concatenated literals.
fn string_literals(code: &str) -> Vec<String> {
    let bytes = code.as_bytes();
    let mut out = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'\'' {
            index = char_run_end(bytes, index);
            continue;
        }
        if byte == b'b' && bytes.get(index + 1) == Some(&b'"') {
            index += 1;
        }
        let raw_hashes = if bytes.get(index) == Some(&b'r') {
            let mut run = index + 1;
            while bytes.get(run) == Some(&b'#') {
                run += 1;
            }
            if bytes.get(run) == Some(&b'"') {
                run - (index + 1)
            } else {
                index += 1;
                continue;
            }
        } else if bytes.get(index) == Some(&b'"') {
            0
        } else {
            index += 1;
            continue;
        };
        let open = index + 1 + raw_hashes;
        let closer = format!("\"{}", "#".repeat(raw_hashes));
        let Some(relative) = code[open..].find(&closer) else {
            index = open;
            continue;
        };
        out.push(if raw_hashes == 0 {
            unescape(&code[open..open + relative])
        } else {
            code[open..open + relative].to_owned()
        });
        index = open + relative + closer.len();
    }
    out
}

/// The escapes that can appear inside a secret-shaped literal. Anything else is left alone: this is
/// for reading a literal as a value, not a second parser.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut escape = false;
    for ch in text.chars() {
        if escape {
            out.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else {
            out.push(ch);
        }
    }
    out
}

/// `Cargo.toml` has its own comment syntax, and a `# tauri` note explaining why a dependency is
/// *absent* must not read as one being present.
fn strip_toml_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let end = line.find('#').unwrap_or(line.len());
        out.push_str(&line[..end]);
        out.push('\n');
    }
    out
}

/// Every `<form …>…</form>` span in a component, so the rule is only applied where AGENTS.md states
/// it (an untyped `<button>` inside a form is the submit-by-default hazard; outside one it is not).
fn form_spans(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = lower[cursor..].find("<form") {
        let open = cursor + found;
        let Some(close) = lower[open..].find("</form>") else {
            break;
        };
        let end = open + close + "</form>".len();
        out.push(text[open..end].to_owned());
        cursor = end;
    }
    out
}

/// Every `<button …>` / `<Button …>` opening tag in a span, whitespace collapsed onto one line so a
/// tag the frontend wrapped over four lines is still read whole.
fn button_tags(span: &str) -> Vec<String> {
    let lower = span.to_lowercase();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = lower[cursor..].find("<button") {
        let open = cursor + found;
        let Some(relative) = span[open..].find('>') else {
            break;
        };
        let end = open + relative + 1;
        out.push(
            span[open..end]
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        );
        cursor = end;
    }
    out
}

/// A literal too long to print in full in a failure message - and, deliberately, one that cannot
/// round-trip a secret either.
fn abbreviate(text: &str) -> String {
    let taken: String = text.chars().take(48).collect();
    if text.chars().count() > 48 {
        format!("{taken}…")
    } else {
        taken
    }
}

fn relative_to_crates(path: &str) -> String {
    path.rsplit_once("/crates/")
        .map_or_else(|| path.to_owned(), |(_, tail)| format!("crates/{tail}"))
}

#[test]
fn the_shipped_finder_window_is_still_opaque_borderless_and_680x420() {
    let windows = repo_file("src-tauri/src/windows.rs");
    let set = option_set(&windows, ".title(\"dev-console-finder\")");
    for promised in [
        ".title(\"dev-console-finder\")",
        ".inner_size(FINDER_SIZE.0, FINDER_SIZE.1)",
        ".resizable(false)",
        ".decorations(false)",
        ".transparent(false)",
        ".always_on_top(true)",
        ".skip_taskbar(true)",
    ] {
        assert!(
            set.contains(promised),
            "the shipped palette window no longer asks for {promised}:\n{set}"
        );
    }
    assert!(
        !set.contains(".transparent(true)"),
        "a transparent Finder surface is the exact regression AGENTS.md names: on WebKitGTK 2.54 + \
         Hyprland/NVIDIA it loses its whole paint on pointer hover while staying mapped. Hyprland \
         rounds the corners; the opacity is not negotiable.\n{set}"
    );
    assert!(
        windows.contains("const FINDER_SIZE: (f64, f64) = (680.0, 420.0);"),
        "the palette is 680x420 exactly - anything larger shows as an empty band around it"
    );
    // The runtime's crate name assembled from parts, because this file is one of the sources
    // `no_workspace_crate_names_a_tauri_type_or_declares_a_tauri_dependency` reads: spelling the
    // path out would trip the gate it exists to prove (the same discipline as `hex_colour` in
    // this crate's colour gate).
    let size_request = format!(
        "set_size({}::LogicalSize::new(FINDER_SIZE.0, FINDER_SIZE.1))",
        "tauri"
    );
    assert!(
        windows.contains(&size_request),
        "the size request after mapping is what pairs with the title the owner's Hyprland rule \
         matches; without it the rule's size and the window's size disagree"
    );
    let native = repo_file("crates/dev-console-desktop/src/identity.rs");
    assert!(
        native.contains("pub const FINDER_TITLE: &str = \"dev-console-finder\";")
            && native.contains("pub const FINDER_SIZE: (f32, f32) = (680.0, 420.0);"),
        "the native shell has to carry the same identity as the window it replaces, or the ledger \
         rows about compositor rules describe a window nobody opens"
    );
}

#[test]
fn the_shipped_library_window_is_still_a_normal_titled_resizable_window() {
    let windows = repo_file("src-tauri/src/windows.rs");
    let set = option_set(&windows, ".title(\"Dev Console\")");
    for promised in [
        ".title(\"Dev Console\")",
        ".inner_size(1040.0, 700.0)",
        ".min_inner_size(760.0, 520.0)",
        ".resizable(true)",
    ] {
        assert!(
            set.contains(promised),
            "the shipped library window no longer asks for {promised}:\n{set}"
        );
    }
    // AGENTS.md: the library stays tiled and only the palette floats, so nothing may give the
    // library a palette-shaped option set. The compositor's rule matches the finder's title alone.
    for forbidden in [
        ".decorations(false)",
        ".always_on_top(true)",
        ".transparent(true)",
        ".skip_taskbar(true)",
    ] {
        assert!(
            !set.contains(forbidden),
            "{forbidden} on the library window would pull it out of the tiling layout the \
             compositor gives it, which is the thing AGENTS.md tells every agent not to do while \
             fixing the palette:\n{set}"
        );
    }
    assert!(
        !windows.contains(".floating(true)"),
        "there is no window-level float for every Dev Console window; the library must stay tiled"
    );
}

#[test]
fn the_webkit_compositor_default_is_still_installed_while_the_tauri_runtime_ships() {
    let main = repo_file("src-tauri/src/main.rs");
    assert!(
        main.contains("if std::env::var_os(\"WEBKIT_DISABLE_DMABUF_RENDERER\").is_none()")
            && main.contains("std::env::set_var(\"WEBKIT_DISABLE_DMABUF_RENDERER\", \"1\");"),
        "the WebKit dmabuf default stays installed until the Tauri runtime is actually gone: \
         without it WebKitGTK 2.54 commits Wayland buffers with no acquire timeline and Hyprland \
         kills the client the moment a window maps.\n{main}"
    );
    let native = repo_file("crates/dev-console-desktop/src/main.rs");
    assert!(
        !native.contains("set_var(\"WEBKIT_DISABLE_DMABUF_RENDERER\""),
        "the native binary must not install a WebKit default. It renders no webview, so the \
         default would outlive the runtime it was for - which is the condition AGENTS.md attaches \
         to removing it."
    );
}

#[test]
fn the_profile_layout_the_keyring_pair_and_the_execution_limits_are_the_shipped_ones() {
    // Assert against the real constants where they are public, and against the shipped source where
    // the value is a private const. Either way the assertion reads the code, not a doc comment.
    assert_eq!(dev_console_core::keyring::SERVICE, "dev-console");
    assert_eq!(dev_console_core::keyring::ACCOUNT, "vault-key");

    let paths = repo_file("crates/dev-console-core/src/paths.rs");
    for name in [
        "commands.sqlite3",
        "secrets.vault",
        "manual-lock",
        "settings.json",
        "recovery.agekey",
        "backups",
        "pending-replace",
        "dev-console",
    ] {
        assert!(
            paths.contains(&format!("\"{name}\"")),
            "the XDG profile layout is part of the shipped contract: {name} is gone from \
             paths.rs, so an existing profile would not be found by the next build.\n{paths}"
        );
    }

    let execution = repo_file("crates/dev-console-core/src/execution.rs");
    assert!(
        execution.contains("const MAX_OUTPUT_BYTES: usize = 64 * 1024;"),
        "the 64 KiB output cap is a shipped limit; raising it changes what the panel can show and \
         what a chatty command can fill"
    );
    assert!(
        execution.contains("const RUN_TIMEOUT: Duration = Duration::from_secs(120);"),
        "the 120 s run timeout is a shipped limit, not an implementation detail"
    );
}

#[test]
fn no_workspace_crate_names_a_tauri_type_or_declares_a_tauri_dependency() {
    // The needle assembled from parts, because this file is one of the sources the scan reads: a
    // literal that spelled the path out would be the one violation this gate could never clear -
    // the same reason `hex_colour` builds its fixtures from parts in this crate's colour gate.
    let runtime_path = format!("{}::", "tauri");
    let mut scanned = 0usize;
    let mut offenders = Vec::new();
    for name in WORKSPACE_CRATES {
        for path in rust_sources(&crate_root(name)) {
            scanned += 1;
            let code = code_only(&read(Path::new(&path)));
            if code.contains(&runtime_path) {
                offenders.push(relative_to_crates(&path));
            }
        }
    }
    assert!(
        scanned >= 30,
        "the runtime-free check walked only {scanned} sources; a crate name above is wrong or a \
         crate moved, and either way nothing would be checked"
    );
    assert!(
        offenders.is_empty(),
        "the workspace must not reach a runtime type: {}\nthe service layer returns `Effects` and \
         `AppEvent` values precisely so no windowing type crosses into it",
        offenders.join(", ")
    );

    let manifests = files_named(&crate_root(""), "Cargo.toml");
    assert!(
        manifests.len() >= WORKSPACE_CRATES.len(),
        "the manifest scan found only {manifests:?}; a crate is not where its name says"
    );
    for manifest in &manifests {
        let code = strip_toml_comments(&read(Path::new(manifest)));
        assert!(
            !code.contains("tauri"),
            "{} declares a tauri dependency; the workspace is the runtime-free half",
            relative_to_crates(manifest)
        );
    }
}

#[test]
fn every_button_inside_a_frontend_form_declares_its_type() {
    let root = repo_root().join("src");
    let views = sources_with_extension(&root.display().to_string(), "svelte");
    assert!(
        views.len() >= 15,
        "the frontend walk found {} .svelte files; a rename would silence this gate",
        views.len()
    );
    let mut forms = 0usize;
    let mut candidates = 0usize;
    let mut offenders = Vec::new();
    for view in &views {
        let text = read(Path::new(view));
        let short = view
            .rsplit_once("/src/")
            .map_or(view.as_str(), |(_, tail)| tail)
            .to_owned();
        for span in form_spans(&text) {
            forms += 1;
            for button in button_tags(&span) {
                candidates += 1;
                if !button.contains("type=") {
                    offenders.push(format!("{}: {}", short, abbreviate(&button)));
                }
            }
        }
    }
    assert!(
        forms >= 2 && candidates >= 4,
        "the walk found {forms} forms and {candidates} buttons inside them; AGENTS.md's rule is \
         only enforced if the walk still sees the forms that carry it"
    );
    assert!(
        offenders.is_empty(),
        "a button inside a form defaults to type=\"submit\", so it needs an explicit \
         type=\"button\" unless submitting is the point:\n{}",
        offenders.join("\n")
    );
}

/// The rule above is a grep, and a grep is only a gate if it can see what it forbids, so the
/// detector is tested against the shapes on both sides of the line. Without this, the scan could
/// pass by matching nothing at all - the failure mode grep-as-test is most likely to have.
#[test]
fn the_form_button_detector_sees_an_untyped_button_and_ignores_a_typed_one() {
    let offender = "<form>\n  <button onclick={run}>Go</button>\n</form>";
    let spans = form_spans(offender);
    assert_eq!(spans.len(), 1);
    let buttons = button_tags(&spans[0]);
    assert_eq!(buttons.len(), 1);
    assert!(
        !buttons[0].contains("type="),
        "the fixture is meant to be the offender case: {:?}",
        buttons[0]
    );

    let typed = "<form>\n  <Button type=\"button\" onclick={cancel}>Cancel</Button>\n  \
                 <button type=\"submit\">Save</button>\n  <button\n    type=\"button\"\n    \
                 onclick={pick}>Browse</button>\n</form>";
    let spans = form_spans(typed);
    let buttons = button_tags(&spans[0]);
    assert_eq!(buttons.len(), 3);
    assert!(
        buttons.iter().all(|button| button.contains("type=")),
        "multi-line and component tags must be read whole: {buttons:?}"
    );
    assert!(
        form_spans("<div><Button label=\"x\" /></div>").is_empty(),
        "a button outside a form is not this rule's business"
    );
    assert_eq!(
        form_spans("<form><button type=\"submit\">A</button></form>\n<form></form>").len(),
        2,
        "two forms in one component are two spans"
    );
    // The whitespace-collapse a wrapped tag needs, checked where it matters: a tag split across
    // four lines still reports its attributes.
    let wrapped =
        button_tags("<button\n  class=\"x\"\n  type=\"button\"\n  onclick={run}>Go</button");
    assert_eq!(wrapped.len(), 1);
    assert!(wrapped[0].contains("type=\"button\""), "{wrapped:?}");
}

#[test]
fn no_native_source_carries_a_value_the_shipped_secret_rule_would_mask() {
    // Reuse the rule instead of restating it. `history::looks_like_secret` is what decides today
    // that a command carrying a token never reaches the suggestions panel, so it is the strictest
    // available in-repo definition of "this looks like a real credential" - and a second bespoke
    // detector would be exactly the parallel helper C7 forbids.
    let mut scanned = 0usize;
    let mut offenders = Vec::new();
    for name in WORKSPACE_CRATES {
        for path in rust_sources(&crate_root(name)) {
            // The module that owns the rule has to contain examples of what it catches.
            if path.ends_with("/history.rs") {
                continue;
            }
            scanned += 1;
            let text = read(Path::new(&path));
            for literal in string_literals(&code_only(&text)) {
                for candidate in credential_shaped_pieces(&literal) {
                    offenders.push(format!(
                        "{}: {}",
                        relative_to_crates(&path),
                        abbreviate(&candidate)
                    ));
                }
            }
        }
    }
    assert!(
        scanned >= 40,
        "the secret scan walked only {scanned} sources; a crate name above is wrong or a crate \
         moved, and nothing would be checked"
    );
    assert!(
        offenders.is_empty(),
        "AGENTS.md allows synthetic values only. Each offender below is a 32+ character token that \
         the shipped rule in `dev_console_core::history` already classifies as a credential, minus \
         the git-object-id carve-out core makes for the same reason - so this adds no second \
         definition of \"credential\", and a real generated key could not hide from either rule:\n{}",
        offenders.join("\n")
    );
}

/// The credential-shaped runs inside one literal. A literal is split into maximal runs of the
/// token alphabet, and every run of 32+ characters is handed to the shipped rule
/// ([`dev_console_core::history::looks_like_secret`], whose high-entropy clause is what flags a
/// generated token), minus the one shape core itself carves out: a git object id. The only thing
/// added here is the *split*, because the shipped rule reads a command line and this scan reads a
/// literal; no second definition of "credential" is maintained.
fn credential_shaped_pieces(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '_' | '-') {
            run.push(ch);
        } else {
            if credential_shaped(&run) {
                out.push(run.clone());
            }
            run.clear();
        }
    }
    out
}

/// The narrowing, stated: core's rule reads a whole command line and is deliberately broad, so
/// this scan adds only what source text needs - a minimum length, the mixed-case-plus-digit shape
/// of a generated key, and core's own git-object-id carve-out. Without the shape test the scan
/// would flag a polkit action path, a fixture named `synthetic-token-do-not-print-8f2b` and the
/// toolkit revision this crate exists to protect, all of which are lowercase, all of which are
/// data, and none of which is what the rule was written to catch.
fn credential_shaped(run: &str) -> bool {
    if run.len() < 32 || looks_like_object_id(run) {
        return false;
    }
    let (mut lower, mut upper, mut digit) = (false, false, false);
    for byte in run.bytes() {
        lower |= byte.is_ascii_lowercase();
        upper |= byte.is_ascii_uppercase();
        digit |= byte.is_ascii_digit();
    }
    lower && upper && digit && looks_like_secret(run)
}

/// A 40- or 64-character lowercase hex run is a git object id: the carve-out
/// [`dev_console_core::history`] already makes for the same reason (a repo is full of commit ids,
/// and this crate's own rev pin is one). Without it, the scan would flag the toolkit revision it
/// exists to protect.
fn looks_like_object_id(run: &str) -> bool {
    matches!(run.len(), 40 | 64)
        && run
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn looks_like_secret(text: &str) -> bool {
    dev_console_core::history::looks_like_secret(text)
}

#[test]
fn the_release_notes_open_with_a_non_empty_unreleased_section() {
    let changelog = repo_file("CHANGELOG.md");
    let heading = "## Unreleased";
    let start = changelog
        .find(heading)
        .expect("AGENTS.md requires an `## Unreleased` section for changes not yet released");
    let rest = &changelog[start + heading.len()..];
    let body = match rest.find("\n## ") {
        Some(end) => &rest[..end],
        None => rest,
    };
    assert!(
        body.lines()
            .map(str::trim)
            .any(|line| line.starts_with("- ") || line.starts_with("* ")),
        "`## Unreleased` exists but holds no entry. The section is how a user finds out what \
         changed, so an empty one is not evidence of anything:\n{body}"
    );
}

#[test]
fn the_lockfile_still_pins_the_toolkit_to_the_agreed_revision() {
    let lock = repo_file("Cargo.lock");
    let pinned =
        format!("git+https://github.com/anomalyco/rune?rev={RUNE_REVISION}#{RUNE_REVISION}");
    let hits = lock.matches(&pinned).count();
    assert!(
        hits >= 2,
        "the lockfile has to carry the pinned toolkit rev for every rune crate (rune and rune-kit \
         at minimum); found {hits} entries"
    );
    for line in lock
        .lines()
        .filter(|line| line.starts_with("source = ") && line.contains("anomalyco/rune"))
    {
        assert!(
            line.contains(RUNE_REVISION),
            "a rune crate resolved to something other than the agreed rev: {line}"
        );
    }
    let workspace = repo_file("Cargo.toml");
    for dependency in ["rune =", "rune-kit ="] {
        let line = workspace
            .lines()
            .find(|line| line.trim_start().starts_with(dependency))
            .unwrap_or_else(|| panic!("the workspace no longer declares {dependency}"));
        assert!(
            line.contains(&format!("rev = \"{RUNE_REVISION}\"")),
            "{line}\nC1's clause is stated against one revision, so asking cargo for anything \
             else is how the pinned rev silently becomes a different one"
        );
    }
}
