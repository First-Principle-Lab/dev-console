//! Native desktop entrypoint, preserving the installed command-line interface.
use dev_console_desktop::{
    app,
    cli::{self, Intent},
    control,
};
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match cli::parse(args) {
        Intent::Library => Ok(app::run(&["--library".into()])),
        Intent::Finder => Ok(app::run(&["--finder".into()])),
        Intent::Background => Ok(
            if dev_console_desktop::instance::instance_running(
                &dev_console_desktop::instance::socket_path(),
            ) {
                0
            } else {
                app::run(&[])
            },
        ),
        Intent::Vault(command) => control::run(command),
        Intent::ImportBitwarden(file) => control::run_import(&file),
        Intent::ScanProject { dir } => control::run_project(&dir, false),
        Intent::ImportProject { dir } => control::run_project(&dir, true),
        Intent::HistorySetup => {
            dev_console_core::project_history::install_zsh_hook().map(|message| {
                println!("{message}");
                0
            })
        }
        Intent::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        Intent::Help => {
            println!("{}", cli::USAGE);
            Ok(0)
        }
    };
    let code = result.unwrap_or_else(|error| {
        eprintln!("dev-console: {error}");
        1
    });
    std::process::exit(code);
}
