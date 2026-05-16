# PRD: Local Scaffolding, `assets/` Reorg, and Codebuff Harness Support

## Introduction

This PRD bundles three related changes to the `instinctagents` registry and tooling:

1. **Local scaffold commands** — let maintainers create a new skill or `agents.md` integration entry from scratch via the internal Python tool, without needing a GitHub source URL. The CLI prompts the maintainer for all required manifest fields and writes a minimal entry into the registry.
2. **`assets/` reorganization** — move the two top-level registry directories (`skills/` and `agents.md/`) under a single `assets/` parent, so the registry's content is cleanly separated from source code, docs, and scripts at the repo root.
3. **Codebuff harness support** — add Codebuff (`https://github.com/CodebuffAI/codebuff`) as a fourth supported harness alongside Claude Code, Codex, and OpenCode, with its own detection signals and install paths.

These three changes are coupled because the harness list (US-008 family), manifest validation (US-007 family), build script (US-005), internal Python ingest scripts (`src/internal/`), and the external Rust installer all reference the directory layout and the set of supported harnesses. A single coordinated PR avoids re-touching the same files three times.

## Goals

- Allow maintainers to bootstrap a brand-new local-only skill or `agents.md` integration entry via a single command (`skill-new <name>` / `agents-md-new <name>`), with required fields collected interactively.
- Relocate `skills/` → `assets/skills/` and `agents.md/` → `assets/agents.md/` without breaking the build, the installer, or any existing slash commands.
- Add Codebuff as a recognized harness: detected via project signals, installable skills go to `.agents/skills/<name>/`, and `agents.md` snippets are injected into `AGENTS.md` at the project root.
- Preserve the existing public surface — TUI flow, harness detection priority for current harnesses, `mapping.yml` schema, manifest schema — so the changes are additive from a user's perspective.

## User Stories

### US-001: Add `skill-new <name>` scaffold command
**Description:** As a maintainer, I want to run `skill-new my-skill` and have the CLI interactively prompt for the manifest fields, so I can start a new skill entry without copying a GitHub URL.

**Acceptance Criteria:**
- [ ] New `src/internal/skill_new.py` module exposes a `main(argv)` entrypoint matching the style of `skill_add.py`.
- [ ] CLI prompts for: `description` (required, non-empty), `version` (default `0.1.0`, must parse as semver), `harness_compatibility` (multi-select among `claude-code`, `codex`, `opencode`, `codebuff`; empty = all). Use the existing `prompt_text` / `prompt_choice` helpers in `lib.py`; add a `prompt_multi_choice` if needed.
- [ ] On confirmation, writes `assets/skills/<name>/manifest.yml` (with the entered fields, `name` matching the sanitized argument) and a placeholder `assets/skills/<name>/SKILL.md` containing `# <name>` and a single TODO line.
- [ ] Appends a new entry to `mapping.yml` under `installed_skills`, with `source_url: local` and `install_path: assets/skills/<name>/`.
- [ ] Refuses to clobber an existing `assets/skills/<name>/` or mapping entry without `--force`.
- [ ] A `.claude/commands/skill-new.md` (and `.codex` / `.opencode` equivalents) slash-command wrapper delegates to `uv run --project src/internal python -m skill_new`.
- [ ] Typecheck / lint passes for the Python module (`ruff` / project linters used by `src/internal/`).

### US-002: Add `agents-md-new <name>` scaffold command
**Description:** As a maintainer, I want the same interactive scaffold for `agents.md` integrations so I can author a new snippet locally.

**Acceptance Criteria:**
- [ ] New `src/internal/agents_md_new.py` with the same shape as US-001.
- [ ] Prompts for `description`, `version`, `harness_compatibility` (same set as US-001).
- [ ] Writes `assets/agents.md/<name>/manifest.yml` and a placeholder `assets/agents.md/<name>/snippet.md` containing `# <name>` and one TODO line.
- [ ] Adds an entry to `mapping.yml` under `installed_agents_md` with `source_url: local` and `install_path: assets/agents.md/<name>/`.
- [ ] `--force` flag and existence checks parallel `skill_add.py`'s behavior.
- [ ] Slash-command wrappers exist for all three (now four) harnesses.
- [ ] Lint / typecheck passes.

### US-003: Relocate `skills/` and `agents.md/` under `assets/`
**Description:** As a maintainer, I want the registry content under `assets/` so the repo root is clearer.

