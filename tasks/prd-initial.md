# PRD: `instinctagents` — Skill & Agents.md Registry and Manager

## 1. Introduction / Overview

`instinctagents` is a two-part system that helps developers organize, share, and install **AI coding agent skills** and **agents.md guideline snippets** across projects.

The system has two components inside a single GitHub repository (also called the **registry repo**):

1. **External tool** — a single-binary Rust CLI with an interactive TUI. Developers download it once with `curl` and run it inside any project to add or remove skills and agents.md snippets defined in the registry. The binary ships with the registry catalog metadata baked in at compile time and downloads the actual file contents from GitHub Releases at install time.
2. **Internal tool** — an agentic skill plus supporting scripts that run inside the registry repo itself. Maintainers use slash commands (or harness-equivalent invocations) to ingest new skills/snippets from external sources (any GitHub repo or raw URL) into this registry, and to remove them. State is tracked in `mapping.yml`.

The system supports three coding-agent harnesses: **Claude Code**, **Codex**, and **OpenCode** — detecting which is in use by inspecting project files and installing into the harness-appropriate folders.

### Problem this solves

Today, copying skills between projects and adding agents.md guidelines is manual, error-prone, and easy to lose track of. There is no single place to browse a curated catalog of skills, no clean way to insert/remove guideline snippets without clobbering hand-written content, and no convention for tracking what's been installed. `instinctagents` makes this a one-command operation.

---

## 2. Goals

- Maintainers can add a new skill or agents.md snippet to the registry by running a single slash command and providing a URL.
- Downstream developers can install/remove cataloged skills and snippets in any project with a single binary, no installation step, no language runtime required.
- The external binary is stateless: it derives all state from the target project's files and a small lock file (`.instinctagents`).
- agents.md snippets are inserted between machine-parseable delimiters so they can be cleanly removed without touching user-authored content.
- Skills install to the correct location for the detected harness (`.claude/skills/`, `.codex/skills/`, `.opencode/skills/`).
- The registry repo's catalog is the source of truth; the binary embeds catalog metadata at build time so it works offline for browsing and only needs network for file downloads.
- GitHub Actions builds, tests, and publishes a Linux x86_64 binary on every release tag.
- The external tool has unit + integration tests using `assert_cmd` against temp-dir fake projects with a mocked HTTP layer.

---

## 3. User Stories

User stories are grouped into four epics. Each story is sized for one focused implementation session.

### Epic A — Registry Repo Scaffold

#### US-A01: Create repository directory layout
**Description:** As a maintainer, I want the registry repo to have the canonical folder layout so all subsequent code has a stable place to write to.

**Acceptance Criteria:**
- [ ] Repo contains top-level folders: `skills/`, `agents.md/`, `src/internal/`, `src/external/`, `.github/workflows/`
- [ ] Repo contains a top-level `mapping.yml` with the schema documented in FR-21 (initially empty: `installed_skills: []`, `installed_agents_md: []`)
- [ ] A `README.md` at the root explains the registry purpose, the curl install command (placeholder URL until US-D02 is done), and basic usage
- [ ] A `.gitignore` excludes `/target`, `*.tmp`, build artifacts
- [ ] An example skill `skills/example-skill/` exists with `SKILL.md` and `manifest.yml` to seed the catalog (real content can be placeholder)
- [ ] An example agents.md integration `agents.md/example-integration/` exists with `snippet.md` and `manifest.yml`
- [ ] `mapping.yml` references both example entries so the catalog is non-empty for downstream tests
- [ ] Typecheck/lint passes (`cargo check` if Rust scaffold exists; otherwise N/A)

---

#### US-A02: Define and document `manifest.yml` schema
**Description:** As a developer, I need a documented, validated schema for each skill and agents.md integration's `manifest.yml` so the build process and tools can rely on its shape.

**Acceptance Criteria:**
- [ ] A `docs/manifest-schema.md` file documents every field with name, type, required/optional, example, and description
- [ ] Required fields: `name` (string, must match folder name), `description` (string), `version` (semver string)
- [ ] Optional fields: `harness_compatibility` (array of `claude-code`/`codex`/`opencode`; empty/omitted means all)
- [ ] Skill manifests additionally support optional `entrypoint` (string, default `SKILL.md`) field
- [ ] agents.md manifests additionally support optional `snippet_file` (string, default `snippet.md`) field
- [ ] Example manifest files for both skill and agents.md integration are included in the doc
- [ ] Typecheck/lint passes

---

#### US-A03: Define and document `mapping.yml` schema
**Description:** As a developer, I need a documented schema for `mapping.yml` so the internal tool's scripts can produce and consume it deterministically.

**Acceptance Criteria:**
- [ ] A `docs/mapping-schema.md` file documents `mapping.yml`'s structure
- [ ] Top-level keys: `installed_skills` (list), `installed_agents_md` (list)
- [ ] Each list entry has: `name`, `version`, `source_url` (origin URL when ingested), `install_path` (path relative to repo root, e.g. `skills/example-skill/`)
- [ ] An example valid `mapping.yml` is included in the doc
- [ ] Typecheck/lint passes

