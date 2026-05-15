//! End-to-end install/remove integration tests for US-018.
//!
//! These tests exercise the `installer` module directly with in-memory
//! tarballs against the US-007 fake-project helpers (no network calls,
//! per AC). Each scenario is repeated across all three harnesses to
//! verify on-disk layout (`.claude/skills/…`, `.codex/skills/…`,
//! `.opencode/skills/…`) and the harness-specific agents.md target file
//! (`CLAUDE.md` / `AGENTS.md`).

mod common;

use std::fs;

use instinctagents::harness::{self, Harness};
use instinctagents::installer::{
    install_agents_md_from_tarball, install_skill_from_tarball, remove_agents_md, remove_skill,
    OverwriteAction, SkillInstallOutcome,
};
use instinctagents::state::ProjectState;

use common::{
    build_skill_tarball, fake_claude_project, fake_codex_project, fake_opencode_project,
};

const SKILL_NAME: &str = "demo";
const SKILL_VERSION: &str = "0.1.0";
const SKILL_SOURCE: &str = "https://example.test/demo";
const INTEG_NAME: &str = "integ";
const INTEG_VERSION: &str = "0.1.0";
const INTEG_SOURCE: &str = "https://example.test/integ";
const INTEG_SNIPPET: &str = "snippet body";

fn skill_tarball() -> Vec<u8> {
    build_skill_tarball(
        SKILL_NAME,
        &[
            ("SKILL.md", b"# demo skill body"),
            ("manifest.yml", b"name: demo\nversion: 0.1.0\n"),
        ],
    )
}

fn agents_md_tarball() -> Vec<u8> {
    build_skill_tarball(INTEG_NAME, &[("snippet.md", INTEG_SNIPPET.as_bytes())])
}

// -- install skill across the three harnesses --------------------------------

#[test]
fn install_skill_into_fake_claude_project() {
    let project = fake_claude_project();
    assert_eq!(harness::detect(project.path()), Some(Harness::ClaudeCode));

    let outcome = install_skill_from_tarball(
        project.path(),
        Harness::ClaudeCode,
        SKILL_NAME,
        SKILL_VERSION,
        SKILL_SOURCE,
        &skill_tarball(),
        OverwriteAction::Skip,
    )
    .expect("install skill");
    assert!(matches!(outcome, SkillInstallOutcome::Installed { .. }));

    let skill_md = project.path().join(".claude/skills/demo/SKILL.md");
    assert!(skill_md.is_file(), "expected .claude/skills/demo/SKILL.md");
    assert_eq!(
        fs::read_to_string(&skill_md).unwrap(),
        "# demo skill body"
    );

    let state = ProjectState::load(project.path()).unwrap();
    assert_eq!(state.installed_skills.len(), 1);
    assert_eq!(state.installed_skills[0].name, SKILL_NAME);
    assert_eq!(
        state.installed_skills[0].install_path.as_deref(),
        Some(".claude/skills/demo/")
    );
}

#[test]
fn install_skill_into_fake_codex_project() {
    let project = fake_codex_project();
    assert_eq!(harness::detect(project.path()), Some(Harness::Codex));

    install_skill_from_tarball(
        project.path(),
        Harness::Codex,
        SKILL_NAME,
        SKILL_VERSION,
        SKILL_SOURCE,
        &skill_tarball(),
        OverwriteAction::Skip,
    )
    .expect("install skill");

    assert!(project.path().join(".codex/skills/demo/SKILL.md").is_file());
    assert!(!project.path().join(".claude").exists());
    let state = ProjectState::load(project.path()).unwrap();
    assert_eq!(
        state.installed_skills[0].install_path.as_deref(),
        Some(".codex/skills/demo/")
    );
}