**Acceptance Criteria:**
- [ ] `skills/` directory moved to `assets/skills/` via `git mv` (preserving history); same for `agents.md/` → `assets/agents.md/`.
- [ ] All existing entries (`grill-me`, `storytelling-mastery-skill`, `write-a-skill`, `karpathy-guidelines`) move with their full folder contents.
- [ ] `mapping.yml` `install_path` values are rewritten: `skills/foo/` → `assets/skills/foo/`, `agents.md/foo/` → `assets/agents.md/foo/`.
- [ ] No references to the old top-level paths remain in the repo (verified by `grep -rn '^skills/\|^agents\.md/\|"skills/\|"agents\.md/' --exclude-dir=target --exclude-dir=.git`).
- [ ] `docs/manifest-schema.md` and `docs/mapping-schema.md` are updated to reference the new paths in every example and prose.
- [ ] `README.md` "Repository layout" section reflects the new tree.

### US-004: Update `build.rs` to bake catalog from `assets/`
**Description:** As a developer, I need the build script to read manifests from the new `assets/` paths so the offline catalog still bakes correctly.

**Acceptance Criteria:**
- [ ] `src/external/build.rs` reads from `assets/skills/*/manifest.yml` and `assets/agents.md/*/manifest.yml` instead of the old paths.
- [ ] The build still fails closed on missing/malformed manifests, with the new path in the error message.
- [ ] The manifest validation rule "`name` equals parent folder's basename" still holds under the new layout (basename is unchanged; only the parent prefix moved).
- [ ] `cargo build` succeeds on a clean checkout post-move.
- [ ] `cargo test` passes (existing build.rs / catalog tests).

### US-005: Update internal Python scripts to write into `assets/`
**Description:** As a maintainer, I need `skill_add` / `agents_md_add` / `skill_remove` / `agents_md_remove` / `skill_list` / `agents_md_list` to operate on the new `assets/` paths.

**Acceptance Criteria:**
- [ ] Path constants in `src/internal/lib.py` (e.g. `entry_dir`, `install_path_for`) emit `assets/skills/<name>/` and `assets/agents.md/<name>/`.
- [ ] All six scripts (`skill_add`, `agents_md_add`, `skill_new`, `agents_md_new`, `skill_remove`, `agents_md_remove`) and the two list scripts work end-to-end on the new layout.
- [ ] Existing pytest / smoke tests under `src/internal/` (if any) pass; if none exist, add a minimal end-to-end test that scaffolds and removes one entry of each kind.
- [ ] Lint passes.

### US-006: Add `Codebuff` variant to harness enum and detection
**Description:** As a user of Codebuff, I want `instinctagents` to detect my project as Codebuff so I get the right install paths.

**Acceptance Criteria:**
- [ ] `src/external/src/harness.rs`: add `Harness::Codebuff` enum variant.
- [ ] `target_folder()` returns `.agents/skills/` for `Codebuff`.
- [ ] `agents_md_file()` returns `AGENTS.md` for `Codebuff` (Codebuff reads both `AGENTS.md` and `CLAUDE.md`; `AGENTS.md` is the agnostic choice).
- [ ] `detect()` checks Codebuff **last**, after ClaudeCode → Codex → OpenCode. Signals: `.agents/` directory OR `knowledge.md` file at the project root.
- [ ] New unit tests in `harness.rs` for: detection via `.agents/` dir, detection via `knowledge.md`, priority (a project with `CLAUDE.md` + `.agents/` resolves as ClaudeCode; a project with `AGENTS.md` + `.agents/` resolves as Codex), and `target_folder` / `agents_md_file` values for Codebuff.
- [ ] All existing `harness.rs` tests still pass.

### US-007: Extend manifest `harness_compatibility` to accept `codebuff`
**Description:** As a maintainer, I want to mark a skill or integration as Codebuff-compatible in its manifest.

**Acceptance Criteria:**
- [ ] `build.rs` manifest validation accepts `codebuff` as a fourth valid `harness_compatibility` value (alongside `claude-code`, `codex`, `opencode`).
- [ ] `docs/manifest-schema.md` lists `codebuff` in the allowed values section.
- [ ] An invalid value still fails the build with a clear error.
- [ ] At least one test (in `build.rs` or `catalog.rs`) covers a manifest with `harness_compatibility: [codebuff]`.

