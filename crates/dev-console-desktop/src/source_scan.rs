//! The source walk shared by the gates that read this repository's own files.
//!
//! Two tests have to look at sources rather than only at types: the colour scan in
//! [`crate::tests::no_view_source_hard_codes_a_colour`], which is AGENTS.md's "no hard-coded colours
//! in view code" rule turned into a gate, and [`crate::ledger_tests`], which checks the
//! native-feasibility ledger's citations against the tests that actually exist. Both fail in the
//! same way if the walk is wrong - by reading nothing and reporting success - so there is one walk
//! here, and each caller asserts on what it returned.

/// Every `.rs` file under `root`, sorted, so a new source is covered without anyone remembering to
/// add it to a list. Panics on an unreadable root: a gate that silently walked nothing would pass.
pub(crate) fn rust_sources(root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue = vec![root.to_string()];
    while let Some(dir) = queue.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("cannot list {dir} (a source walk needs it): {error}"));
        for entry in entries {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                queue.push(path.display().to_string());
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path.display().to_string());
            }
        }
    }
    out.sort();
    out
}
