# PRD: macOS support for the external tool + `uv` for the internal tool

## 1. Introduction / Overview

This PRD covers two independent improvements to the `instinctagents` registry repo:

1. **macOS support for the external tool.** The user-facing Rust CLI/TUI (`instinctagents`) currently ships only a Linux x86_64 release binary. macOS users — a large share of the Claude Code / Codex / OpenCode audience — cannot install via the documented `curl | bash` flow. This work adds macOS arm64 (Apple Silicon) and macOS x86_64 (Intel) release artifacts, teaches `install.sh` to auto-detect OS and architecture, and updates the README.
2. **`uv` as the package manager for the internal tool.** The maintainer-only Python scripts under `src/internal/` (used by `/skill-add`, `/agents-md-add`, and the corresponding `list`/`remove` slash commands) currently rely on a hand-maintained `requirements.txt` installed with `pip`. We switch to `uv` with a `pyproject.toml` + `uv.lock`, and update the Claude Code, Codex, and OpenCode command wrappers to invoke the scripts via `uv run`.

The two workstreams are intentionally bundled because they are both small, are on the same release cadence, and together raise the floor of "out-of-the-box" installability for both the user-facing binary and the maintainer toolchain.

## 2. Goals

- A macOS user on Apple Silicon or Intel can install and run `./instinctagents` via the documented `curl | bash` flow with no manual steps.
- The release workflow publishes three platform binaries on every tag: `instinctagents-linux-x86_64`, `instinctagents-macos-arm64`, `instinctagents-macos-x86_64`.
- `install.sh` auto-detects OS + architecture and downloads the correct asset; it fails fast with a clear message on unsupported platforms.
- The internal tool installs and runs with a single `uv sync` (or implicit `uv run`) — no `pip`, no `requirements.txt` to keep in sync by hand.
- All maintainer slash commands (`/skill-add`, `/skill-list`, `/skill-remove`, `/agents-md-add`, `/agents-md-list`, `/agents-md-remove`) work after the migration in all three harnesses (Claude Code, Codex, OpenCode) without manual environment setup.
- A new contributor can clone the repo and run any internal-tool slash command after installing only `uv` (no other Python tooling required).

## 3. User Stories

### US-001: Release workflow builds macOS arm64 binary
**Description:** As a maintainer, I want the release workflow to build a macOS arm64 binary so Apple Silicon users have a download.

**Acceptance Criteria:**
- [ ] `.github/workflows/release.yml` adds a job (or matrix entry) that runs on `macos-14` (or newer Apple-Silicon-native runner) and builds for `aarch64-apple-darwin`.
- [ ] The resulting binary is stripped and uploaded to the GitHub Release as `instinctagents-macos-arm64`.
- [ ] On a tagged release dry-run, the asset appears in the release alongside the existing Linux binary.
- [ ] Existing Linux x86_64 build still succeeds and uploads `instinctagents-linux-x86_64` unchanged.

### US-002: Release workflow builds macOS x86_64 binary
**Description:** As a maintainer, I want the release workflow to also build a macOS x86_64 binary so Intel Mac users have a download.

**Acceptance Criteria:**
- [ ] Release workflow builds for `x86_64-apple-darwin` (cross-compiled from the arm64 runner via `rustup target add`, or built on a separate runner).
- [ ] Binary is stripped and uploaded as `instinctagents-macos-x86_64`.
- [ ] Both macOS binaries and the Linux binary are produced from the same tag, in the same release.

### US-003: install.sh detects macOS and picks the right asset
**Description:** As a macOS user, I want `curl -fsSL .../install.sh | bash` to download the right binary for my machine without me having to know my architecture.

**Acceptance Criteria:**
- [ ] `install.sh`'s `check_platform` accepts `Darwin` in addition to `Linux`.
- [ ] On `Darwin` + `arm64`/`aarch64`, the script downloads `instinctagents-macos-arm64`.
- [ ] On `Darwin` + `x86_64`/`amd64`, the script downloads `instinctagents-macos-x86_64`.
- [ ] On `Linux` + `x86_64`/`amd64`, behavior is unchanged (`instinctagents-linux-x86_64`).
- [ ] Unsupported combinations (e.g. Linux arm64, FreeBSD, Windows under WSL reporting unexpected uname) exit 1 with a clear `error:` message naming the detected OS and arch.
- [ ] The installed binary is `chmod +x`'d and runnable from the destination path.

