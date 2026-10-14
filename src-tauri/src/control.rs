use std::io::Write;

use crate::auth;
use crate::cli::VaultCommand;
use crate::error::AppResult;
use crate::paths;
use crate::vault::VaultStatus;

/// Exit codes are part of the CLI contract so scripts can branch on them.
pub const EXIT_OK: i32 = 0;
pub const EXIT_LOCKED: i32 = 10;
pub const EXIT_UNAVAILABLE: i32 = 20;
pub const EXIT_ERROR: i32 = 1;

/// CLI side of the vault control API.
///
/// Stage note: the running-instance acknowledgement path (D-Bus session control API) is added
/// with the vault checkpoint. Lock and status are still honest here: they read and write the
/// same persistent manual-lock flag the backend enforces, and never touch secret data.
pub fn run(command: VaultCommand) -> AppResult<i32> {
    match command {
        VaultCommand::Lock => {
            let flag = paths::lock_flag_path()?;
            if let Some(parent) = flag.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            // Atomic and never auto-unlocks: the flag is written without opening the vault.
            let tmp = flag.with_extension("tmp");
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&tmp)
                .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
            file.write_all(b"locked\n")?;
            file.flush()?;
            crate::paths::set_file_private(&tmp)?;
            std::fs::rename(&tmp, &flag)
                .map_err(|e| crate::error::AppError::Other(e.to_string()))?;
            println!(
                "vault manually locked; automatic unlock is disabled until an authenticated unlock"
            );
            Ok(EXIT_OK)
        }
        VaultCommand::Unlock => {
            let flag = paths::lock_flag_path()?;
            if !flag.exists() {
                println!("vault is not manually locked");
                return Ok(EXIT_OK);
            }
            if !auth::mechanism_available() {
                eprintln!("{}", auth::unavailable_reason());
                return Ok(EXIT_UNAVAILABLE);
            }
            eprintln!("authenticated unlock helper is present; the verified unlock flow lands with the vault checkpoint");
            Ok(EXIT_UNAVAILABLE)
        }
        VaultCommand::Status => {
            let service = crate::vault::VaultService::new(paths::lock_flag_path()?);
            match service.status() {
                VaultStatus::Unlocked => {
                    println!("unlocked");
                    Ok(EXIT_OK)
                }
                VaultStatus::Locked { manual, .. } => {
                    println!("{}", if manual { "locked (manual)" } else { "locked" });
                    Ok(EXIT_LOCKED)
                }
                VaultStatus::Unavailable(reason) => {
                    println!("unavailable: {reason}");
                    Ok(EXIT_UNAVAILABLE)
                }
            }
        }
    }
}
