//! Install/remove orchestration for skills and agents.md snippets.
//!
//! Implemented progressively across US-011..US-013. US-011 covers the
//! agents.md snippet injection path. US-012 adds skill tarball install
//! (download via [`crate::http`], extract to `<harness>/skills/<name>/`) plus
//! conflict resolution (Skip / Overwrite / Rename) and the agents.md
//! tarball wrapper that pulls a snippet out of an archive and delegates to
//! [`install_agents_md_snippet`]. US-013 adds removal: deleting the skill
//! folder, deleting the delimited agents.md block, and pruning the matching
//! entry from `.instinctagents`.

use std::fmt;
use std::fs;
use std::io;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use tar::Archive;

use crate::harness::Harness;
use crate::http::{self, HttpError};
use crate::state::{InstalledItem, ProjectState, StateError};

/// GitHub `<owner>/<repo>` hosting the registry's release tarballs.
pub const REGISTRY_REPO: &str = "uinstinct/aiskills";

/// Release tag the running binary fetches assets from. Pinned to its own
/// crate version so each binary release is married to a specific catalog
/// snapshot.
pub const REGISTRY_TAG: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Asset filename convention (see US-029).
pub fn skill_asset_name(name: &str, version: &str) -> String {
    format!("skill-{name}-{version}.tar.gz")
}

pub fn agents_md_asset_name(name: &str, version: &str) -> String {
    format!("agents-md-{name}-{version}.tar.gz")
}

#[derive(Debug)]
pub enum InstallError {
    Io(io::Error),
    State(StateError),
    Http(HttpError),
    Tarball(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Io(e) => write!(f, "agents.md file io: {e}"),
            InstallError::State(e) => write!(f, "{e}"),
            InstallError::Http(e) => write!(f, "{e}"),
            InstallError::Tarball(msg) => write!(f, "tarball: {msg}"),
        }
    }
}

impl std::error::Error for InstallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            InstallError::Io(e) => Some(e),
            InstallError::State(e) => Some(e),
            InstallError::Http(e) => Some(e),
            InstallError::Tarball(_) => None,
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

impl From<HttpError> for InstallError {
    fn from(e: HttpError) -> Self {
        InstallError::Http(e)
    }
}

/// How to resolve a pre-existing target folder when installing a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteAction {
    /// Leave the existing folder untouched. Returns
    /// [`SkillInstallOutcome::Skipped`].
    Skip,
    /// Delete the existing folder and extract over it.
    Overwrite,
    /// Extract to a suffixed name (`<name>-2`, `<name>-3`, …).
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillInstallOutcome {
    Installed {
        install_path: String,
    },
    Skipped,
    Renamed {
        install_path: String,
        new_name: String,
    },
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

/// Default snippet filename inside an `agents-md-<name>-<version>.tar.gz`
/// archive. The manifest schema (US-002) lets integrations override this, but
/// the embedded catalog doesn't expose `snippet_file` yet — when it does, the
/// caller will pass the override and this constant becomes the fallback.
pub const DEFAULT_SNIPPET_FILE: &str = "snippet.md";

/// Returns `true` if a skill is already installed on disk for this harness.
pub fn skill_target_exists(project_root: &Path, harness: Harness, name: &str) -> bool {
    project_root
        .join(harness.target_folder())
        .join(name)
        .is_dir()
}

fn compose_install_path(harness: Harness, name: &str) -> String {
    format!("{}{}/", harness.target_folder(), name)
}

fn extract_tarball(dest: &Path, tarball_gz: &[u8]) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    let gz = GzDecoder::new(tarball_gz);
    let mut archive = Archive::new(gz);
    archive.set_overwrite(true);
    archive.set_preserve_permissions(false);
    archive.unpack(dest)
}

fn next_available_name(target_root: &Path, name: &str) -> String {
    for n in 2..u32::MAX {
        let candidate = format!("{name}-{n}");
        if !target_root.join(&candidate).exists() {
            return candidate;
        }
    }
    // Practically unreachable.
    format!("{name}-renamed")
}

