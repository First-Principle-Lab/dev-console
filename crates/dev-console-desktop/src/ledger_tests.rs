//! The gate over `docs/project/native-feasibility-ledger.md`.
//!
//! That ledger is checkpoint C1's evidence: a row-by-row statement of which native-feasibility
//! checks this container decides and which it cannot decide at all. A document like that rots in two
//! directions, and prose cannot stop either of them, so this test reads the file:
//!
//! - **a cited test that no longer exists** would leave a row claiming the container decides
//!   something it stopped checking, so every name in a container-settled row must be a `fn` in the
//!   real sources - which is also why a renamed test has to touch the ledger in the same change;
//! - **a check this container cannot make** must carry the command that settles it on the owner's
//!   Hyprland/NVIDIA session, so an `UNVERIFIED (owner desktop)` row with no command is a gap nobody
//!   can close, and a `NOT BUILT` row must not pretend otherwise by naming one.
//!
//! The vocabulary rows must use, the areas the ledger is organised by, and the four areas checkpoint
//! C1 names as its evidence are fixed here rather than left to the document's own say-so: a typo in
//! an area would otherwise drop those rows from the coverage check silently.
//!
//! What this gate cannot do is make an UNVERIFIED row true. It only keeps the honest labelling from
//! decaying into a confident-looking list.

use super::source_scan::rust_sources;

/// The ledger itself, found from the crate rather than the working directory so the gate runs from
/// any `cargo test` invocation.
const LEDGER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/project/native-feasibility-ledger.md"
);

/// The sources a cited test may live in: this crate's unit tests and the scripted element-tree
/// tests that drive the same windows.
const TEST_SOURCES: &[&str] = &[
    concat!(env!("CARGO_MANIFEST_DIR"), "/src"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../dev-console-ui-tests/src"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../dev-console-ui-tests/tests"),
];

/// Rows claiming a `#[test]` settles them. Every backticked name in such a row is a test.
const HEADLESS_TEST: &str = "HEADLESS-TEST";
/// Rows claiming the shipped binary was run here. They must name the binary and what it exited with.
const HEADLESS_RUN: &str = "HEADLESS-RUN";
/// Undecidable here; the owner's session decides it, with the command the row names.
const UNVERIFIED: &str = "UNVERIFIED (owner desktop)";
/// No such capability in the Rune shell yet, so no host can check it.
const NOT_BUILT: &str = "NOT BUILT";

const STATUSES: &[&str] = &[HEADLESS_TEST, HEADLESS_RUN, UNVERIFIED, NOT_BUILT];

/// Every area the ledger may file a row under. `CLAUSE_AREAS` is checked against these.
const AREAS: &[&str] = &[
    "two windows",
    "tray",
    "control socket",
    "single instance",
    "residency",
    "keyboard",
    "modal dialogs",
    "opaque Finder rendering",
    "portal folder selection",
    "secret hygiene",
];

/// The wiring checkpoint C1's evidence clause names. Each has to have at least one row this
/// container settles, or the clause is not evidenced however many rows the rest of the file has.
const CLAUSE_AREAS: &[&str] = &["two windows", "tray", "control socket", "single instance"];

/// The binary the `HEADLESS-RUN` rows have to name: the thing that was actually executed.
const RUN_BINARY: &str = "target/debug/dev-console";

/// The heading whose table this gate parses. The vocabulary table above it is documentation, not
/// rows, so the heading is what keeps the two apart.
const TABLE_HEADING: &str = "## The ledger";

struct Row {
    area: String,
    check: String,
    status: String,
    evidence: String,
    command: String,
    line: usize,
}

fn parse() -> Vec<Row> {
    let text = std::fs::read_to_string(LEDGER)
        .unwrap_or_else(|error| panic!("cannot read the ledger at {LEDGER}: {error}"));
    let mut rows = Vec::new();
    let mut in_table = false;
    for (index, line) in text.lines().enumerate() {
        if line.starts_with("## ") {
            in_table = line.trim() == TABLE_HEADING;
            continue;
        }
        if !in_table || !line.trim_start().starts_with('|') {
            continue;
        }
        let cells = cells(line.trim());
        let line = index + 1;
        if cells[0] == "Area" || cells.iter().all(|cell| is_separator(cell)) {
            continue;
        }
        assert_eq!(
            cells.len(),
            5,
            "ledger line {line} has {} cells, not Area/Check/Status/Evidence/Owner-command: {cells:?}",
            cells.len()
        );
        rows.push(Row {
            area: cells[0].clone(),
            check: cells[1].clone(),
            status: cells[2].clone(),
            evidence: cells[3].clone(),
            command: cells[4].clone(),
            line,
        });
    }
    rows
}

