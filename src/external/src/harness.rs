//! Harness detection (Claude Code / Codex / OpenCode). Implemented in US-006.
//!
//! Looks at a project root and decides which coding-agent harness is in use,
//! using a fixed priority order so projects that carry signals for multiple
//! harnesses still resolve deterministically.

#![allow(dead_code)]

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    ClaudeCode,
    Codex,
    OpenCode,
}

impl Harness {
    pub fn target_folder(self) -> &'static str {
        match self {
            Harness::ClaudeCode => ".claude/skills/",
            Harness::Codex => ".codex/skills/",
            Harness::OpenCode => ".opencode/skills/",
        }
    }

    pub fn agents_md_file(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "CLAUDE.md",
            Harness::Codex => "AGENTS.md",
            Harness::OpenCode => "AGENTS.md",
        }
    }
}

/// Detect the harness in use for `project_root`.
///
/// Priority (first match wins): ClaudeCode → Codex → OpenCode. Returns `None`
/// when no signal is present.
pub fn detect(project_root: &Path) -> Option<Harness> {
    if project_root.join("CLAUDE.md").is_file() || project_root.join(".claude").is_dir() {
        return Some(Harness::ClaudeCode);
    }
    if project_root.join("AGENTS.md").is_file() || project_root.join(".codex").is_dir() {
        return Some(Harness::Codex);
    }
    if project_root.join("opencode.json").is_file()
        || project_root.join("opencode.jsonc").is_file()
        || project_root.join(".opencode").is_dir()
    {
        return Some(Harness::OpenCode);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn touch(root: &Path, rel: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, b"").unwrap();
    }

    fn mkdir(root: &Path, rel: &str) {
        std::fs::create_dir_all(root.join(rel)).unwrap();
    }

    #[test]
    fn target_folder_paths() {
        assert_eq!(Harness::ClaudeCode.target_folder(), ".claude/skills/");
        assert_eq!(Harness::Codex.target_folder(), ".codex/skills/");
        assert_eq!(Harness::OpenCode.target_folder(), ".opencode/skills/");
    }

    #[test]
    fn agents_md_file_names() {
        assert_eq!(Harness::ClaudeCode.agents_md_file(), "CLAUDE.md");
        assert_eq!(Harness::Codex.agents_md_file(), "AGENTS.md");
        assert_eq!(Harness::OpenCode.agents_md_file(), "AGENTS.md");
    }

    #[test]
    fn detects_claude_code_via_claude_md() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "CLAUDE.md");
        assert_eq!(detect(tmp.path()), Some(Harness::ClaudeCode));
    }

    #[test]
    fn detects_claude_code_via_claude_dir() {
        let tmp = TempDir::new().unwrap();
        mkdir(tmp.path(), ".claude");
        assert_eq!(detect(tmp.path()), Some(Harness::ClaudeCode));
    }

    #[test]
    fn detects_codex_via_agents_md() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "AGENTS.md");
        assert_eq!(detect(tmp.path()), Some(Harness::Codex));
    }

    #[test]
    fn detects_codex_via_codex_dir() {
        let tmp = TempDir::new().unwrap();
        mkdir(tmp.path(), ".codex");
        assert_eq!(detect(tmp.path()), Some(Harness::Codex));
    }

    #[test]
    fn detects_opencode_via_opencode_json() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "opencode.json");
        assert_eq!(detect(tmp.path()), Some(Harness::OpenCode));
    }

    #[test]
    fn detects_opencode_via_opencode_jsonc() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "opencode.jsonc");
        assert_eq!(detect(tmp.path()), Some(Harness::OpenCode));
    }

    #[test]
    fn detects_opencode_via_opencode_dir() {
        let tmp = TempDir::new().unwrap();
        mkdir(tmp.path(), ".opencode");
        assert_eq!(detect(tmp.path()), Some(Harness::OpenCode));
    }

    #[test]
    fn priority_claude_beats_codex_and_opencode() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "CLAUDE.md");
        touch(tmp.path(), "AGENTS.md");
        touch(tmp.path(), "opencode.json");
        mkdir(tmp.path(), ".opencode");
        assert_eq!(detect(tmp.path()), Some(Harness::ClaudeCode));
    }

    #[test]
    fn priority_codex_beats_opencode() {
        let tmp = TempDir::new().unwrap();
        touch(tmp.path(), "AGENTS.md");
        touch(tmp.path(), "opencode.json");
        assert_eq!(detect(tmp.path()), Some(Harness::Codex));
    }

    #[test]
    fn returns_none_when_no_signals() {
        let tmp = TempDir::new().unwrap();
        // unrelated content should not match
        touch(tmp.path(), "README.md");
        touch(tmp.path(), "src/lib.rs");
        assert_eq!(detect(tmp.path()), None);
    }
}
