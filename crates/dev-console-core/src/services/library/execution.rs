//! Execution: running a saved command, copying its output, opening a URL field (plan section 3).
//!
//! Part of [`LibraryActions`]. Every call here reaches a process or the clipboard, so the revision
//! and approval checks run before anything is spawned - a stale or unapproved request is refused
//! here, in the service, not in a screen. Approval is remembered per saved revision, so an edit
//! invalidates it and the next run asks again.

use crate::error::{AppError, AppResult};
use crate::execution;
use crate::models::{CopyResult, Kind, RunRequest};

use crate::services::LibraryActions;

/// The largest command output the inspector will copy, matching the execution output cap.
const MAX_COPYABLE_OUTPUT: usize = 64 * 1024;

impl LibraryActions {
    // -----------------------------------------------------------------------------------------
    // Running saved commands, their output, and URL fields
    // -----------------------------------------------------------------------------------------
    pub fn copy_command_output(&self, text: String) -> AppResult<CopyResult> {
        if text.is_empty() || text.len() > MAX_COPYABLE_OUTPUT {
            return Err(AppError::Validation(
                "command output is empty or too large to copy".into(),
            ));
        }
        let expiry = self.state.settings.get().clipboard_expiry_seconds;
        self.state
            .clipboard
            .copy(Kind::Command, "command-output", &text, expiry)
    }

    /// Run a saved command. The revision check and the approval check both come first, so a stale or
    /// unapproved request cannot reach a process; approval is remembered per saved revision, so an
    /// edit invalidates it and the next run asks again.
    pub fn run_command(&self, request: RunRequest) -> AppResult<execution::RunResult> {
        let (record, _, _) = self
            .state
            .repo
            .get_command(&request.id)?
            .ok_or(AppError::NotFound)?;
        if request.expected_revision != 0 && request.expected_revision != record.revision {
            return Err(AppError::Conflict);
        }
        if !request.approved {
            return Err(AppError::ApprovalRequired);
        }
        if !self.state.repo.approval_state(&record)? {
            self.state.repo.approve_execution(&record)?;
        }
        let result = if execution::needs_terminal(&record.command) {
            execution::execute(&self.state.settings.get().terminal, &record, "/bin/sh")?;
            execution::RunResult {
                mode: "terminal",
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                truncated: false,
                timed_out: false,
            }
        } else {
            // Blocking on purpose: the caller runs this off the UI thread.
            execution::execute_inline(&record, "/bin/sh")?
        };
        self.state.repo.record_usage(&record.id)?;
        Ok(result)
    }

    /// An http(s) link from a URL field, and nothing else.
    pub fn open_url(&self, url: String) -> AppResult<()> {
        let url = crate::services::validated_openable_url(&url)?;
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|error| AppError::Other(format!("could not open the browser: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Fixture;

    #[test]
    fn command_output_is_capped_at_what_execution_was_allowed_to_capture() {
        let fixture = Fixture::new("copy-output");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        assert_eq!(
            actions
                .copy_command_output(String::new())
                .unwrap_err()
                .to_string(),
            "command output is empty or too large to copy"
        );
        let too_big = "x".repeat(MAX_COPYABLE_OUTPUT + 1);
        assert!(actions.copy_command_output(too_big).is_err());
        // Right at the cap the size check passes; only the clipboard itself may object, and a
        // container with no display server is exactly where that happens. Either outcome is a pass
        // as long as the refusal is not the size rule.
        let at_cap = "x".repeat(MAX_COPYABLE_OUTPUT);
        if let Err(error) = actions.copy_command_output(at_cap) {
            assert!(
                !error.to_string().contains("too large"),
                "the cap is inclusive: {error}"
            );
        }
    }

    #[test]
    fn an_unapproved_or_stale_run_never_reaches_a_process() {
        let fixture = Fixture::new("run");
        fixture.seed_command("echo", "Shell", "echo hi", &[]);
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        assert_eq!(
            actions
                .run_command(RunRequest {
                    id: "echo".into(),
                    expected_revision: 1,
                    approved: false,
                })
                .err()
                .expect("an unapproved run never reaches a process")
                .to_string(),
            "running this command needs explicit approval for the saved version"
        );
        assert_eq!(
            actions
                .run_command(RunRequest {
                    id: "echo".into(),
                    expected_revision: 9,
                    approved: true,
                })
                .err()
                .expect("a stale revision is refused before anything is spawned")
                .to_string(),
            "this entry was changed elsewhere; reload it before saving"
        );
        assert_eq!(
            actions
                .run_command(RunRequest {
                    id: "absent".into(),
                    expected_revision: 0,
                    approved: true,
                })
                .err()
                .expect("a missing entry is a missing entry")
                .to_string(),
            "entry not found"
        );
    }

    #[test]
    fn only_an_http_or_https_link_is_handed_to_the_opener() {
        let fixture = Fixture::new("open-url");
        let actions = LibraryActions::new(fixture.state.clone(), "9.9.9");
        for rejected in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "-file=/tmp/evil",
        ] {
            assert_eq!(
                actions.open_url(rejected.into()).unwrap_err().to_string(),
                "only http(s) links can be opened"
            );
        }
    }
}