---

### Epic B — External Tool (Rust CLI)

#### US-B01: Cargo project setup with module skeleton
**Description:** As a developer, I want a working Cargo project with module structure so subsequent stories have a clear place to add code.

**Acceptance Criteria:**
- [ ] `src/external/Cargo.toml` with package name `instinctagents`, edition `2021`, binary target
- [ ] Workspace or standalone (workspace recommended so internal Python/TS lives in `src/internal/` without polluting Cargo)
- [ ] Module skeleton under `src/external/src/`: `main.rs`, `cli.rs`, `tui.rs`, `harness.rs`, `catalog.rs`, `installer.rs`, `state.rs`, `update.rs`, `http.rs`
- [ ] `cargo check` passes
- [ ] `cargo build --release` produces a binary
- [ ] Typecheck/lint passes (`cargo clippy -- -D warnings`)

---

#### US-B02: Build script bakes catalog metadata into binary
**Description:** As a developer, I want the catalog (list of all skill and agents.md manifests) to be embedded into the binary at compile time so the tool works offline for browsing.

**Acceptance Criteria:**
- [ ] `src/external/build.rs` walks `skills/*/manifest.yml` and `agents.md/*/manifest.yml` from the repo root
- [ ] All manifests are aggregated into a single in-memory structure exposed via `include_str!`-style macro or generated `.rs` file in `OUT_DIR`
- [ ] The aggregate is keyed: catalog has two sections `skills` and `agents_md`, each containing entries with `name`, `description`, `version`, `harness_compatibility`, `source_path` (relative path within the repo where the actual files live, used to construct release download URLs)
- [ ] Missing/malformed manifest causes build failure with a clear error message identifying the offending file
- [ ] A unit test verifies the embedded catalog is non-empty and contains the example entries from US-A01
- [ ] Typecheck/lint passes

---

#### US-B03: Harness detection
**Description:** As a developer, I want the tool to detect the active coding-agent harness based on project files so it installs to the right folder.

**Acceptance Criteria:**
- [ ] `harness.rs` exports `enum Harness { ClaudeCode, Codex, OpenCode }` and `fn detect(project_root: &Path) -> Option<Harness>`
- [ ] Detection rules in this exact priority order (first match wins):
  - [ ] `CLAUDE.md` exists at project root OR `.claude/` directory exists → `ClaudeCode`
  - [ ] `AGENTS.md` exists at project root OR `.codex/` directory exists → `Codex`
  - [ ] `opencode.json` or `opencode.jsonc` exists at project root OR `.opencode/` directory exists → `OpenCode`
- [ ] Returns `None` if none match; caller surfaces an error
- [ ] `harness.target_folder()` method returns: `ClaudeCode` → `.claude/skills/`, `Codex` → `.codex/skills/`, `OpenCode` → `.opencode/skills/`
- [ ] `harness.agents_md_file()` method returns: `ClaudeCode` → `CLAUDE.md`, `Codex` → `AGENTS.md`, `OpenCode` → `AGENTS.md`
- [ ] Unit tests cover all three positive cases, priority ordering when multiple signals coexist, and the `None` case
- [ ] Typecheck/lint passes

---

#### US-B04: HTTP layer with download from GitHub Releases
**Description:** As a developer, I want a centralized HTTP layer that downloads skill/snippet files from the registry's GitHub Releases so installation works in any environment.

**Acceptance Criteria:**
- [ ] `http.rs` uses `reqwest` (blocking) or `ureq` for HTTP (pick the smaller dependency tree; recommend `ureq` for fewer transitive deps)
- [ ] `download_release_asset(repo: &str, tag: &str, asset_name: &str) -> Result<Bytes>` function exists
- [ ] Each release attaches one tarball per skill (named `skill-<name>-<version>.tar.gz`) and one per agents.md integration (`agents-md-<name>-<version>.tar.gz`) — see US-D03 for release packaging
- [ ] The function fetches via `https://github.com/<owner>/<repo>/releases/download/<tag>/<asset_name>`
- [ ] Errors surface clearly (404 → "asset not found"; network → "failed to reach github.com")
- [ ] Tests use the mock HTTP server from US-B14 to verify success and 404 paths
- [ ] Typecheck/lint passes

---

#### US-B05: State file (`.instinctagents`) read/write
**Description:** As a developer, I need a small state file in target projects so the tool knows what's installed without scanning the filesystem.

**Acceptance Criteria:**
- [ ] `state.rs` defines `struct ProjectState { installed_skills: Vec<InstalledItem>, installed_agents_md: Vec<InstalledItem>, last_update_check: Option<DateTime<Utc>>, latest_known_version: Option<String> }`
- [ ] `InstalledItem` fields: `name`, `version`, `source_url`, `install_path` (skill folder path) OR `delimiter_id` (agents.md only, equal to the skill name)
- [ ] File format: TOML (simpler than YAML in Rust; `serde` support widespread); path `<project_root>/.instinctagents`
- [ ] `ProjectState::load(project_root)` returns default empty state if file doesn't exist
- [ ] `ProjectState::save(project_root)` writes atomically (write to temp file + rename)
- [ ] Same file stores the auto-update cache (the `last_update_check` and `latest_known_version` fields) per US-B11
- [ ] Unit tests cover load-missing, load-existing, save-and-roundtrip
- [ ] Typecheck/lint passes

