//! The gate over `docs/project/native-capability-matrix.md`, the artefact checkpoint C6's evidence
//! clause asks for: every user-visible behaviour the shipped app has, mapped either to the native
//! test that decides it or to an explicit statement of what is missing.
//!
//! A document like that is worth nothing as prose, because all three of its failure modes are
//! invisible to a reader: a cited test is renamed and the row keeps claiming coverage; a new
//! `test(...)` title appears in a spec and is never mapped; and a `NOT COVERED` row says "not yet"
//! without naming the seam, so the next reader cannot tell a missing view from a missing chord. So
//! this gate reads both sides - the Playwright and vitest titles straight out of `tests/`, and the
//! rows out of the matrix - and fails on any disagreement.
//!
//! The rules it enforces, and why each one exists:
//!
//! - **one row per shipped title, no more and no less.** A spec title with no row is a silent gap,
//!   which is the exact failure checkpoint C6 forbids; a row whose title no longer exists is the
//!   same document telling a story about a spec that has gone.
//! - **every cited test exists.** A backticked `snake_case` name in a coverage cell has to be a
//!   `#[test]` in this workspace, so renaming a test turns the matrix red instead of leaving a
//!   confident lie in a table cell.
//! - **a gap names its seams, and a `NATIVE-TEST` row names none.** The seam vocabulary is fixed
//!   here rather than left to the document, and the asymmetry is the point: a row cannot both claim
//!   coverage and admit a gap, so "we cover X except for Y" has to be `PARTIAL`.
//! - **the summary tables add up.** Totals are what people quote, and a summary that drifted from
//!   its own rows is how a gap list turns into a score.

use super::source_scan::rust_sources;
use std::collections::BTreeMap;
use std::path::Path;

/// The matrix itself, found from the crate rather than the working directory so the gate runs from
/// any `cargo test` invocation.
const MATRIX: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/project/native-capability-matrix.md"
);

/// The two spec trees the matrix has to inventory: the shipped app's behaviour (Playwright) and the
/// pure logic the views call (vitest).
const SPEC_ROOTS: &[(&str, &str)] = &[
    (
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/e2e"),
        "tests/e2e",
    ),
    (
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/unit"),
        "tests/unit",
    ),
];

/// The two headings whose `###` tables hold mapped rows. Everything else in the file is prose or
/// summary tables, checked separately.
const SPEC_SECTIONS: &[&str] = &["## The Playwright specs", "## The vitest units"];

const NATIVE_TEST: &str = "NATIVE-TEST";
const PARTIAL: &str = "PARTIAL";
const NOT_COVERED: &str = "NOT COVERED";
const STATUSES: &[&str] = &[NATIVE_TEST, PARTIAL, NOT_COVERED];

/// Every seam a gap row may name. Fixed here on purpose: a vocabulary the document could extend at
/// will is a vocabulary that ends up saying "not yet" in a new spelling.
const SEAMS: &[&str] = &[
    "a11y-semantics",
    "argv",
    "backups",
    "clipboard",
    "details",
    "dialog",
    "editor",
    "environment",
    "events",
    "focus-management",
    "folder-picker",
    "history-suggestions",
    "keyboard-delivery",
    "library-list",
    "paint",
    "project-import",
    "recovery",
    "responsive-layout",
    "run-output",
    "search-ranking",
    "settings",
    "theme-switch",
    "updates",
    "vault-state",
    "window-lifecycle",
];

/// One mapped row of the matrix.
struct Row {
    spec: String,
    title: String,
    status: String,
    coverage: String,
    line: usize,
}

/// The escaped-pipe marker, so a behaviour title or a coverage cell can quote a pipeline.
const ESCAPED_PIPE: &str = "\u{1}";

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

/// The text between a pair of backticks, in order.
fn backticked(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(|span| (*span).to_owned())
        .collect()
}

fn matrix_text() -> String {
    std::fs::read_to_string(MATRIX)
        .unwrap_or_else(|error| panic!("cannot read the capability matrix at {MATRIX}: {error}"))
}