### US-008: Wire Codebuff into the installer and TUI compatibility filter
**Description:** As a Codebuff user, I want the TUI Add tab to show me skills tagged for Codebuff (or untagged) and to install them into `.agents/skills/`.

**Acceptance Criteria:**
- [ ] `installer.rs` uses `Harness::Codebuff::target_folder()` and `agents_md_file()` when installing; no Codebuff-specific branching outside the enum.
- [ ] The TUI Add tab's dim/disable filter (US-012 in the original PRD) treats Codebuff the same as the other harnesses: rows with non-empty `harness_compatibility` that excludes the detected `codebuff` are dimmed.
- [ ] `--force` (US-016 equivalent) still allows installing an incompatible entry into a Codebuff project.
- [ ] Manual verification: running the built binary inside a fixture directory containing `.agents/` reports "Codebuff detected" (or equivalent) in the TUI status area.

### US-009: Update README and slash-command docs
**Description:** As a new contributor, I want the README and slash-command reference to mention Codebuff and the new scaffold commands.

**Acceptance Criteria:**
- [ ] `README.md` "Supports the **Claude Code**, **Codex**, and **OpenCode** harnesses" line includes **Codebuff**.
- [ ] `README.md` harness-detection paragraph mentions the `.agents/` / `knowledge.md` signals.
- [ ] `README.md` "For maintainers" section documents `/skill-new` and `/agents-md-new` next to `/skill-add` / `/agents-md-add`.
- [ ] `README.md` "Repository layout" tree shows `assets/skills/` and `assets/agents.md/`.

## Functional Requirements

- **FR-1:** A new Python module `src/internal/skill_new.py` accepts a single positional argument `<name>` and a `--force` flag, prompts the user for `description` (required), `version` (default `0.1.0`, semver-validated), and `harness_compatibility` (multi-select from `claude-code` / `codex` / `opencode` / `codebuff`; empty = all), and writes a registry entry under `assets/skills/<name>/`.
- **FR-2:** A new Python module `src/internal/agents_md_new.py` mirrors FR-1 for `agents.md` integrations, writing under `assets/agents.md/<name>/` with a `snippet.md` placeholder.
- **FR-3:** Both new modules MUST refuse to overwrite an existing entry (folder on disk OR mapping entry) unless `--force` is passed.
- **FR-4:** Both new modules MUST update `mapping.yml` with `source_url: local` and the new `install_path`.
- **FR-5:** The registry content directories MUST live at `assets/skills/` and `assets/agents.md/`. No code in the repo may hard-code the old top-level paths.
- **FR-6:** `build.rs` MUST scan `assets/skills/*/manifest.yml` and `assets/agents.md/*/manifest.yml`; missing manifests or invalid `harness_compatibility` values MUST stop the build with the path in the error.
- **FR-7:** `Harness::Codebuff` MUST be detected when `.agents/` directory OR `knowledge.md` file exists at the project root, with priority BELOW ClaudeCode, Codex, and OpenCode.
- **FR-8:** `Harness::Codebuff::target_folder()` MUST return `.agents/skills/`; `Harness::Codebuff::agents_md_file()` MUST return `AGENTS.md`.
- **FR-9:** The manifest schema MUST accept `codebuff` as a valid `harness_compatibility` value; any other value MUST remain a validation error.
- **FR-10:** Slash-command wrappers (`/skill-new`, `/agents-md-new`) MUST exist under `.claude/commands/`, `.codex/skills/`, and `.opencode/commands/`, delegating to `uv run --project src/internal python -m skill_new` (and the agents.md equivalent).

## Non-Goals

- No support for installing arbitrary local filesystem **paths** through `skill_add` / `agents_md_add` (the scaffold commands handle the "no source URL" case; mixing a third URL kind into the existing adders is out of scope).
- No support for generating Codebuff-native TypeScript agent files in `.agents/types/`. Codebuff skills are installed as markdown into `.agents/skills/<name>/`; converting them into the TypeScript agent format is a separate feature.
- No change to the `mapping.yml` schema beyond the `install_path` prefix update. Specifically, no new top-level keys, no per-entry `kind` field, no nested `assets:` block.
- No rename of the `agents.md/` registry subfolder to `integrations/` or any other name. The subfolder name remains `agents.md` even though it now lives under `assets/`.
- No backfill of `harness_compatibility: [codebuff]` on existing entries. All currently-installed entries (`grill-me`, `storytelling-mastery-skill`, `write-a-skill`, `karpathy-guidelines`) keep their existing `harness_compatibility` (defaulting to "all harnesses" via empty/missing field, which now includes Codebuff implicitly).
- No automated migration tool to move user-installed entries between layouts — the move is a one-time registry-repo refactor; downstream installations are unaffected because the installer writes to `.claude/skills/` etc., not back into the registry.

