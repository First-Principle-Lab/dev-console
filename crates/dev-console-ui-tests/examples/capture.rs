//! Renders one screen of the native shell to a PNG, offscreen, over a synthetic profile.
//!
//! ```sh
//! cargo run -p dev-console-ui-tests --example capture -- SCENE OUT.png [WxH] [dark|light] [PALETTE]
//! ```
//!
//! This is the design-review loop for `docs/design/ui-revamp-scorecard.md`: the same views and
//! state the binary uses, drawn by Rune's headless renderer, so no window is mapped and nothing on
//! the desktop is touched. It proves layout and paint, not compositor behaviour - the Hyprland
//! checks in AGENTS.md still decide that. Every value is synthetic.

use dev_console_core::models::{EntryDraft, ExtraField, FieldDisplay, Kind, SaveRequest};
use dev_console_desktop::app::{finder_spec, library_spec};
use dev_console_desktop::finder::Finder;
use dev_console_desktop::hub::Exec;
use dev_console_desktop::library::Library;
use dev_console_ui_tests::Profile;
use rune::App;

const SCENES: &[&str] = &[
    "library",
    "library-empty",
    "library-filter",
    "library-big",
    "secrets",
    "secrets-revealed",
    "secrets-locked",
    "editor-new",
    "editor-edit",
    "editor-secret",
    "delete",
    "run",
    "settings",
    "import",
    "environment",
    "status",
    "stress",
    "output",
    "settings-theme",
    "history",
    "history-unselected",
    "history-preview",
    "entry-menu",
    "history-empty",
    "finder",
    "finder-query",
    "finder-empty",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(scene), Some(out)) = (args.first(), args.get(1)) else {
        eprintln!(
            "usage: capture SCENE OUT.png [WxH] [dark|light] [PALETTE]\nscenes: {}",
            SCENES.join(", ")
        );
        std::process::exit(2);
    };
    if !SCENES.contains(&scene.as_str()) {
        eprintln!("unknown scene {scene:?}; scenes: {}", SCENES.join(", "));
        std::process::exit(2);
    }
    let finder = scene.starts_with("finder");
    let size = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| if finder { "680x420" } else { "1040x700" }.into());
    let appearance = args.get(3).cloned().unwrap_or_else(|| "dark".into());

    let profile = Profile::new("capture");
    if let Some(palette) = args.get(4) {
        profile.fixture.set_theme(palette);
    }
    if scene.starts_with("history") {
        let settings = profile.hub.library.get_settings();
        profile
            .fixture
            .state
            .settings
            .save(dev_console_core::models::Settings {
                history_suggestions_enabled: true,
                ..settings
            })
            .expect("settings");
    }
    let ids = if scene.ends_with("-empty") && scene != "history-empty" {
        Seeded::default()
    } else {
        seed(&profile)
    };
    // `CAPTURE_ARGS` passes further Rune tooling flags through: `--script`, `--at`, `--strip`.
    let extra = std::env::var("CAPTURE_ARGS").unwrap_or_default();
    let mut rune_args = vec![
        "--snap".to_owned(),
        out.clone(),
        "--size".to_owned(),
        size,
        "--scale".to_owned(),
        "2".to_owned(),
        "--theme".to_owned(),
        appearance,
        "--scroll-bars".to_owned(),
        "automatic".to_owned(),
    ];
    rune_args.extend(extra.split_whitespace().map(str::to_owned));

    if finder {
        let mut state = Finder::new(profile.hub.clone(), Exec::Inline);
        if scene == "finder-query" {
            state.query = rune::TextEdit::new("dep".to_owned());
            state.search();
        }
        App::new(finder_spec())
            .init(move || state)
            .args(rune_args)
            .run();
        return;
    }

    let mut state = Library::new(profile.hub.clone(), Exec::Inline);
    match scene.as_str() {
        "library" | "library-big" => state.select(&ids.command),
        "history-unselected" => state.back_to_list(),
        "history-preview" => {
            if let Some(key) = state.suggestions.first().map(|s| s.key.clone()) {
                state.select_suggestion(&key);
            }
        }
        "entry-menu" => {
            state.select(&ids.command);
            state.entry_options_open = true;
        }
        "library-filter" => {
            state.query = rune::TextEdit::new("deploy".to_owned());
            state.filter_changed();
        }
        "secrets" | "secrets-revealed" => {
            state.select_tab(Kind::Secret);
            state.select(&ids.secret);
            if scene == "secrets-revealed" {
                state.reveal();
            }
        }
        "secrets-locked" => {
            state.select_tab(Kind::Secret);
            state.lock();
        }
        "editor-new" => state.start_new(),
        "editor-edit" => {
            state.select(&ids.command);
            state.start_edit();
        }
        "editor-secret" => {
            state.select_tab(Kind::Secret);
            state.start_new();
        }
        "delete" => {
            state.select(&ids.command);
            state.request_delete(None);
        }
        "run" => {
            state.select(&ids.command);
            state.request_run();
        }
        "settings" => state.open_settings(),
        "import" => state.start_project_import(),
        "environment" => state.open_environment(ids.root.clone()),
        "status" => {
            state.select(&ids.command);
            state.copy_entry(None);
        }
        "stress" => state.select(&ids.stress),
        "output" => {
            state.select(&ids.command);
            state.run_output = Some(dev_console_desktop::library::RunOutput {
                name: "e2e-tests".into(),
                phase: dev_console_desktop::library::RunPhase::Done(
                    dev_console_core::execution::RunResult {
                        mode: "captured",
                        stdout: "Running 187 tests using 8 workers\n\n  187 passed (41.2s)\n"
                            .into(),
                        stderr: "warning: 2 snapshots are obsolete\n".into(),
                        exit_code: Some(0),
                        truncated: false,
                        timed_out: false,
                    },
                ),
            });
        }
        "settings-theme" => {
            state.open_settings();
            if let Some(panel) = state.settings.as_mut() {
                panel.choosing_theme = true;
            }
        }
        _ => {}
    }
    App::new(library_spec())
        .init(move || state)
        .args(rune_args)
        .run();
}

