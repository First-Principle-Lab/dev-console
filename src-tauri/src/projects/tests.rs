use super::*;

/// A project folder named `tag` holding `files`, plus the repository candidates are matched
/// against. Every scan and import test runs through this, so the dedupe key is always a real
/// database row rather than a hand-written tuple.
struct Harness {
    repo: Repository,
    project: PathBuf,
    /// Declared last so it is dropped last: the database must close before its folder vanishes.
    _root: tempfile::TempDir,
}

impl Harness {
    fn new(tag: &str, files: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let repo = Repository::open(&root.path().join("commands.sqlite3")).expect("repository");
        let project = root.path().join(tag);
        std::fs::create_dir_all(&project).expect("project folder");
        write_files(&project, files);
        pin_name(&project);
        Harness {
            project: std::fs::canonicalize(&project).expect("canonical"),
            repo,
            _root: root,
        }
    }

    /// A second folder beside this one, sharing the repository (for "another checkout, same
    /// commands" cases).
    fn sibling(&self, tag: &str, files: &[(&str, &str)]) -> PathBuf {
        let project = self.root_path().join(tag);
        std::fs::create_dir_all(&project).expect("sibling folder");
        write_files(&project, files);
        pin_name(&project);
        std::fs::canonicalize(&project).expect("canonical")
    }

    fn root_path(&self) -> &Path {
        self._root.path()
    }

    fn name(&self) -> &str {
        self.project.file_name().unwrap().to_str().unwrap()
    }

    fn path(&self) -> &str {
        self.project.to_str().unwrap()
    }

    fn scan(&self) -> ProjectScan {
        scan(self.path(), &self.repo).expect("scan")
    }

    fn import(&self, names: &[&str], all: bool) -> AppResult<ImportOutcome> {
        let owned: Vec<String> = names.iter().map(|n| n.to_string()).collect();
        import(self.path(), &owned, all, &self.repo)
    }

    fn saved(&self) -> Vec<crate::models::EntryRecord> {
        self.repo
            .all_command_records()
            .expect("records")
            .into_iter()
            .map(|(record, _, _)| record)
            .collect()
    }
}

fn write_files(dir: &Path, files: &[(&str, &str)]) {
    for (name, contents) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, contents).expect("write");
    }
}

/// `project_name` reports the git root, and a temporary folder is normally outside any checkout.
/// Where a runner keeps TMPDIR inside a repository, every test would inherit that repo's name, so
/// the folder is made a repository root of its own to keep candidate names predictable.
fn pin_name(dir: &Path) {
    let mut cursor = Some(dir);
    while let Some(current) = cursor {
        if current.join(".git").exists() {
            std::fs::create_dir_all(dir.join(".git")).expect("pin .git");
            return;
        }
        cursor = current.parent();
    }
}

fn save(repo: &Repository, name: &str, command: &str, working_directory: &str) {
    let mut draft = EntryDraft {
        kind: Kind::Command,
        name: name.to_string(),
        entry_type: "Shell".to_string(),
        command: command.to_string(),
        working_directory: working_directory.to_string(),
        ..EntryDraft::default()
    };
    repo.save_command(&mut draft).expect("save command");
}

fn names(scan: &ProjectScan) -> Vec<&str> {
    scan.candidates.iter().map(|c| c.name.as_str()).collect()
}

fn command_of<'a>(scan: &'a ProjectScan, task: &str) -> &'a str {
    scan.candidates
        .iter()
        .find(|c| c.name.ends_with(task))
        .unwrap_or_else(|| panic!("no candidate ending in {task}: {:?}", names(scan)))
        .command
        .as_str()
}

// ---------------------------------------------------------------------------
// package.json
// ---------------------------------------------------------------------------