### US-004: install.sh is shell-tested for all supported platforms
**Description:** As a maintainer, I want automated coverage of `install.sh`'s platform detection so a future edit cannot silently break macOS install.

**Acceptance Criteria:**
- [ ] A test script (e.g. `scripts/test-install.sh` or extension of existing test pattern) runs `install.sh` with `uname` stubbed to return each of: Linux+x86_64, Darwin+arm64, Darwin+x86_64, Linux+aarch64 (unsupported), Darwin+i386 (unsupported).
- [ ] For supported platforms, the test verifies the script would request the correct asset URL (e.g. by stubbing `curl` and asserting the URL).
- [ ] For unsupported platforms, the test verifies the script exits non-zero with an `error:` message.
- [ ] Test is wired into `.github/workflows/ci.yml` so it runs on every PR.

### US-005: README documents macOS install
**Description:** As a new user, I want the README to make it clear macOS is supported so I trust the one-liner.

**Acceptance Criteria:**
- [ ] README "Install" section states macOS (arm64 + x86_64) and Linux (x86_64) are supported.
- [ ] The existing `curl | bash` one-liner is unchanged (still works for all supported platforms).
- [ ] A short "Supported platforms" sub-list or table is added if it improves clarity.

### US-006: Add pyproject.toml for the internal tool
**Description:** As a maintainer, I want `src/internal/` declared as a `uv`-managed Python project so dependencies are version-pinned and reproducible.

**Acceptance Criteria:**
- [ ] `src/internal/pyproject.toml` is created with project name (e.g. `instinctagents-internal`), Python version requirement (matching the minimum currently supported), and `pyyaml>=6.0` as a dependency.
- [ ] `uv sync` from `src/internal/` produces a working environment that can run every script in the directory.
- [ ] `requirements.txt` is removed in the same commit (per the chosen migration approach — no pip fallback).
- [ ] `src/internal/README.md` is updated to instruct maintainers to run `uv sync` (or rely on `uv run`) instead of `pip install -r requirements.txt`.

### US-007: Generate uv.lock and commit it
**Description:** As a maintainer, I want a committed `uv.lock` so every contributor and CI run uses the exact same dependency versions.

**Acceptance Criteria:**
- [ ] `src/internal/uv.lock` exists and is committed.
- [ ] `uv sync --frozen` from `src/internal/` succeeds locally.
- [ ] `.gitignore` does not exclude `uv.lock`.

### US-008: Update Claude Code slash command wrappers to use `uv run`
**Description:** As a Claude Code maintainer, I want the slash commands to invoke the Python scripts via `uv run` so I never have to think about Python environments.

**Acceptance Criteria:**
- [ ] All six files under `.claude/commands/` (`skill-add.md`, `skill-list.md`, `skill-remove.md`, `agents-md-add.md`, `agents-md-list.md`, `agents-md-remove.md`) replace the `python src/internal/<script>.py $ARGUMENTS` invocation with `uv run --project src/internal python src/internal/<script>.py $ARGUMENTS` (or equivalent that resolves the project root from any working directory).
- [ ] `allowed-tools` frontmatter is unchanged or adjusted only as strictly needed for `uv` to run.
- [ ] Running `/skill-add <url>` in Claude Code from the repo root succeeds end-to-end against a real GitHub URL.

### US-009: Update Codex slash command wrappers to use `uv run`
**Description:** As a Codex maintainer, I want the Codex skill files to invoke scripts via `uv run` so the Codex harness also works without manual venv setup.

**Acceptance Criteria:**
- [ ] All six skill directories under `.codex/skills/` are updated to invoke scripts via `uv run` with the project pinned to `src/internal/`.
- [ ] Format matches existing Codex skill conventions in this repo (do not invent a new format).
- [ ] Running `/skill-add <url>` in Codex from the repo root succeeds end-to-end.

