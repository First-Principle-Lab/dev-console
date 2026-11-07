//! Import a project folder's own runnable tasks as saved commands.
//!
//! A repository already knows how to run itself: `package.json` scripts, Make targets, just
//! recipes, Docker Compose services. This module reads those definitions out of one folder and
//! turns them into commands that run *in that folder*, so the library stops being the only place
//! a task has to be typed in by hand.
//!
//! Three rules shape the whole module:
//!
//! - **Reading only.** A scan never executes the project. The single exception is `just --dump`,
//!   a read-only serialisation of the justfile: it runs as an argument array (never a shell
//!   string), with a timeout that kills the child, and its output is only ever parsed, never run.
//!   Everything here still works with `just` absent.
//! - **Nothing runs on import either.** Imported command text is the same kind of text a user can
//!   type by hand, so it goes through the existing execution approval: the first Run of an
//!   imported task is confirmed exactly like a hand-written one.
//! - **The folder is the identity.** Every imported command gets the scanned folder as its working
//!   directory, and `(command, working_directory)` is what marks a candidate as already saved.

use std::collections::BTreeSet;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{EntryDraft, Kind};
use crate::repository::Repository;

/// A project folder can hold a 40 MiB lockfile or a generated `compose.yaml`. None of those are
/// worth reading to discover a handful of tasks, and none are worth holding in memory.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// How long `just --dump` may take before the fallback parser is used instead. A frozen `just`
/// must not freeze the library window with it.
const JUST_DUMP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Longest task description kept (`package.json` script bodies are the usual candidate).
const MAX_DESCRIPTION_CHARS: usize = 200;

/// Compact separator between the project and task names in the library and review list.
const NAME_SEPARATOR: char = '·';

const PACKAGE_JSON: &str = "package.json";
const MAKEFILES: [&str; 3] = ["GNUmakefile", "makefile", "Makefile"];
const JUSTFILES: [&str; 2] = ["justfile", ".justfile"];
const COMPOSE_FILES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// Where a task was found. Drives the review dialog's grouping and the import order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Npm,
    Make,
    Just,
    Compose,
}

impl Source {
    /// Heading for the group in the review dialog.
    pub fn label(self) -> &'static str {
        match self {
            Source::Npm => "package.json scripts",
            Source::Make => "Makefile targets",
            Source::Just => "just recipes",
            Source::Compose => "Docker Compose services",
        }
    }

    /// Short discriminator, used only to separate a task that two sources both define.
    fn slug(self) -> &'static str {
        match self {
            Source::Npm => "npm",
            Source::Make => "make",
            Source::Just => "just",
            Source::Compose => "compose",
        }
    }
}

/// One importable task, already named the way the library will store it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub source: Source,
    /// `<project><separator><task>`, e.g. `dev-console·test:e2e`. Unique within a scan, and the
    /// identity the import selection is keyed on.
    pub name: String,
    /// Exact text that will be run; shown verbatim in the review list.
    pub command: String,
    pub description: String,
    /// `entry_type` of the saved command: the tool the task belongs to (`bun`, `make`, ...).
    pub entry_type: String,
    /// A saved command already runs exactly this text in this folder.
    pub already_imported: bool,
}

/// Result of reading one project folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScan {
    /// Repository name: the folder holding `.git` when there is one, else the scanned folder.
    pub project: String,
    /// Canonical absolute path, used as the working directory of every imported command.
    pub dir: String,
    pub candidates: Vec<Candidate>,
}

/// Counts of an import, in the shape of `crate::import::Outcome`. `renamed` has no equivalent: a
/// task is only ever created, and "already there" is reported as `duplicates`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub imported: usize,
    /// Selected but already saved: re-running an import is a normal thing to do, not an error.
    pub duplicates: usize,
    /// Names that could not be saved, each with the reason.
    pub skipped: Vec<String>,
}

/// A task as a parser found it, before naming and dedupe.
struct Task {
    source: Source,
    entry_type: &'static str,
    /// Task name within the project (`test:e2e`, `up-api`). Doubles as the CLI name users type.
    task: String,
    command: String,
    description: String,
}

fn task(
    source: Source,
    entry_type: &'static str,
    task: String,
    command: String,
    description: String,
) -> Task {
    Task {
        source,
        entry_type,
        task,
        command,
        description,
    }
}

