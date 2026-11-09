//! The source walk shared by the gates that read this repository's own files.
//!
//! Three tests have to look at sources rather than only at types: the colour scan in
//! [`crate::tests::no_view_source_hard_codes_a_colour`], which is AGENTS.md's "no hard-coded colours
//! in view code" rule turned into a gate, [`crate::ledger_tests`], which checks the
//! native-feasibility ledger's citations against the tests that actually exist, and
//! [`crate::project_fit_tests`], which reads the shipped Tauri tree and the frontend to re-check the
//! invariants checkpoint C7 names. All three fail in the same way if the walk is wrong - by reading
//! nothing and reporting success - so there is one walk here, each caller asserts on what it returned,
//! and the extension filter is a parameter so a gate that reads `.svelte` or `.toml` reuses the same
//! queue instead of growing a fourth hand-rolled `read_dir` loop.

use std::path::{Path, PathBuf};

/// Every file under `root` whose extension is `extension`, sorted, so a new source is covered
/// without anyone remembering to add it to a list. Panics on an unreadable root: a gate that
/// silently walked nothing would pass.
pub(crate) fn sources_with_extension(root: &str, extension: &str) -> Vec<String> {
    walk(root)
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .map(|path| path.display().to_string())
        .collect()
}

/// Every file under `root` with exactly this file name, sorted - the `Cargo.toml` scan in
/// [`crate::project_fit_tests`] uses it, because a dependency can only be added to a manifest.
pub(crate) fn files_named(root: &str, name: &str) -> Vec<String> {
    walk(root)
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|file| file == name))
        .map(|path| path.display().to_string())
        .collect()
}

/// Every `.rs` file under `root`, sorted. Kept as its own name because two of the three gates are
/// Rust-only and read better for it; the walk itself is [`sources_with_extension`].
pub(crate) fn rust_sources(root: &str) -> Vec<String> {
    sources_with_extension(root, "rs")
}

/// Breadth-first over `root`, panicking if the root cannot be listed.
fn walk(root: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut queue = vec![PathBuf::from(root)];
    while let Some(dir) = queue.pop() {
        let entries = std::fs::read_dir(&dir).unwrap_or_else(|error| {
            panic!(
                "cannot list {} (a source walk needs it): {error}",
                dir.display()
            )
        });
        for entry in entries {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                queue.push(path);
            } else if path.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The file's text, with a panic that names the gate's reason for reading it.
pub(crate) fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!(
            "cannot read {} (a source gate has to read it): {error}",
            path.display()
        )
    })
}