/// Every mapped row, keyed to the spec file it belongs to.
fn parse_rows() -> Vec<Row> {
    let text = matrix_text();
    let mut rows = Vec::new();
    let mut in_section = false;
    let mut spec = String::new();
    for (index, line) in text.lines().enumerate() {
        let line_no = index + 1;
        if let Some(file) = line.strip_prefix("### ") {
            spec = file.trim().to_string();
            continue;
        }
        if line.starts_with("## ") {
            in_section = SPEC_SECTIONS.contains(&line.trim());
            spec.clear();
            continue;
        }
        if line.starts_with('#') {
            spec.clear();
            continue;
        }
        if !in_section || !line.trim_start().starts_with('|') {
            continue;
        }
        let cells = cells(line.trim());
        assert_eq!(
            cells.len(),
            3,
            "matrix line {line_no} has {} cells, not Behaviour/Status/Coverage: {cells:?}",
            cells.len()
        );
        if cells[0] == "Behaviour (the test title)" || cells.iter().all(|cell| is_separator(cell)) {
            continue;
        }
        let title = cells[0].strip_prefix('`').and_then(|t| t.strip_suffix('`'));
        let title = title.unwrap_or_else(|| {
            panic!(
                "matrix line {line_no}: the behaviour cell is not a quoted test title: {:?}",
                cells[0]
            )
        });
        assert!(
            STATUSES.contains(&cells[1].as_str()),
            "matrix line {line_no}: {:?} is outside the documented vocabulary {STATUSES:?}",
            cells[1]
        );
        assert!(
            !spec.is_empty(),
            "matrix line {line_no}: a row above its spec file (`### tests/...`) header, so nothing \
             knows which spec the title belongs to"
        );
        rows.push(Row {
            spec: spec.clone(),
            title: title.to_string(),
            status: cells[1].clone(),
            coverage: cells[2].clone(),
            line: line_no,
        });
    }
    rows
}

/// The `tests/...` paths whose titles the matrix must inventory, sorted, with every `test(...)` or
/// `it(...)` title in each. Titles are read the same way the document quotes them: the first quoted
/// string on a line that opens a test.
fn spec_titles() -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (root, prefix) in SPEC_ROOTS {
        let dir = Path::new(root);
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("cannot list the spec directory {root}: {error}"));
        let mut files: Vec<String> = entries
            .filter_map(|entry| {
                let path = entry.expect("a readable entry").path();
                is_spec_file(&path).then(|| path.display().to_string())
            })
            .collect();
        files.sort();
        assert!(
            !files.is_empty(),
            "no spec files under {root}, so the matrix would be inventoring nothing"
        );
        for file in files {
            let name = format!(
                "{prefix}/{}",
                Path::new(&file)
                    .file_name()
                    .expect("a file name")
                    .to_string_lossy()
            );
            let text = std::fs::read_to_string(&file)
                .unwrap_or_else(|error| panic!("cannot read {file}: {error}"));
            let mut found = Vec::new();
            for line in text.lines() {
                let Some(title) = test_title(line) else {
                    continue;
                };
                assert!(
                    !found.contains(&title),
                    "{name} declares the title {title:?} twice; the matrix keys one row per title"
                );
                found.push(title);
            }
            assert!(
                !found.is_empty(),
                "{name} yielded no test titles, so the extraction is wrong rather than the spec empty"
            );
            out.insert(name, found);
        }
    }
    out
}

/// Only the spec files themselves, not the helpers sitting beside them: `helpers.ts` declares no
/// behaviour and would otherwise make the gate demand a row for it.
fn is_spec_file(path: &std::path::Path) -> bool {
    let name = path
        .file_name()
        .expect("a file name")
        .to_string_lossy()
        .to_string();
    name.ends_with(".spec.ts") || name.ends_with(".test.ts")
}

/// The title of a `test('...')` / `it("...")` / ``test(`...`)`` declaration on this line, if the
/// line opens one. Mirrors how the document quotes titles: one line, first quoted string.
fn test_title(line: &str) -> Option<String> {
    let rest = line
        .trim_start()
        .strip_prefix("test")
        .or_else(|| line.trim_start().strip_prefix("it"))?;
    let mut rest = rest;
    while let Some(after) = rest.strip_prefix('.') {
        let name_len = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        rest = &after[name_len..];
    }
    let rest = rest.strip_prefix('(')?.trim_start();
    let quote = rest
        .chars()
        .next()
        .filter(|c| matches!(c, '\'' | '"' | '`'))?;
    let body = &rest[quote.len_utf8()..];
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                out.push(chars.next()?);
            }
            c if c == quote => {
                return Some(out);
            }
            c => out.push(c),
        }
    }
    None
}

