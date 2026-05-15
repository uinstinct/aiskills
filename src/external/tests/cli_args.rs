//! Integration tests for the CLI argument parsing (US-016). These exercise
//! the actual built binary via `assert_cmd` to catch regressions in the
//! bare-invocation path (which must still route to the TUI's `IsTerminal`
//! escape hatch) and verify `--version` / `--help` work.

use assert_cmd::Command;
use predicates::str::contains;

mod common;

#[test]
fn version_flag_prints_crate_version_and_exits_zero() {
    Command::cargo_bin("instinctagents")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_flag_prints_usage_and_exits_zero() {
    Command::cargo_bin("instinctagents")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage:"))
        .stdout(contains("--non-interactive"))
        .stdout(contains("--force"))
        .stdout(contains("--verbose"));
}

#[test]
fn non_interactive_without_subcommand_errors() {
    let project = common::fake_claude_project();
    Command::cargo_bin("instinctagents")
        .unwrap()
        .current_dir(project.path())
        .arg("--non-interactive")
        .assert()
        .failure()
        .stderr(contains("requires a subcommand"));
}

#[test]
fn non_interactive_remove_unknown_item_errors_cleanly() {
    let project = common::fake_claude_project();
    Command::cargo_bin("instinctagents")
        .unwrap()
        .current_dir(project.path())
        .args(["--non-interactive", "remove", "--name", "no-such-thing"])
        .assert()
        .failure()
        .stderr(contains("not currently installed"));
}

#[test]
fn non_interactive_add_without_harness_errors_cleanly() {
    let project = tempfile::TempDir::new().unwrap();
    Command::cargo_bin("instinctagents")
        .unwrap()
        .current_dir(project.path())
        .args(["--non-interactive", "add", "--name", "storytelling-mastery-skill"])
        .assert()
        .failure()
        .stderr(contains("no harness detected"));
}

#[test]
fn bare_invocation_still_exits_cleanly_in_non_tty() {
    // Sanity check: parsing the empty arg list must NOT print help/error;
    // it must fall through to `tui::run`, which the IsTerminal escape hatch
    // turns into an immediate Ok(()) under assert_cmd's piped stdout.
    let project = common::fake_claude_project();
    Command::cargo_bin("instinctagents")
        .unwrap()
        .current_dir(project.path())
        .write_stdin("")
        .assert()
        .success();
}