fn upsert_skill_state(
    project_root: &Path,
    name: &str,
    version: &str,
    source_url: &str,
    install_path: &str,
) -> Result<(), InstallError> {
    let mut state = ProjectState::load(project_root)?;
    let entry = InstalledItem {
        name: name.to_string(),
        version: version.to_string(),
        source_url: source_url.to_string(),
        install_path: Some(install_path.to_string()),
        delimiter_id: None,
    };
    if let Some(existing) = state.installed_skills.iter_mut().find(|e| e.name == name) {
        *existing = entry;
    } else {
        state.installed_skills.push(entry);
    }
    state.save(project_root)?;
    Ok(())
}

/// Install a skill from an in-memory gzipped tarball.
///
/// The tarball is expected to follow the US-029 layout: a single top-level
/// directory named `<name>/` containing the skill's files. The directory is
/// extracted into `<project_root>/<harness.target_folder()>` so the final on-
/// disk path becomes `<harness>/skills/<name>/`.
///
/// If the target folder already exists, behavior is controlled by
/// `on_conflict`:
/// * [`OverwriteAction::Skip`] — return [`SkillInstallOutcome::Skipped`]; no
///   filesystem or state changes.
/// * [`OverwriteAction::Overwrite`] — `fs::remove_dir_all` the existing folder
///   then extract on top.
/// * [`OverwriteAction::Rename`] — extract, then rename the new folder to
///   `<name>-2` (or the next free suffix) and record that name in state.
///
/// On success, `.instinctagents` is updated with an `installed_skills` entry
/// matched by name.
pub fn install_skill_from_tarball(
    project_root: &Path,
    harness: Harness,
    name: &str,
    version: &str,
    source_url: &str,
    tarball_gz: &[u8],
    on_conflict: OverwriteAction,
) -> Result<SkillInstallOutcome, InstallError> {
    let target_root = project_root.join(harness.target_folder());
    fs::create_dir_all(&target_root)?;

    let final_dir = target_root.join(name);

    if final_dir.exists() {
        match on_conflict {
            OverwriteAction::Skip => return Ok(SkillInstallOutcome::Skipped),
            OverwriteAction::Overwrite => {
                fs::remove_dir_all(&final_dir)?;
                extract_tarball(&target_root, tarball_gz)?;
                let install_path = compose_install_path(harness, name);
                upsert_skill_state(project_root, name, version, source_url, &install_path)?;
                return Ok(SkillInstallOutcome::Installed { install_path });
            }
            OverwriteAction::Rename => {
                let new_name = next_available_name(&target_root, name);
                // Stage extraction in a sibling scratch dir so the existing
                // `<name>/` folder stays byte-identical. The tarball's own
                // root is `<name>/`, so files land at `<scratch>/<name>/…`;
                // we then move that subdirectory to `<new_name>` and drop the
                // scratch dir.
                let scratch = target_root.join(format!(".instinctagents.rename.{new_name}"));
                if scratch.exists() {
                    fs::remove_dir_all(&scratch)?;
                }
                extract_tarball(&scratch, tarball_gz)?;
                let src = scratch.join(name);
                let dst = target_root.join(&new_name);
                fs::rename(&src, &dst)?;
                let _ = fs::remove_dir_all(&scratch);

                let install_path = format!("{}{}/", harness.target_folder(), new_name);
                upsert_skill_state(project_root, &new_name, version, source_url, &install_path)?;
                return Ok(SkillInstallOutcome::Renamed {
                    install_path,
                    new_name,
                });
            }
        }
    }

    extract_tarball(&target_root, tarball_gz)?;
    let install_path = compose_install_path(harness, name);
    upsert_skill_state(project_root, name, version, source_url, &install_path)?;
    Ok(SkillInstallOutcome::Installed { install_path })
}

/// Read `<name>/<snippet_file>` out of a gzipped tarball and return its body.
pub fn read_snippet_from_tarball(
    tarball_gz: &[u8],
    name: &str,
    snippet_file: &str,
) -> Result<String, InstallError> {
    let target_path = format!("{name}/{snippet_file}");
    let gz = GzDecoder::new(tarball_gz);
    let mut archive = Archive::new(gz);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if path.to_string_lossy() == target_path {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return Ok(content);
        }
    }
    Err(InstallError::Tarball(format!(
        "snippet file '{target_path}' not found in tarball"
    )))
}