### US-010: Update OpenCode slash command wrappers to use `uv run`
**Description:** As an OpenCode maintainer, I want the OpenCode command files to invoke scripts via `uv run` so the third supported harness is at parity with Claude and Codex.

**Acceptance Criteria:**
- [ ] All six files under `.opencode/commands/` are updated to invoke scripts via `uv run` with the project pinned to `src/internal/`.
- [ ] Format matches existing OpenCode command conventions in this repo.
- [ ] Running `/skill-add <url>` in OpenCode from the repo root succeeds end-to-end.

### US-011: CI installs and runs the internal tool via `uv`
**Description:** As a maintainer, I want CI to exercise the internal tool the same way humans will (via `uv`) so we catch breakage on PRs.

**Acceptance Criteria:**
- [ ] `.github/workflows/ci.yml` installs `uv` (via the official setup action or curl-pipe install) on the Python job.
- [ ] CI runs `uv sync --frozen` in `src/internal/` and then runs at least one smoke command (e.g. `uv run python src/internal/skill_list.py` or whatever lint/test pass already exists) and asserts it exits 0.
- [ ] CI no longer references `pip install -r requirements.txt`.

### US-012: README documents `uv` as the prerequisite for maintainers
**Description:** As a new maintainer, I want the README to tell me to install `uv` and nothing else for the internal tool.

**Acceptance Criteria:**
- [ ] README "For maintainers" section names `uv` as the only required Python prerequisite and links to its install instructions.
- [ ] No mention of `pip` or `requirements.txt` remains in the README.
- [ ] If `src/internal/README.md` exists, it is consistent with the top-level README.

## 4. Functional Requirements

### macOS support (external tool)

- **FR-1:** The release workflow MUST produce three binaries per tag: `instinctagents-linux-x86_64`, `instinctagents-macos-arm64`, `instinctagents-macos-x86_64`.
- **FR-2:** Each macOS binary MUST be stripped before upload (parity with the existing Linux binary).
- **FR-3:** `install.sh` MUST detect OS via `uname -s` and architecture via `uname -m`, mapping:
  - `Linux` + `x86_64`/`amd64` → `instinctagents-linux-x86_64`
  - `Darwin` + `arm64`/`aarch64` → `instinctagents-macos-arm64`
  - `Darwin` + `x86_64`/`amd64` → `instinctagents-macos-x86_64`
- **FR-4:** `install.sh` MUST exit 1 with a clear, single-line `error:` message on any other (OS, arch) combination, naming both detected values.
- **FR-5:** The destination filename and the install location (current directory, `./instinctagents`) MUST be unchanged across all platforms.
- **FR-6:** The `curl | bash` one-liner in the README MUST continue to work without modification for both Linux and macOS users.

### `uv` migration (internal tool)

- **FR-7:** `src/internal/` MUST contain a `pyproject.toml` declaring the project name, required Python version, and the existing `pyyaml>=6.0` dependency.
- **FR-8:** `src/internal/` MUST contain a committed `uv.lock` produced by `uv lock`.
- **FR-9:** `src/internal/requirements.txt` MUST be removed.
- **FR-10:** All slash command wrappers in `.claude/commands/`, `.codex/skills/`, and `.opencode/commands/` MUST invoke Python scripts via `uv run` with the project pinned to `src/internal/` (e.g. `uv run --project src/internal python src/internal/<script>.py $ARGUMENTS`).
- **FR-11:** CI (`.github/workflows/ci.yml`) MUST install `uv`, run `uv sync --frozen` in `src/internal/`, and run at least one smoke invocation of an internal script.
- **FR-12:** README MUST list `uv` as the sole Python prerequisite for maintainers and remove all references to `pip` / `requirements.txt`.

## 5. Non-Goals (Out of Scope)