#[derive(Default)]
struct Seeded {
    command: String,
    stress: String,
    secret: String,
    root: String,
}

/// A profile that looks like a working developer's: two projects, loose commands, a few secrets.
fn seed(profile: &Profile) -> Seeded {
    // Under the (temporary) home folder, so paths read `~/code/..` as a real profile's do.
    let base = std::path::PathBuf::from(std::env::var("HOME").expect("isolated home")).join("code");
    let project = |name: &str| {
        let root = base.join(name);
        std::fs::create_dir_all(root.join(".git")).expect("project root");
        root.to_string_lossy().into_owned()
    };
    let storefront = project("storefront");
    let billing = project("billing-api");

    let save = |draft: EntryDraft| {
        let mut effects = dev_console_core::Effects::default();
        profile
            .hub
            .library
            .save_entry(SaveRequest { draft }, &mut effects)
            .expect("seed entry")
            .0
            .id
    };
    let command = |name: &str, line: &str, folder: &str| EntryDraft {
        working_directory: folder.into(),
        ..profile.fixture.draft(name, line)
    };

    let deploy = save(EntryDraft {
        description: "Builds the storefront and ships it to the staging cluster.".into(),
        notes: "Needs the VPN. Takes about four minutes; watch the rollout before leaving.".into(),
        tags: vec!["deploy".into(), "staging".into()],
        pinned: true,
        fields: vec![
            ExtraField {
                id: "dashboard".into(),
                label: "Dashboard".into(),
                value: "https://grafana.example.test/d/storefront".into(),
                display: FieldDisplay::Url,
            },
            ExtraField {
                id: "owner".into(),
                label: "Owner".into(),
                value: "platform team".into(),
                display: FieldDisplay::Text,
            },
        ],
        ..command(
            "deploy-storefront-staging",
            "bun run build && kubectl --context staging rollout restart deploy/storefront",
            &storefront,
        )
    });
    save(command("dev-server", "bun run dev", &storefront));
    save(command("e2e-tests", "bun run test:e2e", &storefront));
    save(command(
        "tail-staging-logs",
        "kubectl --context staging logs -f deploy/storefront",
        &storefront,
    ));
    save(command("db-migrate", "cargo sqlx migrate run", &billing));
    save(command(
        "replay-webhooks",
        "cargo run --bin replay -- --since 1h",
        &billing,
    ));
    save(command(
        "deploy-billing",
        "just deploy production",
        &billing,
    ));
    save(command("flush-dns", "resolvectl flush-caches", ""));
    save(command("free-port-3000", "fuser -k 3000/tcp", ""));
    save(command(
        "docker-prune",
        "docker image prune --all --force",
        "",
    ));

    if profile
        .hub
        .library
        .get_settings()
        .history_suggestions_enabled
        && std::env::args()
            .nth(1)
            .is_some_and(|scene| scene.starts_with("history") && scene != "history-empty")
    {
        let db = dev_console_core::project_history::database_path().expect("history path");
        std::fs::create_dir_all(db.parent().expect("history folder")).expect("history folder");
        let history =
            dev_console_core::project_history::ProjectHistory::open(&db).expect("history");
        for (root, line, runs) in [
            (&storefront, "bun run lint --fix", 9),
            (&storefront, "bunx playwright test --ui", 4),
            (&billing, "cargo nextest run --workspace", 6),
            (&storefront, "git switch main", 5),
            (&storefront, "git fetch --all --prune", 4),
            (&billing, "docker compose up --detach postgres redis", 3),
            (
                &billing,
                "cargo run --bin reconcile-invoices -- --environment staging --dry-run",
                2,
            ),
        ] {
            for _ in 0..runs {
                history.record(root, line).expect("record");
            }
        }
    }

    // A library someone has used for a year: the list's cost at rest and on a selection.
    if std::env::args().nth(1).as_deref() == Some("library-big") {
        for project_index in 0..8 {
            let root = project(&format!("service-{project_index}"));
            for index in 0..50 {
                save(command(
                    &format!("service-{project_index}-task-{index:02}"),
                    "just run",
                    &root,
                ));
            }
        }
    }

    // The worst case a real library holds: a long name, a long command, many tags, long notes.
    let stress = save(EntryDraft {
        description: "Rebuilds every container, reseeds the database from the anonymised production snapshot and replays the last hour of queued jobs against it.".into(),
        notes: "Do not run while the nightly import is going. It holds the advisory lock for the whole reseed and the import will fail half way with a constraint error that looks unrelated.\n\nIf it stops at the replay step, check the queue depth first.".into(),
        tags: vec!["database".into(), "reseed".into(), "containers".into(), "slow".into(), "careful".into()],
        fields: vec![ExtraField {
            id: "runbook".into(),
            label: "Runbook for the on-call rota".into(),
            value: "https://wiki.example.test/platform/runbooks/reseed-staging-from-anonymised-production-snapshot".into(),
            display: FieldDisplay::Url,
        }],
        ..command(
            "rebuild-containers-reseed-database-and-replay-queued-jobs-staging",
            "docker compose down --volumes --remove-orphans && docker compose build --no-cache --pull && docker compose up --detach --wait && ./scripts/reseed.sh --snapshot s3://example-bucket/anonymised/latest.sql.zst --jobs 8 && ./scripts/replay.sh --since 1h --queue default,mailers,webhooks",
            "",
        )
    });

    let secret = profile.secret("stripe-test-key", "synthetic-stripe-value-0000");
    profile.secret("staging-db-url", "synthetic-database-value-0000");
    profile.secret("github-deploy-token", "synthetic-github-value-0000");

    Seeded {
        command: deploy,
        stress,
        secret,
        root: storefront,
    }
}
