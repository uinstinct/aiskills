//! Command-line argument parsing (US-016).
//!
//! Bare `instinctagents` launches the TUI on the Add tab. With
//! `--non-interactive`, the binary executes a single `add` / `remove`
//! subcommand without entering the TUI. `--version` and `--help` are wired
//! up by clap. `--force` overrides compatibility checks for the
//! non-interactive install path; `--verbose` enables stderr debug logging
//! (currently surfaces the silent network error from the update check —
//! see US-015's contract).

use std::path::Path;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::catalog::{self, CatalogEntry};
use crate::harness::{self, Harness};
use crate::installer::{self, InstallError, OverwriteAction, SkillInstallOutcome};
use crate::state::ProjectState;

#[derive(Parser, Debug)]
#[command(
    name = "instinctagents",
    version,
    about = "Install skills and agents.md snippets from the instinctagents registry.",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Override compatibility checks (allow installing a skill marked
    /// incompatible with the detected harness).
    #[arg(long, global = true)]
    pub force: bool,

    /// Skip the TUI and execute the subcommand directly.
    #[arg(long)]
    pub non_interactive: bool,

    /// Verbose debug logging to stderr.
    #[arg(long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Install a skill or agents.md integration by name.
    Add(ItemArgs),
    /// Uninstall a skill or agents.md integration by name.
    Remove(ItemArgs),
}

#[derive(Args, Debug)]
pub struct ItemArgs {
    /// Catalog entry name (matches the folder name in skills/ or agents.md/).
    #[arg(long)]
    pub name: String,

    /// Required only when a skill AND an agents.md integration share the
    /// same name. Otherwise inferred from the catalog (for `add`) or from
    /// the project's `.instinctagents` (for `remove`).
    #[arg(long = "type", value_enum)]
    pub item_type: Option<ItemType>,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Skill,
    #[value(name = "agents-md")]
    AgentsMd,
}

#[derive(Debug)]
pub enum CliError {
    NoSubcommand,
    UnknownItem { name: String },
    AmbiguousItem { name: String },
    NoHarness,
    Incompatible { name: String, allowed: Vec<String> },
    NotInstalled { name: String },
    Install(InstallError),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::NoSubcommand => write!(
                f,
                "--non-interactive requires a subcommand (add or remove)"
            ),
            CliError::UnknownItem { name } => {
                write!(f, "no catalog entry named '{name}'")
            }
            CliError::AmbiguousItem { name } => write!(
                f,
                "'{name}' matches both a skill and an agents.md integration; pass --type skill or --type agents-md"
            ),
            CliError::NoHarness => write!(
                f,
                "no harness detected in current directory (need CLAUDE.md/.claude/, AGENTS.md/.codex/, or opencode.json/.opencode/)"
            ),
            CliError::Incompatible { name, allowed } => write!(
                f,
                "'{name}' is not compatible with the detected harness (allowed: {}); pass --force to install anyway",
                allowed.join(", ")
            ),
            CliError::NotInstalled { name } => {
                write!(f, "'{name}' is not currently installed in this project")
            }
            CliError::Install(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<InstallError> for CliError {
    fn from(e: InstallError) -> Self {
        CliError::Install(e)
    }
}

/// Outcome of a single non-interactive operation. Returned so the caller
/// (main.rs) can emit a human-readable line on success.
#[derive(Debug)]
pub enum CliOutcome {
    SkillInstalled { name: String, install_path: String },
    SkillSkipped { name: String },
    SkillRenamed { name: String, new_name: String },
    AgentsMdInstalled { name: String, target_file: String },
    SkillRemoved { name: String },
    AgentsMdRemoved { name: String, block_found: bool },
}

impl std::fmt::Display for CliOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliOutcome::SkillInstalled { name, install_path } => {
                write!(f, "installed skill '{name}' to {install_path}")
            }
            CliOutcome::SkillSkipped { name } => write!(
                f,
                "skill '{name}' already present; skipped (pass --force to overwrite when US-018 lands)"
            ),
            CliOutcome::SkillRenamed { name, new_name } => {
                write!(f, "installed skill '{name}' as '{new_name}' (target existed)")
            }
            CliOutcome::AgentsMdInstalled { name, target_file } => {
                write!(f, "injected agents.md block '{name}' into {target_file}")
            }
            CliOutcome::SkillRemoved { name } => write!(f, "removed skill '{name}'"),
            CliOutcome::AgentsMdRemoved { name, block_found } => {
                if *block_found {
                    write!(f, "removed agents.md block '{name}'")
                } else {
                    write!(
                        f,
                        "agents.md entry '{name}' removed from state (delimiter block was missing)"
                    )
                }
            }
        }
    }
}

