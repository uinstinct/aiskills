//! Integration test for US-010: launching the binary with 'q' on stdin must
//! exit cleanly (exit code 0). Under `assert_cmd`, stdout is a pipe (not a
//! TTY) — `tui::run` detects this via `IsTerminal` and returns immediately
//! without spinning up the alternate screen, which is exactly the
//! non-interactive escape hatch this test exercises.

use assert_cmd::Command;

mod common;

#[test]
fn binary_exits_cleanly_when_quit_is_piped_on_stdin() {
    let project = common::fake_claude_project();
    let mut cmd = Command::cargo_bin("instinctagents").unwrap();
    cmd.current_dir(project.path()).write_stdin("q");
    cmd.assert().success();
}

#[test]
fn binary_exits_cleanly_with_no_harness_detected() {
    let project = tempfile::TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("instinctagents").unwrap();
    cmd.current_dir(project.path()).write_stdin("q");
    cmd.assert().success();
}
