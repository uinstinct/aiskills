//! Install/remove orchestration for skills and agents.md snippets.
//!
//! Implemented progressively across US-011..US-013. US-011 covers the
//! agents.md snippet injection path: stamping a delimited block into the
//! harness instruction file (CLAUDE.md / AGENTS.md) and recording the
//! install in `.instinctagents`.

#![allow(dead_code)]

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use crate::harness::Harness;
use crate::state::{InstalledItem, ProjectState, StateError};

#[derive(Debug)]
pub enum InstallError {
    Io(io::Error),
    State(StateError),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Io(e) => write!(f, "agents.md file io: {e}"),
            InstallError::State(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for InstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            InstallError::Io(e) => Some(e),
            InstallError::State(e) => Some(e),
        }
    }
}

impl From<io::Error> for InstallError {
    fn from(e: io::Error) -> Self {
        InstallError::Io(e)
    }
}

impl From<StateError> for InstallError {
    fn from(e: StateError) -> Self {
        InstallError::State(e)
    }
}

fn start_delim(name: &str) -> String {
    format!("<!-- instinctagents:start:{name} -->")
}

fn end_delim(name: &str) -> String {
    format!("<!-- instinctagents:end:{name} -->")
}

/// Render a delimited block. Snippet content has any leading/trailing newlines
/// trimmed so the block is always exactly three logical lines:
/// `<start>\n<snippet>\n<end>` with no trailing newline.
fn render_block(name: &str, snippet: &str) -> String {
    let body = snippet.trim_matches('\n');
    format!("{}\n{}\n{}", start_delim(name), body, end_delim(name))
}

/// Insert or replace a delimited block in `content`. Pure string operation.
///
/// If a matching `<!-- instinctagents:start:<name> -->` ... `<!-- instinctagents:end:<name> -->`
/// pair exists, the entire range between (and including) those delimiters is
/// replaced with the new block. Bytes outside that range are preserved
/// verbatim.
///
/// If no matching block exists, the new block is appended to the end of
/// `content`, preceded by a single blank line (and a trailing newline so the
/// file ends cleanly). When `content` is empty, just the block + trailing
/// newline is written.
pub fn upsert_block(content: &str, name: &str, snippet: &str) -> String {
    let start = start_delim(name);
    let end = end_delim(name);
    let block = render_block(name, snippet);

    if let Some(s_idx) = content.find(&start) {
        let tail = &content[s_idx..];
        if let Some(rel_e_idx) = tail.find(&end) {
            let e_idx = s_idx + rel_e_idx;
            let e_end = e_idx + end.len();
            let mut out = String::with_capacity(content.len() + block.len());
            out.push_str(&content[..s_idx]);
            out.push_str(&block);
            out.push_str(&content[e_end..]);
            return out;
        }
    }

    let mut out = String::with_capacity(content.len() + block.len() + 4);
    out.push_str(content);
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&block);
    out.push('\n');
    out
}

/// Read `path` (treating NotFound as empty), apply [`upsert_block`], and write
/// the result back. Creates the file if missing.
pub fn upsert_block_in_file(path: &Path, name: &str, snippet: &str) -> io::Result<()> {
    let current = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let updated = upsert_block(&current, name, snippet);
    fs::write(path, updated)
}