#[test]
fn install_skill_into_fake_opencode_project() {
    let project = fake_opencode_project();
    assert_eq!(harness::detect(project.path()), Some(Harness::OpenCode));

    install_skill_from_tarball(
        project.path(),
        Harness::OpenCode,
        SKILL_NAME,
        SKILL_VERSION,
        SKILL_SOURCE,
        &skill_tarball(),
        OverwriteAction::Skip,
    )
    .expect("install skill");

    assert!(project
        .path()
        .join(".opencode/skills/demo/SKILL.md")
        .is_file());
    let state = ProjectState::load(project.path()).unwrap();
    assert_eq!(
        state.installed_skills[0].install_path.as_deref(),
        Some(".opencode/skills/demo/")
    );
}

// -- install agents.md across the three harnesses ----------------------------
//
// The AC requires pre-existing user content to be preserved byte-for-byte
// after the injection. The fake_*_project helpers seed a one-line greeting
// in the harness instruction file; we capture it before injection and
// assert the same bytes are still present at the head of the file
// afterwards.

#[test]
fn install_agents_md_into_fake_claude_project_preserves_user_content() {
    let project = fake_claude_project();
    let target = project.path().join("CLAUDE.md");
    let pre_existing = fs::read_to_string(&target).expect("seed CLAUDE.md present");
    assert!(!pre_existing.is_empty(), "fake_claude_project seeds content");

    install_agents_md_from_tarball(
        project.path(),
        Harness::ClaudeCode,
        INTEG_NAME,
        INTEG_VERSION,
        INTEG_SOURCE,
        &agents_md_tarball(),
    )
    .expect("install agents.md");

    let after = fs::read_to_string(&target).unwrap();
    assert!(
        after.starts_with(&pre_existing),
        "pre-existing user content must be preserved byte-for-byte at the head of the file"
    );
    assert!(after.contains("<!-- instinctagents:start:integ -->"));
    assert!(after.contains(INTEG_SNIPPET));
    assert!(after.contains("<!-- instinctagents:end:integ -->"));

    let state = ProjectState::load(project.path()).unwrap();
    assert_eq!(state.installed_agents_md.len(), 1);
    assert_eq!(state.installed_agents_md[0].name, INTEG_NAME);
    assert_eq!(
        state.installed_agents_md[0].delimiter_id.as_deref(),
        Some(INTEG_NAME)
    );
}

#[test]
fn install_agents_md_into_fake_codex_project_preserves_user_content() {
    let project = fake_codex_project();
    let target = project.path().join("AGENTS.md");
    let pre_existing = fs::read_to_string(&target).expect("seed AGENTS.md present");

    install_agents_md_from_tarball(
        project.path(),
        Harness::Codex,
        INTEG_NAME,
        INTEG_VERSION,
        INTEG_SOURCE,
        &agents_md_tarball(),
    )
    .expect("install agents.md");

    let after = fs::read_to_string(&target).unwrap();
    assert!(after.starts_with(&pre_existing));
    assert!(after.contains("<!-- instinctagents:start:integ -->"));
    assert!(after.contains(INTEG_SNIPPET));
    assert!(
        !project.path().join("CLAUDE.md").exists(),
        "codex install must NOT touch CLAUDE.md"
    );
}

#[test]
fn install_agents_md_into_fake_opencode_project_preserves_user_content() {
    let project = fake_opencode_project();
    // fake_opencode_project doesn't seed AGENTS.md — write some user
    // content so the byte-for-byte preservation assertion is meaningful.
    let target = project.path().join("AGENTS.md");
    let pre_existing = "# Existing OpenCode agents.md\n\nUser prose unaffected.\n";
    fs::write(&target, pre_existing).unwrap();

    install_agents_md_from_tarball(
        project.path(),
        Harness::OpenCode,
        INTEG_NAME,
        INTEG_VERSION,
        INTEG_SOURCE,
        &agents_md_tarball(),
    )
    .expect("install agents.md");

    let after = fs::read_to_string(&target).unwrap();
    assert!(after.starts_with(pre_existing));
    assert!(after.contains("<!-- instinctagents:start:integ -->"));
    assert!(after.contains(INTEG_SNIPPET));
}

// -- remove ------------------------------------------------------------------