/// Install an agents.md integration from a gzipped tarball.
///
/// The tarball is expected to follow the US-029 layout (`<name>/snippet.md`).
/// The snippet body is extracted in-memory and handed to
/// [`install_agents_md_snippet`] which performs the delimiter-block injection
/// and state upsert.
pub fn install_agents_md_from_tarball(
    project_root: &Path,
    harness: Harness,
    name: &str,
    version: &str,
    source_url: &str,
    tarball_gz: &[u8],
) -> Result<(), InstallError> {
    let snippet = read_snippet_from_tarball(tarball_gz, name, DEFAULT_SNIPPET_FILE)?;
    install_agents_md_snippet(project_root, harness, name, version, source_url, &snippet)
}

/// Fetch a skill tarball from the registry's GitHub Releases for the binary's
/// pinned [`REGISTRY_TAG`] and install it. Thin wrapper used by the TUI.
pub fn fetch_and_install_skill(
    project_root: &Path,
    harness: Harness,
    name: &str,
    version: &str,
    source_url: &str,
    on_conflict: OverwriteAction,
) -> Result<SkillInstallOutcome, InstallError> {
    let asset = skill_asset_name(name, version);
    let bytes = http::download_release_asset(REGISTRY_REPO, REGISTRY_TAG, &asset)?;
    install_skill_from_tarball(
        project_root,
        harness,
        name,
        version,
        source_url,
        &bytes,
        on_conflict,
    )
}

/// Fetch an agents.md tarball from the registry's GitHub Releases for the
/// binary's pinned [`REGISTRY_TAG`] and install it. Thin wrapper used by the
/// TUI.
pub fn fetch_and_install_agents_md(
    project_root: &Path,
    harness: Harness,
    name: &str,
    version: &str,
    source_url: &str,
) -> Result<(), InstallError> {
    let asset = agents_md_asset_name(name, version);
    let bytes = http::download_release_asset(REGISTRY_REPO, REGISTRY_TAG, &asset)?;
    install_agents_md_from_tarball(project_root, harness, name, version, source_url, &bytes)
}

/// Remove the delimited block named `name` from `content`. Pure string op.
///
/// Returns the new content plus `true` if a block was found and removed,
/// `false` if no matching delimiter pair existed (content returned unchanged).
/// Bytes outside the removed range are preserved verbatim — surrounding
/// whitespace (blank lines that were inserted by [`upsert_block`]) is left in
/// place; callers that want a tidier file structure can post-process.
pub fn remove_block(content: &str, name: &str) -> (String, bool) {
    let start = start_delim(name);
    let end = end_delim(name);
    if let Some(s_idx) = content.find(&start) {
        let tail = &content[s_idx..];
        if let Some(rel_e_idx) = tail.find(&end) {
            let e_idx = s_idx + rel_e_idx;
            let e_end = e_idx + end.len();
            let mut out = String::with_capacity(content.len());
            out.push_str(&content[..s_idx]);
            out.push_str(&content[e_end..]);
            return (out, true);
        }
    }
    (content.to_string(), false)
}

/// Read `path`, drop the delimited block named `name`, write back. Returns
/// `true` if the block was found and removed, `false` otherwise (also returns
/// `false` when the file is missing — there is nothing to remove).
pub fn remove_block_from_file(path: &Path, name: &str) -> io::Result<bool> {
    let current = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let (updated, found) = remove_block(&current, name);
    if found {
        fs::write(path, updated)?;
    }
    Ok(found)
}

/// Full uninstall path for a skill:
/// 1. Delete `<project_root>/<harness.target_folder()>/<name>/` if it exists.
/// 2. Drop the matching entry from `installed_skills` in `.instinctagents`.
///
/// Missing folder is not an error — the state is still cleaned up so future
/// installs work correctly.
pub fn remove_skill(project_root: &Path, harness: Harness, name: &str) -> Result<(), InstallError> {
    let skill_dir = project_root.join(harness.target_folder()).join(name);
    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir)?;
    }
    let mut state = ProjectState::load(project_root)?;
    state.installed_skills.retain(|e| e.name != name);
    state.save(project_root)?;
    Ok(())
}