/// The cells of one table row. A `\|` inside a cell is an escaped pipe rather than a cell break, so
/// it is lifted out of the way before splitting and restored after - which is what lets a row quote
/// a `hyprctl ... | jq ...` pipeline. `\u{1}` cannot appear in the document.
const ESCAPED_PIPE: &str = "\u{1}";

/// The five cells of one table row, trimmed, with escaped pipes restored.
fn cells(line: &str) -> Vec<String> {
    let lifted = line.replace(r"\|", ESCAPED_PIPE);
    let parts: Vec<&str> = lifted.split('|').collect();
    let inner = &parts[1..parts.len() - 1];
    inner
        .iter()
        .map(|cell| cell.trim().replace(ESCAPED_PIPE, "|"))
        .collect()
}

fn is_separator(cell: &str) -> bool {
    !cell.is_empty() && cell.chars().all(|c| matches!(c, '-' | ':'))
}

/// Every `…` span in a cell, in order.
fn backticked(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(|span| (*span).to_owned())
        .collect()
}

fn test_sources() -> Vec<String> {
    TEST_SOURCES
        .iter()
        .flat_map(|root| rust_sources(root))
        .collect()
}

#[test]
fn the_ledger_is_a_table_this_gate_can_actually_read() {
    let rows = parse();
    assert!(
        rows.len() >= 20,
        "the table under {TABLE_HEADING:?} holds {} rows; either the heading moved, the table lost \
         its shape, or the ledger was rewritten as prose, and a gate reading nothing decides nothing",
        rows.len()
    );
    let mut statuses = std::collections::BTreeMap::new();
    for row in &rows {
        assert!(
            AREAS.contains(&row.area.as_str()),
            "ledger line {}: {area:?} is not an area the ledger is organised by ({}); a new area \
             has to be added to this gate as well, or its rows drop out of the coverage check",
            row.line,
            AREAS.join(", "),
            area = row.area
        );
        assert!(
            STATUSES.contains(&row.status.as_str()),
            "ledger line {}: {status:?} is outside the documented vocabulary ({}); a row cannot \
             invent its own kind of evidence",
            row.line,
            STATUSES.join(", "),
            status = row.status
        );
        assert!(
            !row.check.is_empty() && !row.evidence.is_empty(),
            "ledger line {} has an empty check or evidence cell",
            row.line
        );
        *statuses.entry(row.status.as_str()).or_insert(0usize) += 1;
    }
    assert!(
        *statuses.get(UNVERIFIED).unwrap_or(&0) >= 5,
        "only {:?} rows are labelled {UNVERIFIED:?}; this ledger exists to name what this host \
         cannot decide, and a list with almost none of those is not that",
        statuses.get(UNVERIFIED)
    );
}