// ---------------------------------------------------------------------------
// entry points
// ---------------------------------------------------------------------------

/// Read a project folder and report every task it defines.
pub fn scan(path: &str, repo: &Repository) -> AppResult<ProjectScan> {
    let dir = target_dir(path)?;
    let working_directory = dir.to_string_lossy().into_owned();
    let saved = saved_executables(repo)?;
    let project = project_name(&dir);

    let mut tasks = Vec::new();
    tasks.extend(package_json_tasks(&dir)?);
    tasks.extend(makefile_tasks(&dir)?);
    tasks.extend(justfile_tasks(&dir)?);
    tasks.extend(compose_tasks(&dir)?);

    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut candidates = Vec::with_capacity(tasks.len());
    for t in tasks {
        let base = format!("{project}{NAME_SEPARATOR}{}", t.task);
        let already_imported = saved.contains(&(t.command.clone(), working_directory.clone()));
        candidates.push(Candidate {
            source: t.source,
            // The name a user selects is the name that gets saved, so a task two sources both
            // define (a `build` script *and* a `build` target) is distinguished here rather than
            // dropped, or reported as an error that would block the whole folder.
            name: unique_name(base, t.source, &mut taken),
            command: t.command,
            description: clip(&t.description),
            entry_type: t.entry_type.to_string(),
            already_imported,
        });
    }
    candidates.sort_by(|a, b| (a.source, &a.name).cmp(&(b.source, &b.name)));
    Ok(ProjectScan {
        project,
        dir: working_directory,
        candidates,
    })
}

/// Give every candidate a distinct name. The first task to claim a name keeps it, so rescanning a
/// folder reproduces the names its commands were saved under; only a later claimant is renamed.
fn unique_name(base: String, source: Source, taken: &mut BTreeSet<String>) -> String {
    if taken.insert(base.clone()) {
        return base;
    }
    // A second task that would take the same name keeps both: it is told apart by the source it
    // came from, and by a counter if even that is taken.
    for index in 0.. {
        let candidate = match index {
            0 => format!("{base}-{}", source.slug()),
            _ => format!("{base}-{}-{index}", source.slug()),
        };
        if taken.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("names are finite, so one of these is always free")
}

/// Save the selected candidates. The folder is rescanned first, so what reaches the database is
/// text this module produced rather than text the caller echoed back.
///
/// With `all`, everything the scan offers is imported; otherwise only the named candidates.
pub fn import(
    path: &str,
    names: &[String],
    all: bool,
    repo: &Repository,
) -> AppResult<ImportOutcome> {
    let scanned = scan(path, repo)?;
    let wanted: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let mut outcome = ImportOutcome::default();
    let mut matched = 0usize;

    for candidate in &scanned.candidates {
        if !(all || wanted.contains(candidate.name.as_str())) {
            continue;
        }
        matched += 1;
        if candidate.already_imported {
            outcome.duplicates += 1;
            continue;
        }
        let mut draft = EntryDraft {
            kind: Kind::Command,
            id: None,
            expected_revision: 0,
            name: candidate.name.clone(),
            entry_type: candidate.entry_type.clone(),
            // The project tag is what makes "everything from this repository" one click in the
            // library's tag filter.
            tags: vec![format!("project:{}", project_tag(&scanned.project))],
            description: candidate.description.clone(),
            notes: String::new(),
            fields: Vec::new(),
            pinned: false,
            command: candidate.command.clone(),
            working_directory: scanned.dir.clone(),
            value: String::new(),
        };
        match repo.save_command(&mut draft) {
            Ok(_) => outcome.imported += 1,
            Err(error) => outcome
                .skipped
                .push(format!("{} ({error})", candidate.name)),
        }
    }

    if matched == 0 {
        return Err(AppError::Validation(if names.is_empty() {
            "select at least one task to import".into()
        } else {
            "none of the selected tasks are in this folder any more; scan it again".into()
        }));
    }
    if !all {
        for name in wanted {
            if !scanned
                .candidates
                .iter()
                .any(|candidate| candidate.name == name)
            {
                outcome
                    .skipped
                    .push(format!("{name} (no longer in this folder; scan it again)"));
            }
        }
    }
    Ok(outcome)
}

/// Everything a saved command could match a candidate on: exact text plus working directory.
fn saved_executables(repo: &Repository) -> AppResult<BTreeSet<(String, String)>> {
    let mut out = BTreeSet::new();
    for (record, _, _) in repo.all_command_records()? {
        out.insert((record.command, record.working_directory));
    }
    Ok(out)
}

/// Resolve and validate the folder the importer was pointed at.
fn target_dir(path: &str) -> AppResult<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("choose a project folder first".into()));
    }
    let expanded = if trimmed == "~" || trimmed.starts_with("~/") {
        let home = std::env::var_os("HOME").ok_or_else(|| {
            AppError::Validation("HOME is not set; enter an absolute folder path".into())
        })?;
        PathBuf::from(home).join(trimmed.strip_prefix("~/").unwrap_or(""))
    } else {
        PathBuf::from(trimmed)
    };
    let resolved = std::fs::canonicalize(expanded)
        .map_err(|e| AppError::Validation(format!("cannot open {trimmed}: {e}")))?;
    if !resolved.is_dir() {
        return Err(AppError::Validation(format!(
            "{trimmed} is a file, not a project folder"
        )));
    }
    Ok(resolved)
}