fn lookup_catalog(name: &str) -> (Option<&'static CatalogEntry>, Option<&'static CatalogEntry>) {
    let skill = catalog::SKILLS.iter().find(|e| e.name == name);
    let agents_md = catalog::AGENTS_MD.iter().find(|e| e.name == name);
    (skill, agents_md)
}

fn lookup_state(name: &str, state: &ProjectState) -> (bool, bool) {
    let skill = state.installed_skills.iter().any(|i| i.name == name);
    let agents_md = state.installed_agents_md.iter().any(|i| i.name == name);
    (skill, agents_md)
}

/// Resolve the item type for a given name. Returns the explicit `requested`
/// type when it disambiguates, otherwise infers from the source's
/// (skill, agents-md) presence pair.
pub fn resolve_item_type(
    name: &str,
    requested: Option<ItemType>,
    has_skill: bool,
    has_agents_md: bool,
) -> Result<ItemType, CliError> {
    if let Some(t) = requested {
        return Ok(t);
    }
    match (has_skill, has_agents_md) {
        (true, false) => Ok(ItemType::Skill),
        (false, true) => Ok(ItemType::AgentsMd),
        (true, true) => Err(CliError::AmbiguousItem {
            name: name.to_string(),
        }),
        (false, false) => Err(CliError::UnknownItem {
            name: name.to_string(),
        }),
    }
}

/// Check whether a catalog entry's `harness_compatibility` allows the
/// detected harness. Mirrors the rule used by `tui.rs` for Add-tab row
/// dimming.
pub fn is_compatible(entry: &CatalogEntry, harness: Harness) -> bool {
    if entry.harness_compatibility.is_empty() {
        return true;
    }
    let tag = match harness {
        Harness::ClaudeCode => "claude-code",
        Harness::Codex => "codex",
        Harness::OpenCode => "opencode",
    };
    entry.harness_compatibility.contains(&tag)
}

/// Gate the install path on harness compatibility. Returns `Ok(())` when
/// the install should proceed (`force` bypasses) and
/// [`CliError::Incompatible`] otherwise. Shared between the skill and
/// agents.md branches of [`run_add`] so the AC "with --force succeeds;
/// without --force is blocked" has a single chokepoint.
pub fn check_compat(
    entry: &CatalogEntry,
    harness: Harness,
    force: bool,
) -> Result<(), CliError> {
    if force || is_compatible(entry, harness) {
        return Ok(());
    }
    Err(CliError::Incompatible {
        name: entry.name.to_string(),
        allowed: entry
            .harness_compatibility
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    })
}

/// Execute the non-interactive path. `project_root` is the working
/// directory the binary was launched from.
pub fn run_non_interactive(cli: &Cli, project_root: &Path) -> Result<CliOutcome, CliError> {
    let command = cli.command.as_ref().ok_or(CliError::NoSubcommand)?;
    match command {
        Command::Add(args) => run_add(args, project_root, cli.force),
        Command::Remove(args) => run_remove(args, project_root),
    }
}

