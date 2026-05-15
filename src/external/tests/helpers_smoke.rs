//! Smoke tests for the helpers in `tests/common/mod.rs` (US-007).

mod common;

use std::io::{Read, Write};
use std::net::TcpStream;

use assert_cmd::Command;

#[test]
fn fake_claude_project_is_detected_and_binary_runs_inside_it() {
    let project = common::fake_claude_project();

    assert!(
        project.path().join("CLAUDE.md").is_file(),
        "fake_claude_project() must seed CLAUDE.md"
    );

    // Since US-010, `main` launches the TUI. In a non-TTY environment
    // (assert_cmd pipes stdout) the TUI bails out immediately with exit 0
    // — we still want to assert the binary runs cleanly inside the fake
    // project without panicking on the harness-detection / cwd path.
    Command::cargo_bin("instinctagents")
        .expect("instinctagents binary exists")
        .current_dir(project.path())
        .write_stdin("q")
        .assert()
        .success();
}

#[test]
fn fake_codex_project_seeds_agents_md() {
    let project = common::fake_codex_project();
    assert!(
        project.path().join("AGENTS.md").is_file(),
        "fake_codex_project() must seed AGENTS.md"
    );
    assert!(
        !project.path().join("CLAUDE.md").exists(),
        "fake_codex_project() must not seed CLAUDE.md (would shadow detection)"
    );
}

#[test]
fn fake_opencode_project_seeds_opencode_json() {
    let project = common::fake_opencode_project();
    assert!(
        project.path().join("opencode.json").is_file(),
        "fake_opencode_project() must seed opencode.json"
    );
    assert!(
        !project.path().join("CLAUDE.md").exists()
            && !project.path().join("AGENTS.md").exists(),
        "fake_opencode_project() must not seed higher-priority signals"
    );
}

#[test]
fn mock_github_releases_serves_asset_at_expected_path() {
    let body: &[u8] = b"hello-release-asset";
    let releases =
        common::mock_github_releases("v0.1.0", &[("instinctagents-linux-x86_64", body)]);

    let url = releases.url();
    assert!(url.starts_with("http://"), "mock url should be http://...");

    let addr = url
        .strip_prefix("http://")
        .expect("mockito url starts with http://");

    let mut stream = TcpStream::connect(addr).expect("mock server must be listening");
    write!(
        stream,
        "GET /releases/download/v0.1.0/instinctagents-linux-x86_64 HTTP/1.1\r\n\
         Host: {addr}\r\n\
         Connection: close\r\n\
         \r\n"
    )
    .expect("write request");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read response");

    assert!(
        response.contains("200 OK"),
        "expected 200 OK in response, got:\n{response}"
    );
    assert!(
        response.contains("hello-release-asset"),
        "expected mock body in response, got:\n{response}"
    );
}
