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
use std::io::Cursor;

use flate2::{write::GzEncoder, Compression};
use mockito::{Mock, ServerGuard};
use tempfile::TempDir;

pub fn fake_claude_project() -> TempDir {
    let dir = TempDir::new().expect("create tempdir for fake claude project");
    fs::write(dir.path().join("CLAUDE.md"), "# Test Claude Code project\n")
        .expect("write CLAUDE.md");
    dir
}

pub fn fake_codex_project() -> TempDir {
    let dir = TempDir::new().expect("create tempdir for fake codex project");
    fs::write(dir.path().join("AGENTS.md"), "# Test Codex project\n").expect("write AGENTS.md");
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

/// Build a gzipped tar containing `<root>/<files...>`. Each entry is a
/// regular file with the given body. Mirrors the layout produced by the
/// release-packaging script (US-029): a single top-level directory named
/// after the skill / integration containing its files.
pub fn build_skill_tarball(root: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let gz = GzEncoder::new(&mut buf, Compression::default());
        let mut tb = tar::Builder::new(gz);
        for (rel, body) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            let path = format!("{root}/{rel}");
            tb.append_data(&mut header, &path, Cursor::new(body))
                .expect("append tar entry");
        }
        tb.finish().expect("finalize tar");
    }
    buf
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