fn run_add(args: &ItemArgs, project_root: &Path, force: bool) -> Result<CliOutcome, CliError> {
    let harness = harness::detect(project_root).ok_or(CliError::NoHarness)?;

    let (catalog_skill, catalog_agents_md) = lookup_catalog(&args.name);
    let item_type = resolve_item_type(
        &args.name,
        args.item_type,
        catalog_skill.is_some(),
        catalog_agents_md.is_some(),
    )?;

    match item_type {
        ItemType::Skill => {
            let entry = catalog::SKILLS
                .iter()
                .find(|e| e.name == args.name)
                .ok_or_else(|| CliError::UnknownItem {
                    name: args.name.clone(),
                })?;

            check_compat(entry, harness, force)?;

            let outcome = installer::fetch_and_install_skill(
                project_root,
                harness,
                entry.name,
                entry.version,
                entry.source_path,
                OverwriteAction::Skip,
            )?;
            Ok(match outcome {
                SkillInstallOutcome::Installed { install_path } => CliOutcome::SkillInstalled {
                    name: args.name.clone(),
                    install_path,
                },
                SkillInstallOutcome::Skipped => CliOutcome::SkillSkipped {
                    name: args.name.clone(),
                },
                SkillInstallOutcome::Renamed { new_name, .. } => CliOutcome::SkillRenamed {
                    name: args.name.clone(),
                    new_name,
                },
            })
        }
        ItemType::AgentsMd => {
            let entry = catalog::AGENTS_MD
                .iter()
                .find(|e| e.name == args.name)
                .ok_or_else(|| CliError::UnknownItem {
                    name: args.name.clone(),
                })?;

            check_compat(entry, harness, force)?;

            installer::fetch_and_install_agents_md(
                project_root,
                harness,
                entry.name,
                entry.version,
                entry.source_path,
            )?;
            Ok(CliOutcome::AgentsMdInstalled {
                name: args.name.clone(),
                target_file: harness.agents_md_file().to_string(),
            })
        }
    }
}