/// Full install path for an agents.md integration:
/// 1. Resolve the harness instruction file (`CLAUDE.md` / `AGENTS.md`).
/// 2. Inject the snippet under `<!-- instinctagents:start:<name> --> ... <end> -->`,
///    replacing any existing same-named block in place.
/// 3. Upsert the entry in `.instinctagents` under `installed_agents_md` (matched
///    by `name`).
pub fn install_agents_md_snippet(
    project_root: &Path,
    harness: Harness,
    name: &str,
    version: &str,
    source_url: &str,
    snippet: &str,
) -> Result<(), InstallError> {
    let target = project_root.join(harness.agents_md_file());
    upsert_block_in_file(&target, name, snippet)?;

    let mut state = ProjectState::load(project_root)?;
    let entry = InstalledItem {
        name: name.to_string(),
        version: version.to_string(),
        source_url: source_url.to_string(),
        install_path: None,
        delimiter_id: Some(name.to_string()),
    };
    if let Some(existing) = state
        .installed_agents_md
        .iter_mut()
        .find(|e| e.name == name)
    {
        *existing = entry;
    } else {
        state.installed_agents_md.push(entry);
    }
    state.save(project_root)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn block_for(name: &str, body: &str) -> String {
        format!(
            "<!-- instinctagents:start:{name} -->\n{body}\n<!-- instinctagents:end:{name} -->"
        )
    }

    #[test]
    fn upsert_block_into_empty_string_writes_just_the_block() {
        let out = upsert_block("", "foo", "hello\nworld");
        assert_eq!(out, format!("{}\n", block_for("foo", "hello\nworld")));
    }

    #[test]
    fn upsert_block_appends_to_user_content_with_blank_line_before() {
        let original = "# Project\n\nSome user prose.\n";
        let out = upsert_block(original, "foo", "snippet body");
        let expected = format!("{}\n{}\n", original, block_for("foo", "snippet body"));
        assert_eq!(out, expected);
        assert!(
            out.starts_with(original),
            "user content must be preserved byte-for-byte at the start of the file"
        );
    }

    #[test]
    fn upsert_block_appends_blank_line_even_when_no_trailing_newline() {
        let original = "# Project\n\nSome user prose.";
        let out = upsert_block(original, "foo", "snippet body");
        let expected = format!("# Project\n\nSome user prose.\n\n{}\n", block_for("foo", "snippet body"));
        assert_eq!(out, expected);
    }

    #[test]
    fn upsert_block_replaces_existing_block_in_place_preserving_surroundings() {
        let original = format!(
            "# Project\n\nBefore content.\n\n{}\n\nAfter content.\n",
            block_for("foo", "old body")
        );
        let out = upsert_block(&original, "foo", "new body");
        let expected = format!(
            "# Project\n\nBefore content.\n\n{}\n\nAfter content.\n",
            block_for("foo", "new body")
        );
        assert_eq!(out, expected);
        assert!(out.contains("Before content."));
        assert!(out.contains("After content."));
        assert!(!out.contains("old body"));
    }

    #[test]
    fn upsert_block_with_multiple_blocks_only_touches_matching_name() {
        let original = format!(
            "# Project\n\n{}\n\nmiddle\n\n{}\n\ntail\n",
            block_for("alpha", "alpha body"),
            block_for("beta", "old beta")
        );
        let out = upsert_block(&original, "beta", "new beta");
        let expected = format!(
            "# Project\n\n{}\n\nmiddle\n\n{}\n\ntail\n",
            block_for("alpha", "alpha body"),
            block_for("beta", "new beta")
        );
        assert_eq!(out, expected);
        assert!(out.contains("alpha body"), "unrelated block stays put");
        assert!(!out.contains("old beta"), "matching block was replaced");
    }

    #[test]
    fn upsert_block_trims_snippet_leading_trailing_newlines() {
        let out = upsert_block("", "foo", "\n\nbody\n\n");
        assert_eq!(out, format!("{}\n", block_for("foo", "body")));
    }

    #[test]
    fn upsert_block_idempotent_when_replacing_with_same_content() {
        let original = upsert_block("", "foo", "body");
        let again = upsert_block(&original, "foo", "body");
        assert_eq!(original, again);
    }

    #[test]
    fn upsert_block_in_file_creates_missing_target() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("CLAUDE.md");
        assert!(!path.exists());
        upsert_block_in_file(&path, "foo", "body").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, format!("{}\n", block_for("foo", "body")));
    }

    #[test]
    fn upsert_block_in_file_preserves_existing_user_content_verbatim() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("CLAUDE.md");
        let pre = "# Project\n\nUser prose.\n";
        fs::write(&path, pre).unwrap();
        upsert_block_in_file(&path, "foo", "body").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with(pre));
        assert!(content.contains(&block_for("foo", "body")));
    }

    #[test]
    fn install_agents_md_snippet_writes_file_and_updates_state() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "# User\n").unwrap();

        install_agents_md_snippet(
            dir.path(),
            Harness::ClaudeCode,
            "example",
            "0.1.0",
            "https://example.test/repo",
            "snippet body",
        )
        .unwrap();

        let md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(md.starts_with("# User\n"));
        assert!(md.contains(&block_for("example", "snippet body")));

        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_agents_md.len(), 1);
        let entry = &state.installed_agents_md[0];
        assert_eq!(entry.name, "example");
        assert_eq!(entry.version, "0.1.0");
        assert_eq!(entry.source_url, "https://example.test/repo");
        assert_eq!(entry.delimiter_id.as_deref(), Some("example"));
        assert!(entry.install_path.is_none());
    }

    #[test]
    fn install_agents_md_snippet_targets_agents_md_for_codex() {
        let dir = TempDir::new().unwrap();
        install_agents_md_snippet(
            dir.path(),
            Harness::Codex,
            "example",
            "0.1.0",
            "https://example.test/repo",
            "snippet body",
        )
        .unwrap();
        assert!(dir.path().join("AGENTS.md").exists());
        assert!(!dir.path().join("CLAUDE.md").exists());
    }

    #[test]
    fn install_agents_md_snippet_replaces_existing_state_entry_no_duplicate() {
        let dir = TempDir::new().unwrap();
        install_agents_md_snippet(
            dir.path(),
            Harness::ClaudeCode,
            "example",
            "0.1.0",
            "https://example.test/repo",
            "first body",
        )
        .unwrap();
        install_agents_md_snippet(
            dir.path(),
            Harness::ClaudeCode,
            "example",
            "0.2.0",
            "https://example.test/repo",
            "second body",
        )
        .unwrap();

        let md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(md.contains("second body"));
        assert!(!md.contains("first body"));
        let start_count = md.matches("instinctagents:start:example").count();
        assert_eq!(start_count, 1, "only one block should exist after upsert");

        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_agents_md.len(), 1);
        assert_eq!(state.installed_agents_md[0].version, "0.2.0");
    }
}