/// Every `#[test]` function name in this workspace, so a cited name can be resolved. The matrix
/// cites the ported service, core and shell tests alike, so the walk covers the whole workspace
/// plus the shipped Tauri tree's own tests.
fn test_functions() -> Vec<String> {
    let roots = [
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/src"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../src-tauri/tests"),
    ];
    let mut names = Vec::new();
    for root in roots {
        for path in rust_sources(root) {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
            names.extend(test_fns_in(&text));
        }
    }
    names.sort();
    names.dedup();
    assert!(
        names.len() > 300,
        "the test-name walk found only {} `#[test]` functions; the matrix cites tests from all over          the workspace, so a walk this thin would flag honest rows as bad citations",
        names.len()
    );
    names
}

/// The `fn` names a `#[test]` attribute introduces. Deliberately textual rather than parsed: any
/// identifier following the attribute counts, which is exactly the over-approximation a citation
/// check wants (a stray `#[test]` in an embedded source string costs nothing here).
fn test_fns_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut index = 0;
    while let Some(found) = text[index..].find("#[test]") {
        let start = index + found + "#[test]".len();
        let rest = &text[start..];
        let Some(at) = rest.find("fn ") else { break };
        let name: String = rest[at + 3..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
        index = start + at + 3;
    }
    out
}

/// The seams named by a coverage cell: the token list after `Seam:`, read until the sentence stops
/// being a list of them, so a cell can add prose or citations after the list.
fn parse_seams(cell: &str) -> Vec<String> {
    let Some((_, after)) = cell.split_once("Seam:") else {
        return Vec::new();
    };
    let mut rest = after;
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        let len = rest
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
            .count();
        if len == 0 {
            return out;
        }
        out.push(rest[..len].to_string());
        rest = &rest[len..];
        match rest.trim_start().strip_prefix(',') {
            Some(after) => rest = after,
            None => return out,
        }
    }
}