#[test]
fn remove_skill_deletes_folder_and_updates_state() {
    let project = fake_claude_project();

    install_skill_from_tarball(
        project.path(),
        Harness::ClaudeCode,
        SKILL_NAME,
        SKILL_VERSION,
        SKILL_SOURCE,
        &skill_tarball(),
        OverwriteAction::Skip,
    )
    .unwrap();
    assert!(project.path().join(".claude/skills/demo").is_dir());
    assert_eq!(
        ProjectState::load(project.path()).unwrap().installed_skills.len(),
        1
    );

    remove_skill(project.path(), Harness::ClaudeCode, SKILL_NAME).unwrap();
    assert!(!project.path().join(".claude/skills/demo").exists());
    assert!(ProjectState::load(project.path())
        .unwrap()
        .installed_skills
        .is_empty());
}

#[test]
fn remove_agents_md_drops_delimited_block_and_preserves_user_content() {
    let project = fake_claude_project();
    let target = project.path().join("CLAUDE.md");
    let pre_existing = fs::read_to_string(&target).unwrap();

    install_agents_md_from_tarball(
        project.path(),
        Harness::ClaudeCode,
        INTEG_NAME,
        INTEG_VERSION,
        INTEG_SOURCE,
        &agents_md_tarball(),
    )
    .unwrap();

    let found =
        remove_agents_md(project.path(), Harness::ClaudeCode, INTEG_NAME).unwrap();
    assert!(found, "block must be found and removed");

    let after = fs::read_to_string(&target).unwrap();
    assert!(
        after.starts_with(&pre_existing),
        "pre-existing user content must remain at the head of the file after removal"
    );
    assert!(!after.contains("<!-- instinctagents:start:integ -->"));
    assert!(!after.contains(INTEG_SNIPPET));
    assert!(ProjectState::load(project.path())
        .unwrap()
        .installed_agents_md
        .is_empty());
}

// -- conflict resolution: skip vs overwrite ----------------------------------
//
// These cover the AC: "install when folder exists with --force=skip skips;
// with --force=overwrite overwrites". At the installer layer this is
// `OverwriteAction::Skip` vs `Overwrite`; the CLI gate (and the future
// flag wiring per progress.txt's US-016 note) sits above this contract.

#[test]
fn install_with_target_exists_skip_leaves_original_alone() {
    let project = fake_claude_project();
    let target = project.path().join(".claude/skills/demo");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("KEEP"), b"pre-existing user file").unwrap();

    let outcome = install_skill_from_tarball(
        project.path(),
        Harness::ClaudeCode,
        SKILL_NAME,
        SKILL_VERSION,
        SKILL_SOURCE,
        &skill_tarball(),
        OverwriteAction::Skip,
    )
    .unwrap();
    assert_eq!(outcome, SkillInstallOutcome::Skipped);

    assert!(target.join("KEEP").is_file(), "skip preserves the existing folder");
    assert!(
        !target.join("SKILL.md").exists(),
        "skip must not extract on top of the existing folder"
    );
    assert!(
        ProjectState::load(project.path())
            .unwrap()
            .installed_skills
            .is_empty(),
        "skip leaves state untouched"
    );
}

#[test]
fn install_with_target_exists_overwrite_replaces_existing() {
    let project = fake_claude_project();
    let target = project.path().join(".claude/skills/demo");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("OLD"), b"stale data").unwrap();

    let outcome = install_skill_from_tarball(
        project.path(),
        Harness::ClaudeCode,
        SKILL_NAME,
        SKILL_VERSION,
        SKILL_SOURCE,
        &skill_tarball(),
        OverwriteAction::Overwrite,
    )
    .unwrap();
    assert!(matches!(outcome, SkillInstallOutcome::Installed { .. }));

    assert!(target.join("SKILL.md").is_file(), "overwrite extracts the new tarball");
    assert!(
        !target.join("OLD").exists(),
        "overwrite removes the previous folder first"
    );
    let state = ProjectState::load(project.path()).unwrap();
    assert_eq!(state.installed_skills.len(), 1);
    assert_eq!(state.installed_skills[0].name, SKILL_NAME);
}