#[test]
fn every_row_the_container_settles_points_at_something_that_exists() {
    let sources: Vec<(String, String)> = test_sources()
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
            (path, text)
        })
        .collect();
    assert!(
        sources.iter().any(|(path, _)| path.ends_with("/ui.rs")),
        "the ledger gate walked {} sources but none of them is the view file, which means the \
         roots above no longer describe this workspace",
        sources.len()
    );
    assert!(
        sources
            .iter()
            .any(|(path, _)| path.contains("dev-console-ui-tests")),
        "the ledger gate stopped walking the scripted element-tree tests, so a test name cited from \
         there would read as missing"
    );
    assert!(
        sources.len() >= 12,
        "the ledger gate walked only {} sources; a citation could be missing and nothing would \
         notice",
        sources.len()
    );

    let mut cited = 0;
    for row in parse() {
        match row.status.as_str() {
            HEADLESS_TEST => {
                assert_eq!(
                    row.command, "-",
                    "ledger line {}: a {HEADLESS_TEST} row is settled here, so the \
                     owner-desktop command has to be empty (-) rather than a promise",
                    row.line
                );
                let names = backticked(&row.evidence);
                assert!(
                    !names.is_empty(),
                    "ledger line {} claims a test settles it but names none",
                    row.line
                );
                for name in names {
                    assert!(
                        !name.is_empty()
                            && name.starts_with(|c: char| c.is_ascii_lowercase())
                            && name
                                .chars()
                                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                        "ledger line {} cites {name:?}, which is not the name of a test function",
                        row.line
                    );
                    cited += 1;
                    let needle = format!("fn {name}(");
                    assert!(
                        sources
                            .iter()
                            .any(|(_, text)| text.contains(needle.as_str())),
                        "ledger line {} cites `{name}`, which no source under this workspace \
                         defines. The row claims this container decides it, so the test has to \
                         exist: rename or delete the row in the same change that moves the test.",
                        row.line
                    );
                }
            }
            HEADLESS_RUN => {
                assert_eq!(
                    row.command, "-",
                    "ledger line {}: a {HEADLESS_RUN} row was executed here, so it needs no \
                     owner-desktop command (-)",
                    row.line
                );
                assert!(
                    row.evidence.contains(RUN_BINARY),
                    "ledger line {} claims the shipped binary was run but never names it \
                     ({RUN_BINARY}); \"the binary was executed\" is only evidence if the row says \
                     what was executed",
                    row.line
                );
                let observed_exit = row.evidence.match_indices("exit").any(|(index, _)| {
                    row.evidence[index..]
                        .chars()
                        .take_while(|c| *c != ';')
                        .any(|c| c.is_ascii_digit())
                });
                assert!(
                    observed_exit,
                    "ledger line {} reports running {RUN_BINARY} without recording what it exited \
                     with; the exit code is the observation",
                    row.line
                );
            }
            _ => {}
        }
    }
    assert!(
        cited >= 30,
        "the ledger cites {cited} test names overall; the four wired areas are supposed to be \
         backed by dozens of unit tests, and a ledger that cites almost none has stopped \
         describing the suite"
    );
}

#[test]
fn every_row_this_host_cannot_settle_names_the_command_that_settles_it() {
    for row in parse() {
        match row.status.as_str() {
            UNVERIFIED => {
                let commands = backticked(&row.command);
                assert!(
                    commands.iter().any(|command| command.contains(' ')),
                    "ledger line {} is {UNVERIFIED:?} but names no command: the whole point of \
                     carrying a row this host cannot settle is to say exactly what the owner runs \
                     on their Hyprland/NVIDIA session to settle it",
                    row.line
                );
                assert_ne!(
                    row.evidence, "-",
                    "ledger line {} is {UNVERIFIED:?} with no note on what the container does say \
                     about it (the spec asserted here, the reason it cannot be observed)",
                    row.line
                );
            }
            NOT_BUILT => {
                assert_eq!(
                    row.command, "-",
                    "ledger line {} is {NOT_BUILT:?}, so no host can run anything for it yet - and \
                     a command here would read as a check that is waiting to pass",
                    row.line
                );
                assert!(
                    row.evidence.len() > 20,
                    "ledger line {} is {NOT_BUILT:?} without saying what is missing or when it \
                     arrives, which is how a gap gets mistaken for a pass later",
                    row.line
                );
            }
            _ => {}
        }
        assert!(
            row.evidence != row.command,
            "ledger line {} repeats the same text in both cells",
            row.line
        );
        assert!(
            !row.check.contains('|'),
            "ledger line {}: the check cell must not contain a bare pipe",
            row.line
        );
    }
}

#[test]
fn the_four_wired_areas_are_all_backed_by_evidence_from_this_container() {
    let rows = parse();
    for area in CLAUSE_AREAS {
        let in_area: Vec<&Row> = rows.iter().filter(|row| &row.area == area).collect();
        assert!(
            !in_area.is_empty(),
            "the ledger has no rows for {area:?}, which checkpoint C1's evidence clause names \
             directly",
        );
        let settled = in_area
            .iter()
            .filter(|row| row.status == HEADLESS_TEST || row.status == HEADLESS_RUN)
            .count();
        assert!(
            settled >= 2,
            "only {settled} of the {area:?} rows are settled by something in this container; the \
             claim being made is that the {area} wiring is exercised headlessly, so at least two \
             rows have to point at a test or a run rather than at the owner's desktop",
            area = area.replace(' ', " the")
        );
    }
}