#[test]
fn reads_scripts_and_skips_npm_lifecycle_hooks() {
    let h = Harness::new(
        "hooks",
        &[(
            PACKAGE_JSON,
            r#"{ "scripts": {
                "build": "vite build",
                "prebuild": "rm -rf dist",
                "posttest": "echo done",
                "test": "vitest run",
                "prepare": "husky install",
                "coverage": 42
            } }"#,
        )],
    );
    let scan = h.scan();
    // `prebuild`/`posttest` are npm's own before/after hooks and `coverage` is not a command.
    assert_eq!(
        names(&scan),
        vec!["hooks·build", "hooks·prepare", "hooks·test"]
    );
    assert_eq!(command_of(&scan, "test"), "npm run test");
    assert_eq!(command_of(&scan, "build"), "npm run build");
    assert_eq!(
        scan.candidates[0].description,
        "package.json script: vite build"
    );
}

#[test]
fn every_lockfile_selects_its_own_package_manager() {
    let manifest = (PACKAGE_JSON, r#"{ "scripts": { "dev": "vite" } }"#);
    for (lockfile, expected) in [
        ("bun.lock", "bun run dev"),
        ("bun.lockb", "bun run dev"),
        ("pnpm-lock.yaml", "pnpm run dev"),
        ("yarn.lock", "yarn run dev"),
        ("package-lock.json", "npm run dev"),
        // No lockfile at all is npm's own default.
        ("README.md", "npm run dev"),
    ] {
        let h = Harness::new("pm", &[manifest, (lockfile, "")]);
        let scan = h.scan();
        assert_eq!(
            scan.candidates[0].command, expected,
            "{lockfile} should mean {expected}"
        );
        assert_eq!(
            scan.candidates[0].entry_type,
            expected.split(' ').next().unwrap(),
            "{lockfile}"
        );
    }
}

#[test]
fn a_package_json_that_is_not_json_is_a_validation_error() {
    let h = Harness::new("broken", &[(PACKAGE_JSON, "{ this is not json")]);
    let error = scan(h.path(), &h.repo).unwrap_err();
    assert_eq!(error.code(), "validation");
    assert!(error.to_string().contains(PACKAGE_JSON), "{error}");
}

#[test]
fn a_package_without_scripts_and_a_binary_manifest_offer_nothing() {
    let plain = Harness::new("plain", &[(PACKAGE_JSON, r#"{ "name": "lib" }"#)]);
    assert!(plain.scan().candidates.is_empty());

    // A manifest that is not text at all is "nothing readable", not a broken project.
    let binary = Harness::new("binary", &[]);
    std::fs::write(
        binary.project.join(PACKAGE_JSON),
        [0xff_u8, 0xfe, 0x01, 0x41],
    )
    .expect("write binary");
    assert!(scan(binary.path(), &binary.repo)
        .expect("scan")
        .candidates
        .is_empty());
}

#[test]
fn a_definition_file_over_the_size_cap_is_ignored_not_read() {
    let h = Harness::new("huge", &[]);
    std::fs::write(h.project.join(PACKAGE_JSON), vec![b' '; 2 * 1024 * 1024]).expect("write");
    assert!(scan(h.path(), &h.repo).expect("scan").candidates.is_empty());
}

// ---------------------------------------------------------------------------
// Make
// ---------------------------------------------------------------------------

const MAKEFILE: &str = "\
# A comment line.
CFLAGS := -O2
-include local.mk

.PHONY: all build
.DEFAULT_GOAL := all

all: build ## Build everything
build: $(SRC) ## Compile the binary
%.o: %.c
test: ## Run the tests
lint format:
deploy-$(ENV):
clean:
\t-rm -rf target
release: ## Ship it
";

#[test]
fn make_targets_are_discovered_with_their_help_text() {
    let parsed = parse_make_targets(MAKEFILE);
    let targets: Vec<&str> = parsed.iter().map(|(t, _)| t.as_str()).collect();
    // No pattern rule, no special target, no variable expansion, no assignment, no recipe body.
    assert_eq!(
        targets,
        vec!["all", "build", "test", "lint", "format", "clean", "release"]
    );
    let described: Vec<(&str, &str)> = parsed
        .iter()
        .map(|(t, d)| (t.as_str(), d.as_str()))
        .collect();
    assert!(
        described.contains(&("all", "Build everything")),
        "{described:?}"
    );
    assert!(described.contains(&("release", "Ship it")), "{described:?}");
    assert!(described.contains(&("clean", "")), "{described:?}");
}

#[test]
fn a_make_project_is_commanded_with_make() {
    let h = Harness::new("api", &[("Makefile", MAKEFILE)]);
    let scan = h.scan();
    assert_eq!(command_of(&scan, "build"), "make build");
    assert_eq!(command_of(&scan, "clean"), "make clean");
    assert!(scan
        .candidates
        .iter()
        .all(|c| c.source == Source::Make && c.entry_type == "make"));
    assert_eq!(
        scan.candidates
            .iter()
            .find(|c| c.name.ends_with("test"))
            .expect("test target")
            .description,
        "Run the tests"
    );
}

#[test]
fn every_makefile_spelling_is_read() {
    for file in MAKEFILES {
        let h = Harness::new("mk", &[(file, "serve:\n\tnode .\n")]);
        assert_eq!(command_of(&h.scan(), "serve"), "make serve", "{file}");
    }
}

// ---------------------------------------------------------------------------
// just
// ---------------------------------------------------------------------------

const JUSTFILE: &str = "\
set shell := [\"bash\", \"-c\"]
default := \"dev\"
export RUST_BACKTRACE := \"1\"

[private]
secret-thing:
    echo hush

_internal:
    echo nope

# Run the test suite
test *args:
    cargo test {{ args }}

# Start the server
serve host=\"0.0.0.0\":
    cargo run

deploy:
\t./deploy.sh
";

#[test]
fn just_recipes_survive_parameters_privacy_and_directives() {
    let parsed = parse_just_recipes(JUSTFILE);
    let names: Vec<&str> = parsed.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["test", "serve", "deploy"]);
    assert_eq!(parsed[0].1, "Run the test suite");
    assert_eq!(parsed[2].1, "");
}

#[test]
fn a_justfile_in_either_spelling_is_found() {
    for file in JUSTFILES {
        let h = Harness::new("just", &[(file, JUSTFILE)]);
        assert_eq!(command_of(&h.scan(), "serve"), "just serve", "{file}");
    }
}

#[test]
fn a_just_dump_is_preferred_and_its_private_recipes_are_dropped() {
    let dump = r#"{ "recipes": {
        "test": { "name": "test", "doc": "Run the tests", "private": false },
        "_hidden": { "name": "_hidden", "doc": null, "private": false },
        "internal": { "name": "internal", "doc": null, "private": true }
    } }"#;
    assert_eq!(
        parse_just_dump(dump).expect("parses"),
        vec![("test".to_string(), "Run the tests".to_string())]
    );

    // The bare-map shape works too; anything that is neither shape falls back to the line parser.
    assert_eq!(
        parse_just_dump(r#"{ "build": { "doc": "Build it" } }"#).expect("bare map"),
        vec![("build".to_string(), "Build it".to_string())]
    );
    assert!(parse_just_dump("not json at all").is_none());
    assert!(parse_just_dump("[1,2,3]").is_none());
}

// ---------------------------------------------------------------------------
// Docker Compose
// ---------------------------------------------------------------------------

const COMPOSE: &str = "\
services:
  api:
    build: .
    ports:
      - \"8080:8080\"
  worker: # background jobs
    image: redis
volumes:
  data:
";

#[test]
fn compose_services_become_up_logs_and_one_down() {
    assert_eq!(
        parse_compose_services(COMPOSE, "compose.yaml").expect("services"),
        vec!["api", "worker"]
    );

    let h = Harness::new("stack", &[("compose.yaml", COMPOSE)]);
    let scan = h.scan();
    assert_eq!(
        names(&scan),
        vec![
            "stack·down",
            "stack·logs-api",
            "stack·logs-worker",
            "stack·up-api",
            "stack·up-worker",
        ]
    );
    assert_eq!(command_of(&scan, "up-api"), "docker compose up -d api");
    assert_eq!(
        command_of(&scan, "logs-worker"),
        "docker compose logs -f worker"
    );
    assert_eq!(command_of(&scan, "down"), "docker compose down");
}

#[test]
fn every_compose_spelling_is_read() {
    for file in COMPOSE_FILES {
        let h = Harness::new("compose", &[(file, COMPOSE)]);
        assert_eq!(h.scan().candidates.len(), 5, "{file}");
    }
}

#[test]
fn malformed_compose_is_a_validation_error_never_a_panic() {
    let h = Harness::new("malformed", &[]);
    for (case, body) in [
        (
            "a child with no colon",
            "services:\n  api\n    image: redis\n",
        ),
        (
            "a service declared inline",
            "services:\n  api: image: redis\n",
        ),
        ("a list instead of a mapping", "services:\n  - api\n"),
        ("an empty service name", "services:\n  :\n"),
    ] {
        std::fs::write(h.project.join("compose.yaml"), body).expect("write");
        let error = scan(h.path(), &h.repo)
            .err()
            .unwrap_or_else(|| panic!("{case} should be reported"));
        assert_eq!(error.code(), "validation", "{case}");
        assert!(
            error.to_string().contains("compose.yaml"),
            "{case}: {error}"
        );
    }
}

#[test]
fn a_compose_file_without_services_offers_nothing() {
    let h = Harness::new("volumes", &[("compose.yaml", "volumes:\n  data:\n")]);
    assert!(h.scan().candidates.is_empty());
}

// ---------------------------------------------------------------------------
// naming, grouping and the folder itself
// ---------------------------------------------------------------------------

#[test]
fn sources_group_and_sort_in_a_stable_order() {
    let h = Harness::new(
        "everything",
        &[
            (
                PACKAGE_JSON,
                r#"{ "scripts": { "zebra": "z", "alpha": "a" } }"#,
            ),
            ("Makefile", "omega:\nalpha:\n"),
            ("justfile", "zulu:\nabraco:\n"),
            ("compose.yaml", "services:\n  zeta:\n    image: busybox\n"),
        ],
    );
    let grouped = h.scan().candidates.into_iter().fold(
        Vec::<(Source, Vec<String>)>::new(),
        |mut groups, c| {
            match groups.last_mut() {
                Some((source, names)) if *source == c.source => names.push(c.name),
                _ => groups.push((c.source, vec![c.name])),
            }
            groups
        },
    );
    assert_eq!(
        grouped.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
        vec![Source::Npm, Source::Make, Source::Just, Source::Compose]
    );
    // Names sort inside a group, so the dialog order never depends on file read order.
    assert_eq!(grouped[0].1, vec!["everything·alpha", "everything·zebra"]);
    // Only the task that clashes with a script name is told apart by its source.
    assert_eq!(
        grouped[1].1,
        vec!["everything·alpha-make", "everything·omega"]
    );
    assert_eq!(grouped[2].1, vec!["everything·abraco", "everything·zulu"]);
    assert_eq!(
        grouped[3].1,
        vec![
            "everything·down",
            "everything·logs-zeta",
            "everything·up-zeta"
        ]
    );
}

#[test]
fn two_sources_naming_the_same_task_both_survive() {
    let h = Harness::new(
        "clash",
        &[
            (PACKAGE_JSON, r#"{ "scripts": { "build": "vite build" } }"#),
            ("Makefile", "build:\n\tcargo build\n"),
        ],
    );
    let found = h.scan();
    let mut got = names(&found);
    got.sort_unstable();
    // One keeps the plain name, the other is told apart by its source rather than dropped.
    assert_eq!(got, vec!["clash·build", "clash·build-make"]);
}

#[test]
fn the_folder_is_named_after_its_repository_root_when_there_is_one() {
    let root = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(root.path().join(".git")).expect(".git");
    std::fs::create_dir_all(root.path().join("packages/web")).expect("subfolder");
    std::fs::write(
        root.path().join("packages/web/package.json"),
        r#"{ "scripts": { "dev": "vite" } }"#,
    )
    .expect("write");

    let (store, dir) = (
        tempfile::tempdir().expect("store"),
        root.path().join("packages/web"),
    );
    let repo = Repository::open(&store.path().join("commands.sqlite3")).expect("repository");
    let web = std::fs::canonicalize(&dir).expect("canonical");
    let scan = scan(web.to_str().unwrap(), &repo).expect("scan");
    // A monorepo subfolder imports as the repository, not as `web`.
    assert_eq!(
        scan.project,
        std::fs::canonicalize(root.path())
            .expect("canonical root")
            .file_name()
            .unwrap()
            .to_string_lossy()
    );
    assert_eq!(names(&scan), vec![format!("{}·dev", scan.project).as_str()]);
}

#[test]
fn the_folder_name_is_used_when_nothing_above_is_a_repository() {
    let h = Harness::new(
        "flat",
        &[(PACKAGE_JSON, r#"{ "scripts": { "dev": "vite" } }"#)],
    );
    let scan = h.scan();
    assert_eq!(scan.project, h.name());
    assert_eq!(names(&scan), vec!["flat·dev"]);
    assert_eq!(scan.dir, h.path());
}

#[test]
fn a_file_and_a_missing_folder_are_reported_not_scanned() {
    let h = Harness::new("nothing", &[]);
    let file = h.root_path().join("notes.txt");
    std::fs::write(&file, "not a project").expect("write");

    for case in [
        file.to_str().unwrap().to_string(),
        "   ".to_string(),
        "/definitely/not/here-40e1c".to_string(),
    ] {
        let error = scan(&case, &h.repo).unwrap_err();
        assert_eq!(error.code(), "validation", "{case}");
    }
}

// ---------------------------------------------------------------------------
// import and dedupe
// ---------------------------------------------------------------------------

#[test]
fn an_imported_task_is_recognised_on_rescan_by_command_and_folder() {
    let h = Harness::new(
        "api",
        &[(PACKAGE_JSON, r#"{ "scripts": { "test": "vitest run" } }"#)],
    );
    // The same command in a different folder is a different task: the folder is half the identity.
    save(&h.repo, "elsewhere·test", "npm run test", "/somewhere/else");
    assert_eq!(
        h.scan()
            .candidates
            .iter()
            .filter(|c| c.already_imported)
            .count(),
        0
    );

    save(&h.repo, "api·test", "npm run test", h.path());
    let scan = h.scan();
    assert_eq!(scan.candidates.len(), 1);
    assert!(scan.candidates[0].already_imported);
}

#[test]
fn importing_saves_the_command_with_its_folder_tag_and_type() {
    let h = Harness::new(
        "web",
        &[
            ("bun.lock", ""),
            (
                PACKAGE_JSON,
                r#"{ "scripts": { "dev": "vite", "ship": "rsync -a dist/ host:" } }"#,
            ),
        ],
    );
    let outcome = h.import(&["web·dev"], false).expect("import");
    assert_eq!(
        (outcome.imported, outcome.duplicates, outcome.skipped.len()),
        (1, 0, 0)
    );

    let saved = h
        .saved()
        .into_iter()
        .find(|r| r.name.ends_with("·dev"))
        .expect("saved command");
    assert_eq!(saved.command, "bun run dev");
    assert_eq!(saved.entry_type, "bun");
    assert_eq!(saved.tags, vec!["project:web"]);
    assert_eq!(saved.working_directory, h.path());
    assert_eq!(saved.description, "package.json script: vite");
}

#[test]
fn a_second_import_reports_nothing_new_and_never_duplicates() {
    let h = Harness::new(
        "twice",
        &[(PACKAGE_JSON, r#"{ "scripts": { "one": "a", "two": "b" } }"#)],
    );
    let first = h.import(&[], true).expect("import all");
    assert_eq!((first.imported, first.duplicates), (2, 0));

    let second = h.import(&[], true).expect("re-import");
    assert_eq!((second.imported, second.duplicates), (0, 2));
    assert_eq!(h.saved().len(), 2);
}

#[test]
fn a_task_the_user_already_wrote_by_hand_counts_as_saved() {
    let h = Harness::new(
        "hand",
        &[(PACKAGE_JSON, r#"{ "scripts": { "dev": "vite" } }"#)],
    );
    // Named entirely differently: only the command text and the folder can match.
    save(&h.repo, "my-dev-thing", "npm run dev", h.path());
    let outcome = h.import(&[], true).expect("import");
    assert_eq!((outcome.imported, outcome.duplicates), (0, 1));
    assert_eq!(h.saved().len(), 1);
}

#[test]
fn an_empty_selection_is_refused_and_a_stale_one_is_reported() {
    let h = Harness::new(
        "sel",
        &[(PACKAGE_JSON, r#"{ "scripts": { "dev": "vite" } }"#)],
    );
    let nothing = h.import(&[], false).unwrap_err();
    assert_eq!(nothing.code(), "validation");
    assert!(
        nothing.to_string().contains("select at least one"),
        "{nothing}"
    );

    let stale = h.import(&["sel·deleted"], false).unwrap_err();
    assert_eq!(stale.code(), "validation");
    assert!(stale.to_string().contains("scan it again"), "{stale}");
    assert!(h.saved().is_empty());
}

#[test]
fn only_the_named_tasks_are_imported() {
    let h = Harness::new(
        "picks",
        &[(
            PACKAGE_JSON,
            r#"{ "scripts": { "a": "1", "b": "2", "c": "3" } }"#,
        )],
    );
    let outcome = h.import(&["picks·a", "picks·c"], false).expect("import");
    assert_eq!((outcome.imported, outcome.duplicates), (2, 0));
    let saved: BTreeSet<String> = h.saved().into_iter().map(|r| r.name).collect();
    assert_eq!(
        saved,
        BTreeSet::from(["picks·a".to_string(), "picks·c".to_string()])
    );
}

#[test]
fn a_task_that_cannot_be_saved_skips_without_losing_the_rest() {
    // A name the validator rejects (too long) must not abort the whole import.
    let long = "x".repeat(300);
    let h = Harness::new("partial", &[("Makefile", &format!("{long}:\nplain:\n"))]);
    let outcome = h.import(&[], true).expect("import");
    assert_eq!(outcome.imported, 1);
    assert_eq!(outcome.skipped.len(), 1);
    assert!(
        outcome.skipped[0].contains("name is too long"),
        "{:?}",
        outcome.skipped
    );
    assert_eq!(h.saved()[0].name, "partial·plain");
}

#[test]
fn a_scan_of_a_second_folder_never_sees_the_first_one_saves() {
    let h = Harness::new(
        "two-folders",
        &[(PACKAGE_JSON, r#"{ "scripts": { "dev": "vite" } }"#)],
    );
    assert_eq!(h.import(&[], true).expect("import").imported, 1);

    let other = h.sibling(
        "other",
        &[(PACKAGE_JSON, r#"{ "scripts": { "dev": "vite" } }"#)],
    );
    let outcome = import(other.to_str().unwrap(), &[], true, &h.repo).expect("import second");
    // Same text, different folder: a new command, not a duplicate.
    assert_eq!((outcome.imported, outcome.duplicates), (1, 0));
    assert_eq!(h.saved().len(), 2);
}

#[test]
fn make_uses_the_same_file_precedence_as_the_runner() {
    let h = Harness::new(
        "priority",
        &[
            ("Makefile", "upper:\n"),
            ("makefile", "lower:\n"),
            ("GNUmakefile", "gnu:\n"),
        ],
    );
    assert_eq!(command_of(&h.scan(), "gnu"), "make gnu");
    assert_eq!(h.scan().candidates.len(), 1);
    std::fs::remove_file(h.project.join("GNUmakefile")).unwrap();
    assert_eq!(command_of(&h.scan(), "lower"), "make lower");
}

#[test]
fn make_skips_define_blocks_and_continued_lines() {
    let parsed = parse_make_targets(include_str!("../../tests/fixtures/projects/Makefile"));
    assert_eq!(parsed, vec![("real".into(), "Real target".into())]);
}

#[test]
fn compose_validates_nested_yaml_and_service_types() {
    for raw in [
        include_str!("../../tests/fixtures/projects/invalid-compose.yaml"),
        "services:\n  api: [redis]\n",
        "services:\n  api:\n    image: redis\n bad: indent\n",
        "services: {api: {image: redis}, api: {image: nginx}}",
        "services: {'api;echo': {image: redis}}",
    ] {
        assert!(
            parse_compose_services(raw, "compose.yaml").is_err(),
            "accepted {raw}"
        );
    }
    assert_eq!(
        parse_compose_services("---\nservices: {'api': {image: redis}}\n", "compose.yaml").unwrap(),
        vec!["api"]
    );
}

#[test]
fn tilde_folder_paths_expand_to_home() {
    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    assert_eq!(
        target_dir("~").unwrap(),
        std::fs::canonicalize(&home).unwrap()
    );
    let folder = tempfile::tempdir_in(&home).unwrap();
    let path = format!("~/{}", folder.path().file_name().unwrap().to_str().unwrap());
    assert_eq!(
        target_dir(&path).unwrap(),
        std::fs::canonicalize(folder.path()).unwrap()
    );
}

#[test]
fn partially_stale_selections_report_each_missing_task() {
    let h = Harness::new(
        "stale",
        &[(
            PACKAGE_JSON,
            include_str!("../../tests/fixtures/projects/package.json"),
        )],
    );
    std::fs::write(
        h.project.join(PACKAGE_JSON),
        r#"{"scripts":{"a":"printf a"}}"#,
    )
    .unwrap();
    let outcome = h.import(&["stale·a", "stale·b"], false).unwrap();
    assert_eq!(outcome.imported, 1);
    assert_eq!(outcome.skipped.len(), 1);
    assert!(outcome.skipped[0].contains("stale·b"));
}

#[test]
fn real_just_159_dump_fixture_has_the_expected_shape() {
    let recipes = parse_just_dump(include_str!(
        "../../tests/fixtures/projects/just-1.59.0.json"
    ))
    .unwrap();
    assert_eq!(recipes, vec![("public".into(), "Public recipe".into())]);
}

#[test]
fn just_dump_deadline_covers_exit_and_inherited_stdout() {
    for script in [
        "exec 1>&-; exec sleep 5",
        "sleep 5 & exit 0",
        "exec sleep 5",
    ] {
        let start = Instant::now();
        assert!(dump_output(
            Command::new("/bin/sh").args(["-c", script]),
            Duration::from_millis(200)
        )
        .is_none());
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "timeout failed for {script}"
        );
    }
}

#[test]
fn just_dump_rejects_failed_and_oversized_output() {
    assert!(dump_output(
        Command::new("/bin/sh").args(["-c", "printf '{}'; exit 1"]),
        Duration::from_secs(2)
    )
    .is_none());
    assert!(dump_output(
        Command::new("/bin/sh").args(["-c", "yes x"]),
        Duration::from_secs(2)
    )
    .is_none());
    assert_eq!(
        dump_output(
            Command::new("/bin/sh").args(["-c", "printf '{\"recipes\":{}}'"]),
            Duration::from_secs(2)
        )
        .as_deref(),
        Some("{\"recipes\":{}}")
    );
}

#[test]
#[ignore = "requires a real just binary on PATH; run this test explicitly with --ignored"]
fn installed_just_dump_imports_only_public_recipes() {
    assert!(executable_in_path("just"), "install just or add it to PATH");
    let h = Harness::new(
        "real-just",
        &[(
            "justfile",
            include_str!("../../tests/fixtures/projects/justfile"),
        )],
    );
    let dump = just_dump(&h.project).expect("real just --dump must succeed, without fallback");
    assert_eq!(
        parse_just_dump(&dump).unwrap(),
        vec![("public".into(), "Public recipe".into())]
    );
    assert_eq!(h.import(&[], true).unwrap().imported, 1);
    let again = h.import(&[], true).unwrap();
    assert_eq!((again.imported, again.duplicates), (0, 1));
}

#[test]
fn imported_task_names_are_literal_shell_arguments() {
    for name in [
        "dev server",
        "test; echo injected",
        "test$(printf injected)",
        "it's-a-task",
    ] {
        let definition = serde_json::json!({"scripts": {name: "echo task"}}).to_string();
        let h = Harness::new("Dev Console", &[(PACKAGE_JSON, &definition)]);
        let scanned = h.scan();
        let command = &scanned.candidates[0].command;
        let output = Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("npm() {{ printf '%s\\n' \"$@\"; }}; {command}"))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("run\n{name}\n")
        );
        assert!(!scanned.candidates[0].name.chars().any(char::is_whitespace));
        assert_eq!(h.import(&[], true).unwrap().imported, 1);
    }
    assert_eq!(task_argument("--help"), "-- --help");
    assert_eq!(task_argument("test:e2e"), "test:e2e");
}