---

#### US-B06: TUI scaffolding with tabs
**Description:** As a user, I want a single TUI with tabs for Add / Remove / List / Update so I can do all operations from one interface.

**Acceptance Criteria:**
- [ ] Use `ratatui` for the TUI (full-screen) and `crossterm` for input
- [ ] Tabs: `Add`, `Remove`, `List`, `Update`; switch with Tab / Shift+Tab or number keys 1–4
- [ ] Header bar shows detected harness or warning "No harness detected"; if no harness, the Add and Remove tabs are disabled
- [ ] Footer bar shows context-sensitive keybindings (e.g. `↑/↓ navigate · Space select · Enter confirm · q quit`)
- [ ] `q` or `Esc` exits cleanly; terminal state restored
- [ ] Empty placeholder content in each tab is fine for this story; subsequent stories fill them
- [ ] An integration test launches the binary, sends `q`, and asserts exit code 0
- [ ] Typecheck/lint passes
- [ ] Verify in terminal manually (this is a TUI, not a browser UI — note this exception to the dev-browser rule)

---

#### US-B07: Add tab — list, multi-select, install skills and agents.md
**Description:** As a user, I want to browse the catalog in the Add tab and install one or more skills/snippets at once.

**Acceptance Criteria:**
- [ ] Two-column or two-section layout: top half lists skills, bottom half lists agents.md integrations (or split with a header)
- [ ] Each row shows: name, version, description (truncated), harness-compat tag, `[installed]` marker if present in `.instinctagents`
- [ ] Rows that are incompatible with the detected harness are dimmed and not selectable; tagged like `[claude-code only]`
- [ ] Space toggles selection; selected rows are highlighted with a checkmark
- [ ] Enter triggers installation for all checked items
- [ ] For each install: if the target install path already exists on disk, show a modal prompt: `Overwrite / Skip / Rename` (default `Skip` if non-interactive, e.g. `--force=skip` flag)
- [ ] Skill install: download tarball via US-B04, extract to `<harness_folder>/<skill_name>/`, update `.instinctagents`
- [ ] agents.md install: see US-B08 (delegate)
- [ ] On install completion, refresh the list to show new `[installed]` markers
- [ ] Errors are shown inline as a toast/footer message without crashing the TUI
- [ ] Typecheck/lint passes
- [ ] Manual verification in terminal

---

#### US-B08: Inject agents.md snippet with delimiters
**Description:** As a user, when I install an agents.md integration, the snippet must be injected into the project's harness instruction file with machine-parseable delimiters so it can be cleanly removed later.

**Acceptance Criteria:**
- [ ] Determine the target file from `harness.agents_md_file()` (e.g. `CLAUDE.md`)
- [ ] If the file doesn't exist, create it
- [ ] Append to the end of the file (with a blank line before):
  ```
  <!-- instinctagents:start:<name> -->
  <contents of snippet.md>
  <!-- instinctagents:end:<name> -->
  ```
- [ ] If a block with the same `<name>` already exists, replace it in place (do not duplicate)
- [ ] Never modify any content outside the delimiter pair
- [ ] Update `.instinctagents` with the new entry
- [ ] Unit tests cover: fresh file, file with unrelated user content (must be preserved verbatim), file with an existing block of the same name (must be replaced), file with multiple existing blocks (only the matching one is affected)
- [ ] Typecheck/lint passes

---

#### US-B09: Remove tab — list installed, multi-select, uninstall
**Description:** As a user, I want a separate Remove tab that only shows what's currently installed in this project so I can uninstall items without distraction.

**Acceptance Criteria:**
- [ ] Reads `.instinctagents`; shows only entries from `installed_skills` and `installed_agents_md`
- [ ] Empty state: "Nothing installed in this project."
- [ ] Multi-select with Space, confirm with Enter
- [ ] Skill removal: delete the skill folder under `<harness_folder>/<skill_name>/`; remove entry from `.instinctagents`
- [ ] agents.md removal: open the harness instruction file, delete the block bounded by `<!-- instinctagents:start:<name> -->` and `<!-- instinctagents:end:<name> -->` (inclusive); remove entry from `.instinctagents`
- [ ] If the matching delimiter pair is not found in the file, log a warning, still remove from `.instinctagents`
- [ ] Confirmation modal before destructive action: "Remove N item(s)? [y/N]"
- [ ] Unit tests cover successful removal, missing-delimiter case, partial-state edge cases
- [ ] Typecheck/lint passes
- [ ] Manual verification in terminal

---

#### US-B10: List tab — show what's installed
**Description:** As a user, I want a read-only List tab showing everything installed in this project with versions.