## Technical Considerations

- **Move ordering in the PR.** Do the file move (`git mv`) and `mapping.yml` rewrite in one commit, then the `build.rs` / `lib.py` path-constant updates in the next. This keeps `cargo build` broken for at most one commit during review.
- **`lib.py` is the choke point.** `entry_dir`, `install_path_for`, and any constants like `SKILLS_DIR` in `src/internal/lib.py` (per the imports in `skill_add.py`) likely encode the registry paths. Centralize the change there; do not let individual scripts hard-code `assets/skills/`.
- **Codebuff also reads `AGENTS.md`** (and `CLAUDE.md`). A project with `AGENTS.md` and **no** other signal will continue to resolve as Codex under the existing priority order. This is intentional per the user decision: Codebuff is the lowest-priority match; users with both `AGENTS.md` and `.agents/` get Codex unless they remove `AGENTS.md`. Document this behavior in the README and in `harness.rs` doc comments.
- **`knowledge.md` is a generic filename.** Using it as a detection signal can cause false positives in non-Codebuff projects that happen to have a `knowledge.md`. Mitigation: it is checked **only after** all three other harnesses fail to match, and only alongside the `.agents/` directory check (`.agents/` OR `knowledge.md`). Users in false-positive territory have `--force` to override (US-016 of the original PRD).
- **Multi-select prompt for `harness_compatibility`.** `lib.py` currently exposes `prompt_choice` (single-select). The scaffold commands need a multi-select. Implement once in `lib.py` as `prompt_multi_choice(prompt, options) -> list[str]` and reuse from both `skill_new.py` and `agents_md_new.py`.
- **Slash-command surface.** The existing slash commands live under `.claude/commands/`, `.codex/skills/`, `.opencode/commands/` per the README. Add `skill-new` and `agents-md-new` to each. With Codebuff added as a harness, also add wrappers under `.agents/` if Codebuff has a slash-command convention; if it doesn't (per docs research, Codebuff slash commands are CLI-internal, not file-based), skip this and note it in the README.
- **No `--no-input` mode.** The scaffold commands are interactive only. If a non-TTY environment invokes them, fail with a clear error rather than guessing defaults — this matches `skill_add.py`'s existing behavior for the `--as` flag.

## Success Metrics

- A new skill can be created with `./instinctagents` slash command `/skill-new my-skill` and a few interactive prompts, producing a valid registry entry that the next `cargo build` includes in the catalog.
- After the `assets/` move, `cargo build`, `cargo test`, and a full ingest round-trip (`skill_add` of an existing entry → `skill_remove`) work without code path changes outside the constants module.
- Running `./instinctagents` in a directory containing only `.agents/` correctly reports Codebuff and installs a Codebuff-compatible skill into `.agents/skills/<name>/`.
- `grep -rn '^skills/\|^agents\.md/' --exclude-dir=target --exclude-dir=.git --exclude-dir=assets` returns no matches in source code (excluding the literal `agents.md/` subfolder name under `assets/`).

## Open Questions

- Should `skill-new` / `agents-md-new` accept a `--description` / `--version` / `--harness` flag set as an alternative to interactive prompts (useful for scripted use)? Current decision: no, interactive only. Revisit if a CI use case appears.
- If a user runs `/skill-new` in a Codebuff project, do we want any Codebuff-specific scaffold content (e.g. a hint about `.agents/types/` TypeScript)? Current decision: no; the placeholder is harness-neutral markdown.
- Should `mapping.yml` distinguish local-scaffolded entries from URL-ingested ones beyond `source_url: local` (e.g. a `kind: local` field)? Current decision: no — `source_url` already differentiates them.
- Does Codebuff's TUI / CLI auto-discover markdown files under `.agents/skills/`, or does the user need to reference them explicitly in `knowledge.md` or `AGENTS.md`? Worth confirming against codebuff.com/docs before US-008 lands; if auto-discovery is missing, the installer may need to append a reference line to `AGENTS.md` for each installed Codebuff skill.