/// Full uninstall path for an agents.md integration:
/// 1. Remove the delimited block from `<project_root>/<harness.agents_md_file()>`.
/// 2. Drop the matching entry from `installed_agents_md` in `.instinctagents`.
///
/// Returns `true` if the delimiter block was found and removed from the file,
/// `false` if it was missing (e.g. user deleted it manually); the state entry
/// is removed in either case.
pub fn remove_agents_md(
    project_root: &Path,
    harness: Harness,
    name: &str,
) -> Result<bool, InstallError> {
    let target = project_root.join(harness.agents_md_file());
    let found = remove_block_from_file(&target, name)?;
    let mut state = ProjectState::load(project_root)?;
    state.installed_agents_md.retain(|e| e.name != name);
    state.save(project_root)?;
    Ok(found)
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
        format!("<!-- instinctagents:start:{name} -->\n{body}\n<!-- instinctagents:end:{name} -->")
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
        let expected = format!(
            "# Project\n\nSome user prose.\n\n{}\n",
            block_for("foo", "snippet body")
        );
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

    // -- skill tarball install tests -----------------------------------------

    /// Build a gzipped tar containing `<root>/<files...>`. Each entry is a
    /// regular file with the given body.
    fn build_skill_tarball(root: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Cursor;

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
                    .unwrap();
            }
            tb.finish().unwrap();
        }
        buf
    }

    #[test]
    fn install_skill_from_tarball_extracts_to_harness_folder() {
        let dir = TempDir::new().unwrap();
        let gz = build_skill_tarball(
            "demo",
            &[
                ("SKILL.md", b"# demo skill"),
                ("manifest.yml", b"name: demo\n"),
            ],
        );

        let outcome = install_skill_from_tarball(
            dir.path(),
            Harness::ClaudeCode,
            "demo",
            "0.3.0",
            "https://example.test/demo",
            &gz,
            OverwriteAction::Skip,
        )
        .unwrap();

        assert_eq!(
            outcome,
            SkillInstallOutcome::Installed {
                install_path: ".claude/skills/demo/".to_string()
            }
        );

        let skill_root = dir.path().join(".claude/skills/demo");
        assert!(skill_root.is_dir());
        let body = fs::read_to_string(skill_root.join("SKILL.md")).unwrap();
        assert_eq!(body, "# demo skill");
        assert!(skill_root.join("manifest.yml").exists());

        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_skills.len(), 1);
        let entry = &state.installed_skills[0];
        assert_eq!(entry.name, "demo");
        assert_eq!(entry.version, "0.3.0");
        assert_eq!(entry.source_url, "https://example.test/demo");
        assert_eq!(entry.install_path.as_deref(), Some(".claude/skills/demo/"));
        assert!(entry.delimiter_id.is_none());
    }

    #[test]
    fn install_skill_from_tarball_skip_when_target_exists() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join(".claude/skills/demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("KEEP"), b"pre-existing").unwrap();

        let gz = build_skill_tarball("demo", &[("SKILL.md", b"# new")]);
        let outcome = install_skill_from_tarball(
            dir.path(),
            Harness::ClaudeCode,
            "demo",
            "0.3.0",
            "https://example.test/demo",
            &gz,
            OverwriteAction::Skip,
        )
        .unwrap();

        assert_eq!(outcome, SkillInstallOutcome::Skipped);
        assert!(target.join("KEEP").exists(), "skip leaves original alone");
        assert!(!target.join("SKILL.md").exists());
        let state = ProjectState::load(dir.path()).unwrap();
        assert!(state.installed_skills.is_empty(), "state untouched on skip");
    }

    #[test]
    fn install_skill_from_tarball_overwrite_replaces_existing() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join(".claude/skills/demo");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("OLD"), b"old data").unwrap();

        let gz = build_skill_tarball("demo", &[("SKILL.md", b"# new")]);
        let outcome = install_skill_from_tarball(
            dir.path(),
            Harness::ClaudeCode,
            "demo",
            "0.3.0",
            "https://example.test/demo",
            &gz,
            OverwriteAction::Overwrite,
        )
        .unwrap();

        assert!(matches!(outcome, SkillInstallOutcome::Installed { .. }));
        assert!(target.join("SKILL.md").exists());
        assert!(
            !target.join("OLD").exists(),
            "overwrite removes the previous folder before extracting"
        );
        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_skills.len(), 1);
        assert_eq!(state.installed_skills[0].name, "demo");
    }

    #[test]
    fn install_skill_from_tarball_rename_uses_suffixed_folder() {
        let dir = TempDir::new().unwrap();
        let target_root = dir.path().join(".claude/skills");
        fs::create_dir_all(target_root.join("demo")).unwrap();
        fs::write(target_root.join("demo/OLD"), b"old").unwrap();

        let gz = build_skill_tarball("demo", &[("SKILL.md", b"# new")]);
        let outcome = install_skill_from_tarball(
            dir.path(),
            Harness::ClaudeCode,
            "demo",
            "0.3.0",
            "https://example.test/demo",
            &gz,
            OverwriteAction::Rename,
        )
        .unwrap();

        match outcome {
            SkillInstallOutcome::Renamed {
                new_name,
                install_path,
            } => {
                assert_eq!(new_name, "demo-2");
                assert_eq!(install_path, ".claude/skills/demo-2/");
            }
            other => panic!("expected Renamed, got {other:?}"),
        }

        assert!(target_root.join("demo/OLD").exists(), "original untouched");
        assert!(target_root.join("demo-2/SKILL.md").exists());
        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_skills.len(), 1);
        assert_eq!(state.installed_skills[0].name, "demo-2");
        assert_eq!(
            state.installed_skills[0].install_path.as_deref(),
            Some(".claude/skills/demo-2/")
        );
    }

    #[test]
    fn install_skill_targets_codex_folder() {
        let dir = TempDir::new().unwrap();
        let gz = build_skill_tarball("demo", &[("SKILL.md", b"# demo")]);
        install_skill_from_tarball(
            dir.path(),
            Harness::Codex,
            "demo",
            "0.1.0",
            "https://example.test/demo",
            &gz,
            OverwriteAction::Skip,
        )
        .unwrap();
        assert!(dir.path().join(".codex/skills/demo/SKILL.md").exists());
        assert!(!dir.path().join(".claude").exists());
    }

    #[test]
    fn install_skill_targets_opencode_folder() {
        let dir = TempDir::new().unwrap();
        let gz = build_skill_tarball("demo", &[("SKILL.md", b"# demo")]);
        install_skill_from_tarball(
            dir.path(),
            Harness::OpenCode,
            "demo",
            "0.1.0",
            "https://example.test/demo",
            &gz,
            OverwriteAction::Skip,
        )
        .unwrap();
        assert!(dir.path().join(".opencode/skills/demo/SKILL.md").exists());
    }

    #[test]
    fn skill_target_exists_reports_target_state() {
        let dir = TempDir::new().unwrap();
        assert!(!skill_target_exists(
            dir.path(),
            Harness::ClaudeCode,
            "demo"
        ));
        fs::create_dir_all(dir.path().join(".claude/skills/demo")).unwrap();
        assert!(skill_target_exists(dir.path(), Harness::ClaudeCode, "demo"));
    }

    // -- agents.md tarball install tests -------------------------------------

    #[test]
    fn read_snippet_from_tarball_returns_body() {
        let gz = build_skill_tarball("demo", &[("snippet.md", b"hello body")]);
        let body = read_snippet_from_tarball(&gz, "demo", "snippet.md").unwrap();
        assert_eq!(body, "hello body");
    }

    #[test]
    fn read_snippet_from_tarball_missing_file_errors() {
        let gz = build_skill_tarball("demo", &[("other.md", b"x")]);
        let err = read_snippet_from_tarball(&gz, "demo", "snippet.md").unwrap_err();
        match err {
            InstallError::Tarball(msg) => assert!(msg.contains("snippet.md")),
            other => panic!("expected Tarball error, got {other:?}"),
        }
    }

    #[test]
    fn install_agents_md_from_tarball_injects_snippet() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "# User content\n").unwrap();
        let gz = build_skill_tarball("integ", &[("snippet.md", b"snippet from tar")]);

        install_agents_md_from_tarball(
            dir.path(),
            Harness::ClaudeCode,
            "integ",
            "0.1.0",
            "https://example.test/integ",
            &gz,
        )
        .unwrap();

        let md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(md.starts_with("# User content\n"));
        assert!(md.contains(&block_for("integ", "snippet from tar")));
        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_agents_md.len(), 1);
        assert_eq!(state.installed_agents_md[0].name, "integ");
    }

    #[test]
    fn skill_asset_name_format() {
        assert_eq!(skill_asset_name("foo", "1.2.3"), "skill-foo-1.2.3.tar.gz");
        assert_eq!(
            agents_md_asset_name("bar", "0.1.0"),
            "agents-md-bar-0.1.0.tar.gz"
        );
    }

    // -- remove tests --------------------------------------------------------

    #[test]
    fn remove_block_drops_matching_pair_and_reports_true() {
        let original = format!(
            "# Project\n\nBefore.\n\n{}\n\nAfter.\n",
            block_for("foo", "body")
        );
        let (out, found) = remove_block(&original, "foo");
        assert!(found);
        assert!(!out.contains("instinctagents:start:foo"));
        assert!(!out.contains("instinctagents:end:foo"));
        assert!(out.contains("Before."));
        assert!(out.contains("After."));
    }

    #[test]
    fn remove_block_missing_returns_unchanged_and_false() {
        let original = "# Project\n\nNo blocks here.\n";
        let (out, found) = remove_block(original, "foo");
        assert!(!found);
        assert_eq!(out, original);
    }

    #[test]
    fn remove_block_only_touches_matching_name() {
        let original = format!(
            "head\n\n{}\n\n{}\n\ntail\n",
            block_for("alpha", "alpha body"),
            block_for("beta", "beta body")
        );
        let (out, found) = remove_block(&original, "beta");
        assert!(found);
        assert!(out.contains("alpha body"), "non-matching block preserved");
        assert!(!out.contains("beta body"));
        assert!(out.contains("instinctagents:start:alpha"));
        assert!(!out.contains("instinctagents:start:beta"));
    }

    #[test]
    fn remove_block_from_file_missing_returns_false() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("CLAUDE.md");
        let found = remove_block_from_file(&path, "foo").unwrap();
        assert!(!found);
        assert!(!path.exists());
    }

    #[test]
    fn remove_skill_deletes_folder_and_state_entry() {
        let dir = TempDir::new().unwrap();
        // Install first so we have folder + state.
        let gz = build_skill_tarball("demo", &[("SKILL.md", b"# demo")]);
        install_skill_from_tarball(
            dir.path(),
            Harness::ClaudeCode,
            "demo",
            "0.1.0",
            "https://example.test/demo",
            &gz,
            OverwriteAction::Skip,
        )
        .unwrap();
        assert!(dir.path().join(".claude/skills/demo").is_dir());

        remove_skill(dir.path(), Harness::ClaudeCode, "demo").unwrap();
        assert!(!dir.path().join(".claude/skills/demo").exists());
        let state = ProjectState::load(dir.path()).unwrap();
        assert!(state.installed_skills.is_empty());
    }

    #[test]
    fn remove_skill_missing_folder_still_prunes_state() {
        let dir = TempDir::new().unwrap();
        // Seed state without on-disk folder (partial-state edge).
        let mut state = ProjectState::default();
        state.installed_skills.push(InstalledItem {
            name: "ghost".into(),
            version: "0.1.0".into(),
            source_url: "https://example.test/ghost".into(),
            install_path: Some(".claude/skills/ghost/".into()),
            delimiter_id: None,
        });
        state.save(dir.path()).unwrap();

        remove_skill(dir.path(), Harness::ClaudeCode, "ghost").unwrap();
        let after = ProjectState::load(dir.path()).unwrap();
        assert!(after.installed_skills.is_empty());
    }

    #[test]
    fn remove_skill_only_drops_matching_state_entry() {
        let dir = TempDir::new().unwrap();
        let gz = build_skill_tarball("a", &[("SKILL.md", b"a")]);
        install_skill_from_tarball(
            dir.path(),
            Harness::ClaudeCode,
            "a",
            "0.1.0",
            "https://example.test/a",
            &gz,
            OverwriteAction::Skip,
        )
        .unwrap();
        let gz2 = build_skill_tarball("b", &[("SKILL.md", b"b")]);
        install_skill_from_tarball(
            dir.path(),
            Harness::ClaudeCode,
            "b",
            "0.1.0",
            "https://example.test/b",
            &gz2,
            OverwriteAction::Skip,
        )
        .unwrap();

        remove_skill(dir.path(), Harness::ClaudeCode, "a").unwrap();
        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_skills.len(), 1);
        assert_eq!(state.installed_skills[0].name, "b");
        assert!(!dir.path().join(".claude/skills/a").exists());
        assert!(dir.path().join(".claude/skills/b").is_dir());
    }

    #[test]
    fn remove_agents_md_drops_block_and_state_entry() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("CLAUDE.md"), "# User\n").unwrap();
        install_agents_md_snippet(
            dir.path(),
            Harness::ClaudeCode,
            "integ",
            "0.1.0",
            "https://example.test/integ",
            "snippet body",
        )
        .unwrap();

        let found = remove_agents_md(dir.path(), Harness::ClaudeCode, "integ").unwrap();
        assert!(found);

        let md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(md.contains("# User"));
        assert!(!md.contains("instinctagents:start:integ"));
        assert!(!md.contains("snippet body"));

        let state = ProjectState::load(dir.path()).unwrap();
        assert!(state.installed_agents_md.is_empty());
    }

    #[test]
    fn remove_agents_md_missing_block_still_prunes_state() {
        let dir = TempDir::new().unwrap();
        // State entry exists but file has no matching delimiter pair
        // (user-deleted the block by hand).
        fs::write(
            dir.path().join("CLAUDE.md"),
            "# User content with no block here.\n",
        )
        .unwrap();
        let mut state = ProjectState::default();
        state.installed_agents_md.push(InstalledItem {
            name: "orphan".into(),
            version: "0.1.0".into(),
            source_url: "https://example.test/orphan".into(),
            install_path: None,
            delimiter_id: Some("orphan".into()),
        });
        state.save(dir.path()).unwrap();

        let found = remove_agents_md(dir.path(), Harness::ClaudeCode, "orphan").unwrap();
        assert!(!found, "missing delimiter pair reported as not-found");
        let after = ProjectState::load(dir.path()).unwrap();
        assert!(after.installed_agents_md.is_empty());
    }

    #[test]
    fn remove_agents_md_targets_codex_agents_md() {
        let dir = TempDir::new().unwrap();
        install_agents_md_snippet(
            dir.path(),
            Harness::Codex,
            "integ",
            "0.1.0",
            "https://example.test/integ",
            "snippet body",
        )
        .unwrap();
        assert!(dir.path().join("AGENTS.md").exists());

        let found = remove_agents_md(dir.path(), Harness::Codex, "integ").unwrap();
        assert!(found);
        let md = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
        assert!(!md.contains("instinctagents:start:integ"));
    }

    #[test]
    fn remove_agents_md_preserves_unrelated_block() {
        let dir = TempDir::new().unwrap();
        install_agents_md_snippet(
            dir.path(),
            Harness::ClaudeCode,
            "alpha",
            "0.1.0",
            "https://example.test/alpha",
            "alpha body",
        )
        .unwrap();
        install_agents_md_snippet(
            dir.path(),
            Harness::ClaudeCode,
            "beta",
            "0.1.0",
            "https://example.test/beta",
            "beta body",
        )
        .unwrap();

        remove_agents_md(dir.path(), Harness::ClaudeCode, "beta").unwrap();
        let md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(md.contains("alpha body"));
        assert!(!md.contains("beta body"));
        let state = ProjectState::load(dir.path()).unwrap();
        assert_eq!(state.installed_agents_md.len(), 1);
        assert_eq!(state.installed_agents_md[0].name, "alpha");
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