- **No universal (`lipo`) macOS binary.** We ship two separate arch-specific binaries, not a fat binary.
- **No Windows binary.** Windows is still unsupported; out of scope for this PRD.
- **No Linux arm64 binary.** Adding Linux arm64 is a separate request.
- **No Homebrew formula, no Cargo `binstall` integration.** Distribution remains `curl | bash`.
- **No macOS code signing or notarization.** Users may see Gatekeeper warnings on first run; documenting workarounds (right-click → Open, `xattr -d com.apple.quarantine`) is acceptable but not required by this PRD.
- **No CI builds of macOS binaries on every PR.** Per the chosen scope, macOS binaries are built only on release tags. PR CI continues to build only the Linux toolchain.
- **No restructure of `src/internal/`.** The script files keep their current names, locations, and CLI behavior. Only the dependency-management layer changes.
- **No migration of `src/external/` (the Rust crate) to anything other than `cargo`.** `uv` is for the internal Python tool only.
- **No back-compat shim for pip users of the internal tool.** Per the chosen migration approach, `requirements.txt` is removed outright.

## 6. Design Considerations

- The README's "Install" section should remain a single `curl | bash` one-liner for all platforms — discoverability matters more than splitting per-OS instructions.
- Error messages in `install.sh` should match the existing style (lowercase `error:` prefix, one line, exit 1).
- macOS binary names use `arm64` (not `aarch64`) and `x86_64` (not `amd64`) to match common macOS conventions.

## 7. Technical Considerations

- **Runners.** GitHub-hosted `macos-14` is Apple Silicon. From it, `aarch64-apple-darwin` builds natively and `x86_64-apple-darwin` can be cross-compiled after `rustup target add x86_64-apple-darwin`. Decide between a single macOS job that builds both targets (simpler, slower) or a matrix with one Apple-Silicon and one Intel runner (faster, more runner minutes). Either is acceptable; pick whichever keeps `release.yml` simpler.
- **`uv` invocation path.** `uv run --project <path>` lets the command be run from any working directory in the repo, which is important because slash commands may execute from the project root. Test that the chosen invocation works regardless of which subdirectory the harness considers `cwd`.
- **CI `uv` install.** Prefer the official `astral-sh/setup-uv` action over `curl | sh`; it caches the lock file's resolved environment and is faster on warm runs.
- **`build.rs` and `tests/`.** `src/external/build.rs` and `src/external/tests/` already exist; verify the macOS build passes the existing tests before declaring US-001/US-002 done.
- **`scripts/package-release-assets.sh`.** This script currently bundles skill/agents.md tarballs for the Linux release. Verify it does not assume Linux-specific paths/binaries; if it produces per-binary tarballs, extend it to cover macOS variants.

## 8. Success Metrics

- A fresh macOS arm64 user can install and launch the TUI in under 60 seconds from the README one-liner.
- A fresh maintainer (with only `uv` and `git` installed) can run `/skill-add <url>` successfully in any of the three supported harnesses without any other setup step.
- CI on `main` is green after the migration; no PR is blocked by missing Python dependencies.
- Release artifacts list for the next tag contains three platform binaries (down from one).
- Zero references to `pip`, `requirements.txt`, or `python -m venv` remain anywhere in repo docs or slash command wrappers.

## 9. Open Questions

- **Single mac job vs matrix?** Whoever implements US-001/US-002 should pick the runner topology that keeps `release.yml` cleanest; the PRD does not mandate one.
- **Gatekeeper warning.** Do we add a one-line note to the README telling first-time macOS users how to bypass the unsigned-binary warning, or leave it undocumented? Recommend adding a small note when US-005 is implemented, but it is not strictly required.
- **`uv` minimum version.** Should we pin a minimum `uv` version in the README so maintainers don't hit lockfile-format incompatibilities? Probably yes; capture the version actually used to generate `uv.lock` in US-007.
- **`pyproject.toml` location.** `src/internal/pyproject.toml` is assumed throughout this PRD. If the implementer prefers a repo-root `pyproject.toml` (with `src/internal/` as a package), that is acceptable as long as `uv run` from the slash commands still works without manual `cd`.