/// A cited name: backticked, all lowercase with underscores, no spaces. File paths, `El`,
/// `vault_unlocked` in prose and API names are deliberately not citations.
fn citations(cell: &str) -> Vec<String> {
    backticked(cell)
        .into_iter()
        .filter(|name| {
            name.contains('_')
                && !name.contains(' ')
                && name.starts_with(|c: char| c.is_ascii_lowercase())
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        .collect()
}

#[test]
fn every_shipped_behaviour_title_has_exactly_one_matrix_row() {
    let rows = parse_rows();
    assert!(
        rows.len() >= 150,
        "the matrix parsed {} rows; it inventories one per Playwright and vitest title, so a count \
         this low means the section or table headings moved and the gate would be reading nothing",
        rows.len()
    );
    let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for row in &rows {
        *seen.entry((&row.spec[..], &row.title[..])).or_default() += 1;
    }
    let specs = spec_titles();
    let mut duplicates = Vec::new();
    let mut unmapped = Vec::new();
    let mut stale = Vec::new();
    let mut total_titles = 0usize;
    for (file, titles) in &specs {
        for title in titles {
            total_titles += 1;
            match seen.get(&(file.as_str(), title.as_str())) {
                None => unmapped.push(format!("{file}: {title}")),
                Some(1) => {}
                Some(count) => duplicates.push(format!("{file}: {title} ({count} rows)")),
            }
        }
    }
    for key in seen.keys() {
        let known = specs
            .get(key.0)
            .is_some_and(|titles| titles.iter().any(|title| title == key.1));
        if !known {
            stale.push(format!("{}: {}", key.0, key.1));
        }
    }
    assert!(
        unmapped.is_empty(),
        "{} of the {} shipped behaviour titles have no row in the matrix: {}\nA gap the matrix does \
         not state is the exact thing checkpoint C6's evidence clause forbids, so map it - a \
         `NOT COVERED` row costs nothing but has to name the seam it waits on.",
        unmapped.len(),
        total_titles,
        unmapped.join("\n  ")
    );
    assert!(
        stale.is_empty(),
        "the matrix maps titles no spec declares: {}\nEither the test was renamed (move the row) or \
         it was deleted (delete the row), but a row about a test that no longer runs is a claim about \
         a behaviour nobody has.",
        stale.join("\n  ")
    );
    assert!(
        duplicates.is_empty(),
        "a title mapped more than once makes every total above it wrong: {}",
        duplicates.join(", ")
    );
    let files = spec_titles().len();
    assert!(
        files >= 16,
        "the walk found {files} spec files; the shipped app has 11 Playwright and 5 vitest files, so \
         a smaller count means the roots above stopped describing this repository"
    );
}

#[test]
fn every_matrix_citation_names_a_test_that_actually_exists() {
    let names = test_functions();
    let mut cited = 0usize;
    for row in parse_rows() {
        for citation in citations(&row.coverage) {
            cited += 1;
            assert!(
                names.binary_search(&citation).is_ok(),
                "matrix line {}: `{citation}` is cited as the rule underneath the {:?} row \n  {}\nbut \
                 no `#[test]` in this workspace is called that. Rename the row in the same change \
                 that renames the test - a citation that resolves to nothing is how a gap list \
                 quietly becomes a coverage claim.",
                row.line,
                row.status,
                row.title
            );
        }
        assert!(
            row.status == NOT_COVERED || row.coverage.contains('`'),
            "matrix line {}: a {:?} row has to name at least one test, either the one that decides it \
             or the lower-layer rule that is already ported",
            row.line,
            row.status
        );
    }
    assert!(
        cited >= 120,
        "the matrix cites {cited} tests overall; every covered row quotes the test that decides it, \
         so a count this low means the citation scan stopped matching the cells"
    );
}

#[test]
fn every_gap_names_its_missing_seam_and_no_covered_row_names_one() {
    let mut gaps = 0usize;
    for row in parse_rows() {
        let seams = parse_seams(&row.coverage);
        for seam in &seams {
            assert!(
                SEAMS.contains(&seam.as_str()),
                "matrix line {}: {seam:?} is not a seam this gate knows ({}). A gap that invents a \
                 new name for an old missing screen is how the summary table stops meaning anything.",
                row.line,
                SEAMS.join(", ")
            );
        }
        assert!(
            row.coverage.matches("Seam:").count() <= 1,
            "matrix line {}: two `Seam:` markers, so the summary cannot tell which list belongs to \
             the row",
            row.line
        );
        match row.status.as_str() {
            NATIVE_TEST => assert!(
                seams.is_empty(),
                "matrix line {}: a {NATIVE_TEST} row names the missing seams {:?}. A row cannot both \
                 claim coverage and admit a gap - if part of it is missing, it is {PARTIAL}.",
                row.line,
                seams
            ),
            NOT_COVERED | PARTIAL => {
                assert!(
                    !seams.is_empty(),
                    "matrix line {}: a {:?} row has to name the seam it waits on, from the fixed \
                     vocabulary; \"not yet\" is not evidence and tells the next reader nothing.",
                    row.line,
                    row.status
                );
                gaps += 1;
            }
            other => unreachable!("status vocabulary checked when parsing: {other}"),
        }
    }
    // 27 paint rows and the 4 updater rows cannot move until a renderer check and the updater
    // exist, so fewer gaps than that means the seam marker stopped parsing, not that they closed.
    assert!(
        gaps >= 31,
        "only {gaps} rows name a missing seam; the paint and updater rows alone are 31, so a count \
         this low means either rows moved up without tests or the seam marker stopped parsing"
    );
}

#[test]
fn the_matrix_summary_counts_match_its_rows() {
    let rows = parse_rows();
    let text = matrix_text();
    let mut status: BTreeMap<&str, usize> = BTreeMap::new();
    let mut per_spec: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    let mut seams: BTreeMap<String, usize> = BTreeMap::new();
    for row in &rows {
        *status.entry(row.status.as_str()).or_default() += 1;
        *per_spec
            .entry(row.spec.as_str())
            .or_default()
            .entry(row.status.as_str())
            .or_default() += 1;
        for seam in parse_seams(&row.coverage) {
            *seams.entry(seam).or_default() += 1;
        }
    }

    // `| NATIVE-TEST | 24 |` and friends, in the status summary table.
    for name in STATUSES {
        let row = format!("| {name} | ");
        let line = summary_line(&text, &row);
        let stated: usize = cell_of(&line, 1).parse().expect("a count");
        assert_eq!(
            stated,
            status.get(name).copied().unwrap_or_default(),
            "the summary table says {stated} {name} rows and the rows above say {}",
            status.get(name).copied().unwrap_or_default()
        );
    }
    let total_line = summary_line(&text, "| **Total** | ");
    assert_eq!(
        cell_of(&total_line, 1).parse::<usize>().expect("a count"),
        rows.len(),
        "the summary total is not the number of rows"
    );

    // Every seam in a gap row appears in the seam table with the right count, and no seam table
    // row is stale.
    let mut stated_seams: BTreeMap<&str, usize> = BTreeMap::new();
    for line in section_lines(&text, "## Where the gaps cluster") {
        if !line.starts_with('|') || line.contains("Missing seam") || is_separator_row(&line) {
            continue;
        }
        let cells = cells(line.trim());
        if cells.len() != 3 {
            continue;
        }
        let name = cells[0]
            .trim_start_matches('`')
            .trim_end_matches('`')
            .to_string();
        if !SEAMS.contains(&name.as_str()) {
            continue;
        }
        stated_seams.insert(
            Box::leak(name.into_boxed_str()),
            cells[1].parse().expect("a count"),
        );
    }
    for seam in SEAMS {
        let actual = seams
            .iter()
            .filter(|(name, _)| name == seam)
            .map(|(_, count)| count)
            .copied()
            .sum::<usize>();
        let stated = *stated_seams.get(seam).unwrap_or(&0);
        assert_eq!(
            stated, actual,
            "the seam table says {stated} rows wait on {seam:?} and the rows say {actual}"
        );
    }
    assert_eq!(
        stated_seams.values().sum::<usize>(),
        seams.values().sum::<usize>(),
        "the seam table's total does not match the seams named by the rows"
    );

    // Per-spec table.
    for (spec, by_status) in &per_spec {
        let line = summary_line(&text, &format!("| `{spec}` | "));
        let stated_total: usize = cell_of(&line, 1).parse().expect("a count");
        let actual: usize = by_status.values().sum();
        assert_eq!(
            stated_total, actual,
            "the per-spec table disagrees with the rows for {spec}"
        );
        for name in STATUSES {
            let index = 2 + STATUSES
                .iter()
                .position(|s| s == name)
                .expect("a known status");
            let stated: usize = cell_of(&line, index).parse().expect("a count");
            assert_eq!(
                stated,
                by_status.get(name).copied().unwrap_or_default(),
                "the per-spec table says {stated} {name} rows for {spec} and the rows say {}",
                by_status.get(name).copied().unwrap_or_default()
            );
        }
    }
    let specs_in_table = section_lines(&text, "## Where the gaps cluster")
        .into_iter()
        .filter(|line| line.starts_with("| `tests/") && !line.contains("###"))
        .count();
    assert_eq!(
        specs_in_table,
        per_spec.len(),
        "the per-spec table has {specs_in_table} rows for {} spec files, so one is missing or stale",
        per_spec.len()
    );
}

/// The one line in the summary section that starts with `prefix`.
fn summary_line(text: &str, prefix: &str) -> String {
    let found: Vec<String> = section_lines(text, "## Where the gaps cluster")
        .into_iter()
        .filter(|line| line.starts_with(prefix))
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one summary row starting {prefix:?}, found {}",
        found.len()
    );
    found.into_iter().next().expect("the row")
}

/// The lines after a `## ` heading, up to the next one.
fn section_lines(text: &str, heading: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("## ") {
            inside = line.trim() == heading;
            continue;
        }
        if inside {
            out.push(line.to_string());
        }
    }
    assert!(!out.is_empty(), "the matrix has no section {heading:?}");
    out
}

fn cell_of(line: &str, index: usize) -> String {
    let cells = cells(line.trim());
    assert!(
        cells.len() > index,
        "summary row {line:?} has no cell {index}"
    );
    cells[index].replace(['`', '*'], "").trim().to_string()
}

fn is_separator_row(line: &str) -> bool {
    cells(line.trim()).iter().all(|cell| is_separator(cell))
}