/// Repository name when the folder sits inside a git checkout, otherwise the folder's own name.
/// A monorepo subfolder and its root therefore share one name, one prefix and one tag.
fn project_name(dir: &Path) -> String {
    let mut cursor = Some(dir);
    while let Some(current) = cursor {
        if current.join(".git").exists() {
            if let Some(name) = current.file_name().and_then(|n| n.to_str()) {
                return name.to_string();
            }
            break;
        }
        cursor = current.parent();
    }
    dir.file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| dir.to_string_lossy().into_owned())
}

/// Tag-safe form of the project name. `normalise_tags` lowercases and caps a tag at 40 characters,
/// so the value is built to fit rather than being silently truncated on save.
fn project_tag(project: &str) -> String {
    project
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_whitespace() { '-' } else { c })
        .take(32)
        .collect()
}

fn clip(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_DESCRIPTION_CHARS {
        return trimmed.to_string();
    }
    let mut clipped: String = trimmed.chars().take(MAX_DESCRIPTION_CHARS - 1).collect();
    clipped.push('…');
    clipped
}

// ---------------------------------------------------------------------------
// filesystem reads
// ---------------------------------------------------------------------------

/// Read one definition file. `Ok(None)` means "nothing usable here": absent, a directory, too
/// large to be a task definition, or not text at all (a UTF-16 `package.json` and a binary blob
/// both land there). A real read failure is reported rather than looking like an empty project.
fn read_definition(dir: &Path, name: &str) -> AppResult<Option<String>> {
    let path = dir.join(name);
    let meta = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(AppError::Other(format!("cannot read {name}: {e}"))),
    };
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Ok(None);
    }
    let mut raw = String::new();
    match std::fs::File::open(&path)?.read_to_string(&mut raw) {
        Ok(_) => Ok(Some(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok(None),
        Err(e) => Err(AppError::Other(format!("cannot read {name}: {e}"))),
    }
}

/// First file in `names` that exists and reads, as `(name, contents)`.
fn first_definition(dir: &Path, names: &[&str]) -> AppResult<Option<(String, String)>> {
    for name in names {
        if let Some(raw) = read_definition(dir, name)? {
            return Ok(Some(((*name).to_string(), raw)));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// package.json
// ---------------------------------------------------------------------------

/// Package manager implied by the folder's lockfile. Each one runs scripts its own way and prints
/// progress in its own words, so the command uses the one the project actually uses.
fn package_manager(dir: &Path) -> &'static str {
    for (lockfile, manager) in [
        ("bun.lockb", "bun"),
        ("bun.lock", "bun"),
        ("pnpm-lock.yaml", "pnpm"),
    ] {
        if dir.join(lockfile).is_file() {
            return manager;
        }
    }
    if dir.join("yarn.lock").is_file() {
        return "yarn";
    }
    "npm"
}

/// `<pm> run <script>` for every script except npm's own lifecycle hooks.
fn package_json_tasks(dir: &Path) -> AppResult<Vec<Task>> {
    let Some(raw) = read_definition(dir, PACKAGE_JSON)? else {
        return Ok(Vec::new());
    };
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|_| AppError::Validation(format!("{PACKAGE_JSON} is not valid JSON")))?;
    let Some(scripts) = parsed.get("scripts").and_then(Value::as_object) else {
        return Ok(Vec::new());
    };
    let manager = package_manager(dir);
    let named: BTreeSet<&String> = scripts.keys().collect();
    let mut out = Vec::new();
    for (name, body) in scripts {
        let Some(body) = body.as_str() else { continue };
        if is_lifecycle_hook(name, &named) {
            continue;
        }
        out.push(task(
            Source::Npm,
            manager,
            name.clone(),
            format!("{manager} run {name}"),
            format!("package.json script: {body}"),
        ));
    }
    Ok(out)
}

/// `pretest`/`posttest` next to a `test` script are npm's before/after hooks, not tasks anyone
/// types. `prepare` has no matching `are` script and stays: it is a real, runnable thing.
fn is_lifecycle_hook(name: &str, named: &BTreeSet<&String>) -> bool {
    for prefix in ["pre", "post"] {
        let Some(rest) = name.strip_prefix(prefix) else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let mut lower_first = rest.to_string();
        if let Some(head) = lower_first.get_mut(..1) {
            head.make_ascii_lowercase();
        }
        if named.iter().any(|n| **n == rest || **n == lower_first) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Make
// ---------------------------------------------------------------------------

fn makefile_tasks(dir: &Path) -> AppResult<Vec<Task>> {
    let Some((_, raw)) = first_definition(dir, &MAKEFILES)? else {
        return Ok(Vec::new());
    };
    Ok(parse_make_targets(&raw)
        .into_iter()
        .map(|(target, help)| {
            task(
                Source::Make,
                "make",
                target.clone(),
                format!("make {target}"),
                help,
            )
        })
        .collect())
}

/// Line-oriented Make reader: every plain rule becomes a task.
///
/// Deliberately conservative — a pattern rule (`%.o: %.c`), a variable assignment
/// (`CFLAGS := -O2`), a conditional, an `include`, a target built from a variable expansion, or a
/// continued line is skipped rather than guessed at. A missed task is a shrug; a wrong task name,
/// or a command that expands to nothing, is a broken entry in someone's library.
///
/// A `## comment` on the same line is the target's help text (`make` itself ignores it).
fn parse_make_targets(raw: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut define_depth = 0usize;
    let mut continued = false;
    for line in raw.lines() {
        let was_continued = continued;
        continued = line.trim_end().ends_with('\\');
        if was_continued {
            continue;
        }
        let directive = line.trim_start();
        if !line.starts_with('\t') {
            // Make allows modifiers before define and nested variable definitions.
            let words: Vec<_> = directive.split_whitespace().collect();
            let head = words
                .iter()
                .find(|word| !matches!(**word, "override" | "export" | "private"));
            if head == Some(&"define") {
                define_depth += 1;
                continue;
            }
            if head == Some(&"endef") {
                define_depth = define_depth.saturating_sub(1);
                continue;
            }
        }
        if define_depth > 0 {
            continue;
        }
        // Blank, comment, and anything indented: recipe bodies, continuation lines and bodies of
        // nested conditionals all start with whitespace.
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with(char::is_whitespace)
            || line.ends_with('\\')
        {
            continue;
        }
        let (code, comment) = match line.find('#') {
            Some(at) => (&line[..at], line[at + 1..].trim_start_matches('#').trim()),
            None => (line, ""),
        };
        let Some(colon) = code.find(':') else {
            continue;
        };
        let names = &code[..colon];
        // `CFLAGS := -O2` and `PATH ::= /usr/bin`: the colon belongs to an assignment operator,
        // so there is no rule here at all.
        if matches!(code[colon + 1..].chars().next(), Some('=') | Some(':'))
            || names.contains('=')
            || names.trim().is_empty()
        {
            continue;
        }
        // `-include foo.mk` has nothing to build.
        if names.trim_start().starts_with('-') {
            continue;
        }
        for name in names.split_whitespace() {
            // Pattern rules, variable expansions and special targets are not importable tasks.
            if name.contains('%')
                || name.contains('$')
                || name.contains(':')
                || name.starts_with('.')
            {
                continue;
            }
            if !out.iter().any(|(target, _)| target == name) {
                out.push((name.to_string(), comment.to_string()));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// just
// ---------------------------------------------------------------------------

fn justfile_tasks(dir: &Path) -> AppResult<Vec<Task>> {
    let Some((_, raw)) = first_definition(dir, &JUSTFILES)? else {
        return Ok(Vec::new());
    };
    // Prefer just's own view of the file: it understands parameters, `[doc]`, imports and aliases.
    // When `just` is absent, fails, or answers something unrecognisable, the line parser below
    // still gives a complete answer for a plain justfile.
    let recipes = just_dump(dir)
        .as_deref()
        .and_then(parse_just_dump)
        .unwrap_or_else(|| parse_just_recipes(&raw));
    Ok(recipes
        .into_iter()
        .map(|(recipe, doc)| {
            task(
                Source::Just,
                "just",
                recipe.clone(),
                format!("just {recipe}"),
                doc,
            )
        })
        .collect())
}

/// `just --dump --dump-format json`, or `None` when `just` is unavailable or unhelpful.
///
/// This is the one place the module starts a process, so: an argument array (never a shell
/// string), a timeout that kills the child, a capped read, and output that is only ever parsed.
fn just_dump(dir: &Path) -> Option<String> {
    if !executable_in_path("just") {
        return None;
    }
    dump_output(
        Command::new("just")
            .args(["--dump", "--dump-format", "json"])
            .current_dir(dir),
        JUST_DUMP_TIMEOUT,
    )
}

/// One deadline covers both stdout and exit. A process can close stdout before it exits,
/// or exit while a descendant still owns the pipe, so neither event alone completes the dump.
fn dump_output(command: &mut Command, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    // Nonblocking reads let the same deadline cover a pipe held open by descendants,
    // without leaving a blocked reader thread behind when the process is stopped.
    let fd = stdout.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    let configured =
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0;
    let mut output = Vec::new();
    let mut status = None;
    let mut eof = false;
    let mut chunk = [0u8; 65536];
    let result = loop {
        if !configured || Instant::now() >= deadline {
            break None;
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(Some(exit)) if exit.success() => status = Some(exit),
                Ok(Some(_)) | Err(_) => break None,
                Ok(None) => {}
            }
        }
        let mut read_bytes = false;
        if !eof {
            match stdout.read(&mut chunk) {
                Ok(0) => eof = true,
                Ok(count) => {
                    if output.len() + count > MAX_FILE_BYTES as usize * 4 {
                        break None;
                    }
                    output.extend_from_slice(&chunk[..count]);
                    read_bytes = true;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break None,
            }
        }
        if status.is_some() && eof {
            break String::from_utf8(output).ok();
        }
        if !read_bytes {
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    // Stop descendants too, including a child holding stdout open after its parent exits.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.wait();
    result
}

fn executable_in_path(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        std::fs::metadata(&candidate)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

/// One `(recipe, doc)` per public recipe of a `just --dump` payload.
///
/// The dump has carried the recipes both as a top-level map and under a `recipes` key, so both are
/// accepted. Anything that looks like neither returns `None`, which falls back to the line parser
/// rather than importing a plausible-but-wrong list.
fn parse_just_dump(raw: &str) -> Option<Vec<(String, String)>> {
    let value: Value = serde_json::from_str(raw.trim()).ok()?;
    let map = match value.get("recipes").and_then(Value::as_object) {
        Some(recipes) => recipes.clone(),
        None => value.as_object()?.clone(),
    };
    let mut out = Vec::new();
    for (key, recipe) in &map {
        let fields = recipe.as_object()?;
        let name = fields
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(key.as_str())
            .to_string();
        if name.is_empty()
            || name.starts_with('_')
            || fields.get("private").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        out.push((
            name,
            fields
                .get("doc")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ));
    }
    Some(out)
}

/// Recipe headers read straight off the justfile text. Same conservatism as the Make reader: a
/// recipe name, and nothing else.
///
/// Skipped: `_private` recipes, `[private]` recipes, assignments, `set`/`export` directives,
/// attribute lines, and every indented line (a body, a continuation, or a comment inside one).
/// The comment line directly above a header is its documentation; the rest of a comment block is
/// dropped, which is exactly the case `just --dump` covers.
fn parse_just_recipes(raw: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut doc = String::new();
    let mut private = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            // A blank line ends a documentation block, so an unrelated comment far above a recipe
            // is never attached to it.
            doc.clear();
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            doc.clear();
            private = false;
            continue;
        }
        if trimmed.starts_with('#') {
            doc = trimmed.trim_start_matches('#').trim().to_string();
            continue;
        }
        if trimmed.starts_with('[') {
            private |= trimmed
                .trim_matches(['[', ']'])
                .split(',')
                .any(|attribute| attribute.trim() == "private");
            continue;
        }
        let owned_doc = std::mem::take(&mut doc);
        let is_private = std::mem::take(&mut private);
        let Some(name) = just_recipe_name(trimmed) else {
            continue;
        };
        if is_private || name.starts_with('_') {
            continue;
        }
        if out.iter().any(|(existing, _)| existing == name) {
            continue;
        }
        out.push((name.to_string(), owned_doc));
    }
    out
}

/// `name` from a recipe header (`serve host="0.0.0.0" *args:`), or `None` when the line is a
/// directive or an assignment rather than a recipe.
fn just_recipe_name(line: &str) -> Option<&str> {
    for directive in ["set ", "export ", "alias ", "import ", "mod ", "os "] {
        if line.starts_with(directive) {
            return None;
        }
    }
    // The header's colon closes the parameter list, so it is the *last* one on the line: a
    // parameter default such as `host="0.0.0.0"` contains earlier ones.
    let colon = line.rfind(':')?;
    let end = line.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))?;
    let name = &line[..end];
    if name.is_empty() || name.starts_with(char::is_numeric) || colon < end {
        return None;
    }
    // `FOO := bar` assigns a variable; `serve host="0.0.0.0":` declares a parameter default. The
    // difference is the whitespace around the equals sign.
    if is_assignment(&line[end..colon]) || line[colon + 1..].starts_with('=') {
        return None;
    }
    Some(name)
}

/// `name := value` (or `name = value`) rather than a recipe's `param=default` list.
fn is_assignment(parameters: &str) -> bool {
    if parameters.contains(":=") {
        return true;
    }
    parameters.char_indices().any(|(at, c)| {
        if c != '=' {
            return false;
        }
        let before = parameters[..at].chars().next_back();
        let after = parameters[at + 1..].chars().next();
        before.is_none_or(|c| c.is_whitespace()) || after.is_none_or(|c| c.is_whitespace())
    })
}

// ---------------------------------------------------------------------------
// Docker Compose
// ---------------------------------------------------------------------------

fn compose_tasks(dir: &Path) -> AppResult<Vec<Task>> {
    let Some((file, raw)) = first_definition(dir, &COMPOSE_FILES)? else {
        return Ok(Vec::new());
    };
    let services = parse_compose_services(&raw, &file)?;
    let mut out = Vec::new();
    for service in services {
        out.push(task(
            Source::Compose,
            "compose",
            format!("up-{service}"),
            format!("docker compose up -d {service}"),
            format!("Start {service} in the background"),
        ));
        out.push(task(
            Source::Compose,
            "compose",
            format!("logs-{service}"),
            format!("docker compose logs -f {service}"),
            format!("Follow the logs of {service}"),
        ));
    }
    if !out.is_empty() {
        out.push(task(
            Source::Compose,
            "compose",
            "down".to_string(),
            "docker compose down".to_string(),
            "Stop this compose project".to_string(),
        ));
    }
    Ok(out)
}

/// Validate the complete YAML document before extracting service names. Nested syntax errors
/// must not turn into plausible commands that Docker Compose will later refuse to run.
fn parse_compose_services(raw: &str, file: &str) -> AppResult<Vec<String>> {
    let invalid = |message: String| AppError::Validation(format!("{file}: {message}"));
    let document: serde_norway::Value =
        serde_norway::from_str(raw).map_err(|error| invalid(format!("invalid YAML: {error}")))?;
    if document.is_null() {
        return Ok(Vec::new());
    }
    let root = document
        .as_mapping()
        .ok_or_else(|| invalid("expected a mapping".into()))?;
    let Some(services) = root.get("services") else {
        return Ok(Vec::new());
    };
    let services = services
        .as_mapping()
        .ok_or_else(|| invalid("services must be a mapping of service names".into()))?;
    let mut out = Vec::new();
    for (name, definition) in services {
        let name = name
            .as_str()
            .ok_or_else(|| invalid("service names must be strings".into()))?;
        if !name.starts_with(|c: char| c.is_ascii_alphanumeric())
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Err(invalid(format!("invalid service name: {name}")));
        }
        if !definition.is_mapping() {
            return Err(invalid(format!("service {name} must be a mapping")));
        }
        out.push(name.to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
