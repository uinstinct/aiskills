//! Shared test infrastructure for integration tests (US-007).
//!
//! Helpers:
//! - `fake_claude_project()` / `fake_codex_project()` / `fake_opencode_project()`
//!   build minimal on-disk projects in a `TempDir` that the harness detector
//!   will recognize.
//! - `mock_github_releases(tag, assets)` boots a `mockito` server that serves
//!   the supplied bytes at the GitHub Releases download path
//!   (`/releases/download/<tag>/<asset_name>`).
//!
//! All helpers are intentionally cheap and side-effect free outside the
//! returned guards. `TempDir` cleans up on drop; `MockReleases` shuts down the
//! HTTP server (and unregisters its mocks) on drop.

#![allow(dead_code)]

use std::fs;

use mockito::{Mock, ServerGuard};
use tempfile::TempDir;

pub fn fake_claude_project() -> TempDir {
    let dir = TempDir::new().expect("create tempdir for fake claude project");
    fs::write(
        dir.path().join("CLAUDE.md"),
        "# Test Claude Code project\n",
    )
    .expect("write CLAUDE.md");
    dir
}

pub fn fake_codex_project() -> TempDir {
    let dir = TempDir::new().expect("create tempdir for fake codex project");
    fs::write(dir.path().join("AGENTS.md"), "# Test Codex project\n")
        .expect("write AGENTS.md");
    dir
}

pub fn fake_opencode_project() -> TempDir {
    let dir = TempDir::new().expect("create tempdir for fake opencode project");
    fs::write(dir.path().join("opencode.json"), "{}\n").expect("write opencode.json");
    dir
}

pub struct MockReleases {
    pub server: ServerGuard,
    _mocks: Vec<Mock>,
}

impl MockReleases {
    pub fn url(&self) -> String {
        self.server.url()
    }
}

pub fn mock_github_releases(tag: &str, assets: &[(&str, &[u8])]) -> MockReleases {
    let mut server = mockito::Server::new();
    let mut mocks = Vec::with_capacity(assets.len());
    for (asset_name, body) in assets {
        let path = format!("/releases/download/{tag}/{asset_name}");
        let mock = server
            .mock("GET", path.as_str())
            .with_status(200)
            .with_body(*body)
            .create();
        mocks.push(mock);
    }
    MockReleases {
        server,
        _mocks: mocks,
    }
}