fn run_remove(args: &ItemArgs, project_root: &Path) -> Result<CliOutcome, CliError> {
    let harness = harness::detect(project_root).ok_or(CliError::NoHarness)?;
    let state = ProjectState::load(project_root).map_err(InstallError::from)?;

    let (has_skill, has_agents_md) = lookup_state(&args.name, &state);
    if !has_skill && !has_agents_md {
        return Err(CliError::NotInstalled {
            name: args.name.clone(),
        });
    }

    let item_type = resolve_item_type(&args.name, args.item_type, has_skill, has_agents_md)?;

    match item_type {
        ItemType::Skill => {
            installer::remove_skill(project_root, harness, &args.name)?;
            Ok(CliOutcome::SkillRemoved {
                name: args.name.clone(),
            })
        }
        ItemType::AgentsMd => {
            let found = installer::remove_agents_md(project_root, harness, &args.name)?;
            Ok(CliOutcome::AgentsMdRemoved {
                name: args.name.clone(),
                block_found: found,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    use crate::state::InstalledItem;

    fn parse(argv: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(argv)
    }

    #[test]
    fn bare_invocation_parses_with_no_command() {
        let cli = parse(&["instinctagents"]).expect("bare invocation must parse");
        assert!(cli.command.is_none());
        assert!(!cli.non_interactive);
        assert!(!cli.force);
        assert!(!cli.verbose);
    }

    #[test]
    fn version_flag_intercepts() {
        let err = parse(&["instinctagents", "--version"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
    }

    #[test]
    fn help_flag_intercepts() {
        let err = parse(&["instinctagents", "--help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn add_subcommand_parses_minimal_args() {
        let cli = parse(&[
            "instinctagents",
            "--non-interactive",
            "add",
            "--name",
            "example-skill",
        ])
        .expect("must parse");
        assert!(cli.non_interactive);
        match cli.command {
            Some(Command::Add(a)) => {
                assert_eq!(a.name, "example-skill");
                assert!(a.item_type.is_none());
            }
            other => panic!("expected Add, got {other:?}"),
        }
    }

    #[test]
    fn add_subcommand_parses_type_skill() {
        let cli = parse(&[
            "instinctagents",
            "--non-interactive",
            "add",
            "--name",
            "x",
            "--type",
            "skill",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Add(a)) => assert_eq!(a.item_type, Some(ItemType::Skill)),
            other => panic!("expected Add, got {other:?}"),
        }
    }

    #[test]
    fn add_subcommand_parses_type_agents_md_with_hyphen() {
        let cli = parse(&[
            "instinctagents",
            "--non-interactive",
            "add",
            "--name",
            "x",
            "--type",
            "agents-md",
        ])
        .unwrap();
        match cli.command {
            Some(Command::Add(a)) => assert_eq!(a.item_type, Some(ItemType::AgentsMd)),
            other => panic!("expected Add, got {other:?}"),
        }
    }

    #[test]
    fn remove_subcommand_parses() {
        let cli = parse(&[
            "instinctagents",
            "--non-interactive",
            "remove",
            "--name",
            "foo",
        ])
        .unwrap();
        assert!(matches!(cli.command, Some(Command::Remove(_))));
    }

    #[test]
    fn force_and_verbose_are_global_and_parse_before_or_after_subcommand() {
        let a = parse(&[
            "instinctagents",
            "--force",
            "--verbose",
            "--non-interactive",
            "add",
            "--name",
            "x",
        ])
        .unwrap();
        assert!(a.force);
        assert!(a.verbose);

        let b = parse(&[
            "instinctagents",
            "--non-interactive",
            "add",
            "--name",
            "x",
            "--force",
        ])
        .unwrap();
        assert!(b.force);
    }

    #[test]
    fn missing_name_is_rejected() {
        let err = parse(&["instinctagents", "--non-interactive", "add"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn unknown_type_value_is_rejected() {
        let err = parse(&[
            "instinctagents",
            "--non-interactive",
            "add",
            "--name",
            "x",
            "--type",
            "bogus",
        ])
        .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn run_non_interactive_without_subcommand_errors() {
        let cli = Cli {
            force: false,
            non_interactive: true,
            verbose: false,
            command: None,
        };
        let tmp = tempfile::TempDir::new().unwrap();
        let err = run_non_interactive(&cli, tmp.path()).unwrap_err();
        assert!(matches!(err, CliError::NoSubcommand));
    }

    #[test]
    fn resolve_item_type_explicit_wins() {
        let t = resolve_item_type("foo", Some(ItemType::Skill), true, true).unwrap();
        assert_eq!(t, ItemType::Skill);
    }

    #[test]
    fn resolve_item_type_infers_skill() {
        let t = resolve_item_type("foo", None, true, false).unwrap();
        assert_eq!(t, ItemType::Skill);
    }

    #[test]
    fn resolve_item_type_infers_agents_md() {
        let t = resolve_item_type("foo", None, false, true).unwrap();
        assert_eq!(t, ItemType::AgentsMd);
    }

    #[test]
    fn resolve_item_type_ambiguous_requires_explicit() {
        let err = resolve_item_type("foo", None, true, true).unwrap_err();
        assert!(matches!(err, CliError::AmbiguousItem { .. }));
    }

    #[test]
    fn resolve_item_type_unknown_when_neither_matches() {
        let err = resolve_item_type("foo", None, false, false).unwrap_err();
        assert!(matches!(err, CliError::UnknownItem { .. }));
    }

    #[test]
    fn is_compatible_empty_list_is_universal() {
        let entry = CatalogEntry {
            name: "x",
            description: "",
            version: "0.0.0",
            harness_compatibility: &[],
            source_path: "",
        };
        assert!(is_compatible(&entry, Harness::ClaudeCode));
        assert!(is_compatible(&entry, Harness::Codex));
        assert!(is_compatible(&entry, Harness::OpenCode));
    }

    #[test]
    fn check_compat_blocks_without_force_for_incompatible_entry() {
        let entry = CatalogEntry {
            name: "claude-only",
            description: "",
            version: "0.1.0",
            harness_compatibility: &["claude-code"],
            source_path: "",
        };
        let err = check_compat(&entry, Harness::Codex, false).unwrap_err();
        match err {
            CliError::Incompatible { name, allowed } => {
                assert_eq!(name, "claude-only");
                assert_eq!(allowed, vec!["claude-code".to_string()]);
            }
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }

    #[test]
    fn check_compat_bypasses_with_force_for_incompatible_entry() {
        let entry = CatalogEntry {
            name: "claude-only",
            description: "",
            version: "0.1.0",
            harness_compatibility: &["claude-code"],
            source_path: "",
        };
        check_compat(&entry, Harness::Codex, true).expect("--force must bypass the gate");
    }

    #[test]
    fn check_compat_universal_entry_passes_without_force() {
        let entry = CatalogEntry {
            name: "anywhere",
            description: "",
            version: "0.1.0",
            harness_compatibility: &[],
            source_path: "",
        };
        check_compat(&entry, Harness::ClaudeCode, false).unwrap();
        check_compat(&entry, Harness::Codex, false).unwrap();
        check_compat(&entry, Harness::OpenCode, false).unwrap();
    }

    #[test]
    fn is_compatible_respects_allow_list() {
        let entry = CatalogEntry {
            name: "x",
            description: "",
            version: "0.0.0",
            harness_compatibility: &["claude-code"],
            source_path: "",
        };
        assert!(is_compatible(&entry, Harness::ClaudeCode));
        assert!(!is_compatible(&entry, Harness::Codex));
        assert!(!is_compatible(&entry, Harness::OpenCode));
    }

    #[test]
    fn remove_unknown_item_reports_not_installed() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "").unwrap();
        let cli = Cli {
            force: false,
            non_interactive: true,
            verbose: false,
            command: Some(Command::Remove(ItemArgs {
                name: "ghost".to_string(),
                item_type: None,
            })),
        };
        let err = run_non_interactive(&cli, tmp.path()).unwrap_err();
        assert!(matches!(err, CliError::NotInstalled { .. }));
    }

    #[test]
    fn add_with_no_harness_errors() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cli = Cli {
            force: false,
            non_interactive: true,
            verbose: false,
            command: Some(Command::Add(ItemArgs {
                name: "example-skill".to_string(),
                item_type: None,
            })),
        };
        let err = run_non_interactive(&cli, tmp.path()).unwrap_err();
        assert!(matches!(err, CliError::NoHarness));
    }

    #[test]
    fn add_unknown_name_errors() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "").unwrap();
        let cli = Cli {
            force: false,
            non_interactive: true,
            verbose: false,
            command: Some(Command::Add(ItemArgs {
                name: "does-not-exist".to_string(),
                item_type: None,
            })),
        };
        let err = run_non_interactive(&cli, tmp.path()).unwrap_err();
        assert!(matches!(err, CliError::UnknownItem { .. }));
    }

    #[test]
    fn lookup_state_finds_both_kinds_under_one_name() {
        let mut state = ProjectState::default();
        state.installed_skills.push(InstalledItem {
            name: "shared".into(),
            version: "1.0.0".into(),
            source_url: "".into(),
            install_path: Some(".claude/skills/shared/".into()),
            delimiter_id: None,
        });
        state.installed_agents_md.push(InstalledItem {
            name: "shared".into(),
            version: "1.0.0".into(),
            source_url: "".into(),
            install_path: None,
            delimiter_id: Some("shared".into()),
        });
        let (has_skill, has_agents_md) = lookup_state("shared", &state);
        assert!(has_skill && has_agents_md);
        let err =
            resolve_item_type("shared", None, has_skill, has_agents_md).unwrap_err();
        assert!(matches!(err, CliError::AmbiguousItem { .. }));
    }
}