**Acceptance Criteria:**
- [ ] Two sections: "Skills" and "agents.md integrations"
- [ ] Each entry shows: name, version, source_url, install_path
- [ ] Empty state: "Nothing installed."
- [ ] No editing — purely read-only
- [ ] Typecheck/lint passes
- [ ] Manual verification in terminal

---

#### US-B11: Update tab — check binary for new version
**Description:** As a user, I want the Update tab to show whether the binary has a newer version available, with the GitHub release URL to re-download.

**Acceptance Criteria:**
- [ ] On entering the Update tab (and on every binary launch passively), check GitHub Releases API: `GET https://api.github.com/repos/<owner>/<repo>/releases/latest`
- [ ] Cache result in `.instinctagents` (fields `last_update_check`, `latest_known_version`) — skip the network call if the cache is < 24h old
- [ ] Compare semver against the binary's `CARGO_PKG_VERSION`
- [ ] If newer: print/show one-line message "A new version v<X.Y.Z> is available. Re-run the curl install command to update." with the release URL
- [ ] If up to date: show "You are on the latest version (vX.Y.Z)."
- [ ] On binary launch (any tab), print the "update available" line once at the top if applicable, then proceed normally
- [ ] Never auto-replace the binary
- [ ] Network errors during the check are silent (don't block the tool); they only log to stderr if `--verbose` is set
- [ ] Unit tests cover: cache fresh (no network), cache stale (network call made), API error (graceful fallback), version comparison cases
- [ ] Typecheck/lint passes

---

#### US-B12: CLI argument parsing
**Description:** As a user, I want a small set of CLI flags for non-interactive use and overrides.

**Acceptance Criteria:**
- [ ] Use `clap` v4 with derive macros
- [ ] Flags: `--version` (print version, exit), `--help` (print help, exit), `--force` (override compatibility checks), `--non-interactive` (skip TUI; require `add`/`remove` subcommands with explicit `--name`), `--verbose` (debug logging to stderr)
- [ ] Bare `instinctagents` launches the TUI on the Add tab
- [ ] `instinctagents --non-interactive add --name <name> [--type skill|agents-md]` and `instinctagents --non-interactive remove --name <name> [--type skill|agents-md]` work without TUI
- [ ] `--type` is required only when both a skill and an agents.md integration share the same name
- [ ] Typecheck/lint passes

---

#### US-B13: Single-command curl install script
**Description:** As a user, I want to install the binary with one curl command, no package manager needed.

**Acceptance Criteria:**
- [ ] An `install.sh` lives at the root of the registry repo
- [ ] Curl invocation: `curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/install.sh | bash`
- [ ] Script downloads the latest release asset for Linux x86_64 from GitHub Releases API
- [ ] Defaults install location to `./instinctagents` in the current directory (no system-wide install per spec: "no need of installation in the machine")
- [ ] Marks the binary executable (`chmod +x`)
- [ ] Prints next-step hint: "Run ./instinctagents to start."
- [ ] Script fails clearly if not on Linux x86_64
- [ ] Typecheck/lint passes (`shellcheck install.sh` clean)

---

#### US-B14: Test infrastructure — mock HTTP and tempdir fake projects
**Description:** As a developer, I want reusable test infrastructure so subsequent integration tests can run in isolation.

**Acceptance Criteria:**
- [ ] Dev-dependency: `mockito` or `wiremock` for HTTP mocking
- [ ] Dev-dependency: `assert_cmd` for invoking the built binary; `predicates` for assertions; `tempfile` for tempdir scaffolding
- [ ] Helper module `tests/common/mod.rs` exposes: `fn fake_claude_project() -> TempDir`, `fn fake_codex_project() -> TempDir`, `fn fake_opencode_project() -> TempDir`
- [ ] Helper `fn mock_github_releases(tag: &str, assets: &[(&str, Bytes)]) -> Server` returns a running mock server
- [ ] At least one smoke test uses each helper and passes
- [ ] Typecheck/lint passes

---

#### US-B15: Integration tests — install/remove flows for all three harnesses
**Description:** As a developer, I need end-to-end integration tests covering install and remove for both skills and agents.md across all three harnesses.

**Acceptance Criteria:**
- [ ] Test: install skill into fake Claude Code project; assert `.claude/skills/<name>/SKILL.md` exists; assert `.instinctagents` updated
- [ ] Test: install skill into fake Codex project; assert `.codex/skills/<name>/SKILL.md` exists
- [ ] Test: install skill into fake OpenCode project; assert `.opencode/skills/<name>/SKILL.md` exists
- [ ] Test: install agents.md integration into fake Claude Code project; assert `CLAUDE.md` contains the delimiter block; assert pre-existing user content above is preserved byte-for-byte
- [ ] Test: install agents.md into fake Codex project (writes `AGENTS.md`); same assertions
- [ ] Test: install agents.md into fake OpenCode project (writes `AGENTS.md`); same assertions
- [ ] Test: remove skill; assert folder deleted and `.instinctagents` updated
- [ ] Test: remove agents.md; assert only the delimited block is removed, user content preserved
- [ ] Test: install incompatible skill with `--force` succeeds; without `--force` is blocked
- [ ] Test: install when folder already exists with `--force=skip` skips; with `--force=overwrite` overwrites
- [ ] All tests use US-B14 helpers; no real network calls
- [ ] Typecheck/lint passes
- [ ] `cargo test` runs all tests green

---

### Epic C — Internal Tool (Agentic skill for the registry repo)

#### US-C01: Internal tool source skeleton
**Description:** As a maintainer, I want a clear `src/internal/` layout for the scripts that back the slash commands.

**Acceptance Criteria:**
- [ ] `src/internal/` contains Python scripts (recommended for stdlib YAML/HTTP via `urllib`/`pyyaml`): `skill_add.py`, `skill_remove.py`, `skill_list.py`, `agents_md_add.py`, `agents_md_remove.py`, `agents_md_list.py`, plus a shared `lib.py` for YAML and URL parsing
- [ ] A `requirements.txt` pins the minimum deps (`pyyaml`) — keep it tiny
- [ ] A `README.md` inside `src/internal/` documents how each script is invoked and what arguments it accepts
- [ ] All scripts use `argparse`, support `--help`, return nonzero on failure
- [ ] Typecheck passes (`python -m py_compile src/internal/*.py`)

---

#### US-C02: `skill-add` script — ingest a skill from a URL into the registry
**Description:** As a maintainer, I want a script that accepts a GitHub URL (repo, folder, or single file), downloads the contents, and adds them to `skills/` in this registry repo.

**Acceptance Criteria:**
- [ ] Input: one positional argument, a URL
- [ ] URL detection logic:
  - [ ] Repo root URL (`https://github.com/<owner>/<repo>`) → fetch the repo's top-level tree via GitHub API; if it contains a single skill folder (folder with a `SKILL.md` or `manifest.yml`), import it; if multiple skill folders, print a numbered list and prompt the user to pick one
  - [ ] Folder URL (`https://github.com/<owner>/<repo>/tree/<ref>/<path>`) → import that folder as a single skill
  - [ ] Raw file URL (`https://github.com/<owner>/<repo>/blob/<ref>/<path>` or `raw.githubusercontent.com`) → download the single file, then prompt: "Add as (1) skill, (2) agents.md integration, (3) both?"
- [ ] Imported skill goes to `skills/<name>/` (name derived from source folder name, or sanitized filename for single-file imports)
- [ ] If `manifest.yml` is missing from the source, scaffold a minimal one (prompt for `description` and `version`)
- [ ] Update `mapping.yml` with the new entry (`name`, `version`, `source_url`, `install_path`)
- [ ] Refuse to overwrite an existing entry without `--force`
- [ ] Print a confirmation summary of files added
- [ ] Typecheck passes

---

#### US-C03: `skill-remove` script
**Description:** As a maintainer, I want a script to remove a skill from the registry by name.

**Acceptance Criteria:**
- [ ] Input: `--name <skill_name>`
- [ ] Resolves to `skills/<name>/`; deletes the folder
- [ ] Removes the entry from `mapping.yml`
- [ ] Errors clearly if the name is not in `mapping.yml`
- [ ] `--dry-run` flag previews what would be deleted
- [ ] Typecheck passes

---

#### US-C04: `skill-list` script
**Description:** As a maintainer, I want a script that prints all skills currently in the registry.

**Acceptance Criteria:**
- [ ] Reads `mapping.yml`; prints a tabular list (name, version, source_url)
- [ ] `--json` flag prints machine-readable JSON
- [ ] Typecheck passes

---

#### US-C05: `agents-md-add` script — ingest an agents.md snippet
**Description:** As a maintainer, I want a script that adds a new agents.md integration to the registry from a URL.

**Acceptance Criteria:**
- [ ] Input: one positional URL (single-file raw URL preferred; repo-folder URL containing `snippet.md` + `manifest.yml` also supported)
- [ ] Imported integration goes to `agents.md/<name>/snippet.md` + `agents.md/<name>/manifest.yml`
- [ ] If `manifest.yml` is missing from the source, scaffold it (prompt for `name`, `description`, `version`, `harness_compatibility`)
- [ ] Update `mapping.yml` `installed_agents_md` section
- [ ] Refuses overwrite without `--force`
- [ ] Typecheck passes

---

#### US-C06: `agents-md-remove` and `agents-md-list` scripts
**Description:** As a maintainer, I want the remove and list scripts for agents.md integrations.

**Acceptance Criteria:**
- [ ] `agents_md_remove.py` mirrors US-C03 but for `agents.md/<name>/`
- [ ] `agents_md_list.py` mirrors US-C04 but reads `installed_agents_md`
- [ ] Both update `mapping.yml`
- [ ] Typecheck passes

---

#### US-C07: Claude Code slash command files
**Description:** As a maintainer running Claude Code in the registry repo, I want native slash commands.

**Acceptance Criteria:**
- [ ] Six files exist: `.claude/commands/skill-add.md`, `.claude/commands/skill-remove.md`, `.claude/commands/skill-list.md`, `.claude/commands/agents-md-add.md`, `.claude/commands/agents-md-remove.md`, `.claude/commands/agents-md-list.md`
- [ ] Each command's body instructs Claude to invoke the corresponding Python script in `src/internal/` with the user-provided arguments
- [ ] Each command's YAML frontmatter sets a description matching the script's purpose
- [ ] Manual verification: invoking `/skill-add <url>` in Claude Code runs `python src/internal/skill_add.py <url>` and reports the result
- [ ] Typecheck passes

---

#### US-C08: Codex skill equivalents
**Description:** As a maintainer running Codex in the registry repo, I want the same commands available as Codex skills (Codex custom prompts are deprecated; skills are the recommended replacement).

**Acceptance Criteria:**
- [ ] Six skill folders exist: `.codex/skills/skill-add/SKILL.md` (and the corresponding five others)
- [ ] Each `SKILL.md` has correct YAML frontmatter (`name`, `description`) per Codex skill conventions
- [ ] Skill body instructs the agent to call the matching Python script in `src/internal/` with arguments
- [ ] Manual verification: from a Codex session in the registry repo, asking to "run skill-add with URL <url>" triggers the script
- [ ] Typecheck passes

---

#### US-C09: OpenCode command files
**Description:** As a maintainer running OpenCode in the registry repo, I want native OpenCode commands.

**Acceptance Criteria:**
- [ ] Six files exist: `.opencode/commands/skill-add.md`, `.opencode/commands/skill-remove.md`, `.opencode/commands/skill-list.md`, `.opencode/commands/agents-md-add.md`, `.opencode/commands/agents-md-remove.md`, `.opencode/commands/agents-md-list.md`
- [ ] Each file body invokes the Python script using OpenCode's `RUN` directive or equivalent (verify exact syntax against OpenCode docs at implementation time)
- [ ] Manual verification: `/skill-add <url>` in OpenCode runs the script
- [ ] Typecheck passes

---

### Epic D — CI / Release Pipeline

#### US-D01: GitHub Actions CI workflow — build and test on PR
**Description:** As a maintainer, I want every PR to trigger a build + test run.

**Acceptance Criteria:**
- [ ] `.github/workflows/ci.yml` exists; triggers on `pull_request` and `push` to `main`
- [ ] Job runs on `ubuntu-latest`
- [ ] Steps: checkout, install Rust stable, cache cargo, `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- [ ] Optional Python lint step for `src/internal/` (`python -m py_compile` over all `.py` files)
- [ ] CI fails on any non-zero exit
- [ ] Workflow runs green on a sample PR

---

#### US-D02: GitHub Actions release workflow — build and publish binary
**Description:** As a maintainer, I want pushing a `v*` git tag to build the Linux x86_64 binary and attach it to a GitHub release.

**Acceptance Criteria:**
- [ ] `.github/workflows/release.yml` triggers on tags matching `v*.*.*`
- [ ] Job runs on `ubuntu-latest`, target `x86_64-unknown-linux-gnu`
- [ ] Steps: checkout, install Rust stable + target, `cargo build --release --target x86_64-unknown-linux-gnu`, strip binary, rename to `instinctagents-linux-x86_64`
- [ ] Creates a GitHub Release for the tag using `softprops/action-gh-release` or equivalent
- [ ] Attaches `instinctagents-linux-x86_64` as a release asset
- [ ] Also attaches packaged tarballs for every skill and agents.md integration in the registry (one tarball each, named `skill-<name>-<version>.tar.gz` / `agents-md-<name>-<version>.tar.gz`) — see US-D03 for the packaging step
- [ ] Release notes auto-generated from commits since previous tag
- [ ] Manual verification: tag `v0.1.0`, observe release created with all assets attached

---

#### US-D03: Release asset packaging step
**Description:** As a developer, I need a reproducible step in the release workflow that packages each skill and agents.md integration into a tarball.

**Acceptance Criteria:**
- [ ] A script `scripts/package-release-assets.sh` walks `skills/` and `agents.md/`, reads each entry's `manifest.yml` for `version`, and emits `dist/skill-<name>-<version>.tar.gz` and `dist/agents-md-<name>-<version>.tar.gz`
- [ ] Tarball contents: the entire folder rooted at the skill/integration name (so extracting `skill-foo-1.0.0.tar.gz` produces a `foo/` directory)
- [ ] Script invoked in `release.yml` before the `softprops/action-gh-release` step; all tarballs from `dist/` attached
- [ ] Script is deterministic (`tar --sort=name --owner=0 --group=0 --mtime=...` to enable reproducible builds)
- [ ] Tests: a shell test runs the script against the repo and asserts the expected tarballs are produced for the example entries from US-A01
- [ ] `shellcheck` clean

---

## 4. Functional Requirements

### Registry repo

- **FR-1:** The registry repo must contain `skills/`, `agents.md/`, `src/internal/`, `src/external/`, and `mapping.yml` at the root.
- **FR-2:** Every entry under `skills/<name>/` must contain `SKILL.md` and `manifest.yml`. The folder name must equal `manifest.yml`'s `name` field.
- **FR-3:** Every entry under `agents.md/<name>/` must contain `snippet.md` and `manifest.yml`. The folder name must equal `manifest.yml`'s `name` field.
- **FR-4:** `manifest.yml` schema: `name` (string, required), `description` (string, required), `version` (semver string, required), `harness_compatibility` (array of strings: `claude-code`, `codex`, `opencode`; omitted/empty = all).
- **FR-5:** `mapping.yml` schema: top-level `installed_skills` and `installed_agents_md` lists; each entry has `name`, `version`, `source_url`, `install_path`.
- **FR-6:** A CI step must validate that every folder under `skills/` and `agents.md/` is listed in `mapping.yml` and vice versa.

### External tool — general

- **FR-7:** The external tool must be a single statically-linked Rust binary, no system installation required, invoked as `./instinctagents`.
- **FR-8:** The binary must be installable via a single curl command per US-B13.
- **FR-9:** The binary's catalog must be baked in at compile time via `build.rs`.
- **FR-10:** The binary must download skill/snippet file contents from GitHub Releases of the registry repo (not raw GitHub URLs).
- **FR-11:** The binary must be stateless: all state must be derivable from the target project's files and the `.instinctagents` lock file.

### External tool — harness detection

- **FR-12:** The tool must detect the active harness using this priority: (1) `CLAUDE.md` or `.claude/` → `claude-code`; (2) `AGENTS.md` or `.codex/` → `codex`; (3) `opencode.json`/`opencode.jsonc` or `.opencode/` → `opencode`.
- **FR-13:** When no harness is detected, the Add and Remove tabs must be disabled and a clear message shown.
- **FR-14:** Skill install path per harness: `claude-code` → `.claude/skills/<name>/`; `codex` → `.codex/skills/<name>/`; `opencode` → `.opencode/skills/<name>/`.
- **FR-15:** agents.md target file per harness: `claude-code` → `CLAUDE.md`; `codex` → `AGENTS.md`; `opencode` → `AGENTS.md`.

### External tool — TUI

- **FR-16:** The TUI must have four tabs: Add, Remove, List, Update.
- **FR-17:** The Add tab must show the full catalog with incompatible-with-harness rows visually dimmed and not selectable (unless `--force` is used).
- **FR-18:** Multi-select via Space; confirm via Enter; quit via `q`.
- **FR-19:** When installing a skill whose folder already exists at the target path, the tool must prompt: Overwrite / Skip / Rename (default Skip if non-interactive).
- **FR-20:** The Remove tab must show only items currently in `.instinctagents`.

### External tool — agents.md injection

- **FR-21:** Injected agents.md snippets must be wrapped in delimiters of the exact form: `<!-- instinctagents:start:<name> -->` and `<!-- instinctagents:end:<name> -->`.
- **FR-22:** Removal must delete only the content between (and including) the matching delimiter pair; all other content must remain byte-for-byte unchanged.
- **FR-23:** Re-installing an agents.md integration with the same name must replace the existing block in place, not append a duplicate.

### External tool — auto-update

- **FR-24:** The binary must check `GET https://api.github.com/repos/<owner>/<repo>/releases/latest` at most once per 24h (cached in `.instinctagents`).
- **FR-25:** When a newer version exists, the tool must print a one-line notice at startup and show details in the Update tab.
- **FR-26:** The tool must never auto-replace its own binary.
- **FR-27:** Network errors during the update check must be silent (not block tool use); they may log to stderr only when `--verbose`.

### Internal tool

- **FR-28:** The internal tool's scripts must be plain Python files in `src/internal/`, invokable directly.
- **FR-29:** `/skill-add <url>` (and harness equivalents) must accept a GitHub repo URL, folder URL, or raw file URL; behavior is per US-C02.
- **FR-30:** Single-file inputs must prompt the user: skill / agents.md / both.
- **FR-31:** `/skill-remove <name>` removes the skill folder and its `mapping.yml` entry; same for agents.md.
- **FR-32:** All slash command invocations must update `mapping.yml` atomically.
- **FR-33:** The internal tool must support invocation from all three harnesses via the files specified in US-C07–C09.

### CI/release

- **FR-34:** Every PR must run `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`.
- **FR-35:** Pushing a tag `v*.*.*` must produce a GitHub Release with the Linux x86_64 binary plus one tarball per catalog entry.

---

## 5. Non-Goals (Out of Scope)

- **No central skill registry hosted by the project owner.** This repo is the registry; users fork or use it as-is.
- **No multi-platform binaries for v1.** Only Linux x86_64; macOS, Windows, ARM left for a later version.
- **No internal tool test suite for v1.** Per maintainer's decision, skip automated tests for the internal Python scripts. Manual verification only.
- **No system-wide installation.** The binary lives in the project directory; users re-run the curl command to update.
- **No auto-replacement of the binary.** Update check only notifies; the user re-runs the install command.
- **No support for global (~/.claude/skills, etc.) installation.** Project-scoped only.
- **No conflict resolution for users editing inside delimiter blocks.** If a user edits inside a `<!-- instinctagents:start:... -->` block, their changes are lost on reinstall — documented behavior, not a bug.
- **No remote source other than the registry repo's own GitHub Releases for the external tool.** The internal tool can ingest from anywhere; the external tool cannot.
- **No skill versioning UX in the external tool's TUI for v1.** Always installs the latest version present in the binary's embedded catalog.
- **No web UI, no GUI, no IDE integration.** Terminal only.
- **No authentication for downloading public release assets.** GitHub Releases are public.
- **No skill dependency graph.** Each skill is independent; no `depends_on` field in v1.

---

## 6. Design Considerations

### TUI design

- Use `ratatui` for layout; tabs at the top, body in the middle, keybinding hints at the bottom.
- Color: use only widely-supported 8-color ANSI to avoid terminal compatibility issues. Reserve red for destructive actions, green for successful installs.
- The "incompatible" visual: dim text + a `[claude-code only]` tag at the end of the row.
- Empty states should be helpful, not blank: "No skills installed yet. Press 1 to switch to the Add tab and pick some."

### Delimiter design

- HTML comments (`<!-- ... -->`) are invisible in rendered Markdown and well-supported by every parser.
- The `<name>` in delimiters must match the skill/integration name exactly (case-sensitive), enabling unambiguous removal.

### CLI ergonomics

- Bare `instinctagents` launches the TUI. This is the primary path.
- Subcommands exist for non-interactive use (CI, scripting), gated behind `--non-interactive`.
- `--version` prints the binary version and the embedded catalog's commit SHA (from `build.rs` baking in `git rev-parse HEAD`).

---

## 7. Technical Considerations

### Rust dependencies (external tool)

- `clap` v4 (derive) — CLI parsing
- `ratatui` + `crossterm` — TUI
- `serde` + `serde_yaml` — manifest parsing at build time
- `serde` + `toml` — state file (`.instinctagents`)
- `ureq` — minimal blocking HTTP (smaller than `reqwest`); add `rustls` feature
- `tar` + `flate2` — tarball extraction
- `semver` — version comparison
- `chrono` — timestamps for update cache
- Dev: `assert_cmd`, `predicates`, `tempfile`, `mockito` (or `wiremock`)

### Build script (`build.rs`) responsibilities

- Walk `skills/*/manifest.yml` and `agents.md/*/manifest.yml` from the repo root.
- Validate each manifest against the schema (fail build on invalid YAML or missing required fields).
- Emit a generated `catalog.rs` in `OUT_DIR` with `pub const CATALOG: &str = r#"..."#` or a typed structure consumed by `include!`.
- Also embed the current git commit SHA (`git rev-parse HEAD`) for version display.

### Python dependencies (internal tool)

- `pyyaml` only. Use stdlib `urllib`, `argparse`, `pathlib`, `tarfile`, `tempfile`, `subprocess`. Keep deps minimal.

### State file format choice

- TOML for `.instinctagents` (Rust ecosystem affinity, simpler than YAML for nested data).
- YAML for `manifest.yml` and `mapping.yml` (human-edited; matches the broader agent-config ecosystem).

### Reproducible release builds

- Use `tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='1970-01-01'` for asset tarballs so identical inputs produce identical hashes.
- Strip the binary (`strip --strip-all instinctagents-linux-x86_64`) for size.

### Failure modes to handle gracefully

- No harness detected → clear error, exit non-zero in non-interactive mode; show banner in TUI mode.
- GitHub API rate limit → fall back to "update check unavailable" silently.
- Skill download 404 → say "this skill is not in the latest release; please re-install or report an issue."
- Corrupt `.instinctagents` → log warning, treat as empty, continue.

### Risks

- **OpenCode and Codex both use `AGENTS.md`.** Our detection priority makes `AGENTS.md` → Codex by default; an OpenCode user without `opencode.json` will be mis-detected. Mitigation: document this clearly in the README and recommend OpenCode users keep an `opencode.json` (which they typically have anyway).
- **Custom prompts in Codex are deprecated.** We use skills for Codex's internal-tool entry points (US-C08). Verify the exact frontmatter required at implementation time against current Codex docs.
- **Skill release tarball cardinality grows linearly with the catalog.** Acceptable until the catalog gets very large (>500 entries); revisit then.

---

## 8. Success Metrics

- A new developer can install the binary and add a skill to their project in under 60 seconds (one curl, one TUI interaction).
- A maintainer can ingest a new skill from any GitHub URL via slash command in under 30 seconds end-to-end.
- 100% of installs and removes leave the project's pre-existing files byte-identical outside the targeted folder/delimiter block (verified by the integration tests in US-B15).
- Zero false positives in harness detection across at least 10 sample real-world projects.
- Binary cold-start time (launch to TUI visible) under 200ms on a typical Linux laptop.
- CI: `cargo test` runs to completion in under 60 seconds on `ubuntu-latest`.
- Release build artifact size under 10 MB stripped.
