# instinctagents

A personal two-part Skill & `agents.md` registry:

- A single-binary **Rust CLI with TUI** (`instinctagents`) for installing/removing cataloged skills and `agents.md` snippets in any project.
- An **agentic internal tool** for maintainers that ingests entries from GitHub URLs into this registry repo.

Supports the **Claude Code**, **Codex**, and **OpenCode** harnesses.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/uinstinct/aiskills/main/install.sh | bash
```

This downloads the latest release binary into the current directory as `./instinctagents`.

**Supported platforms:** macOS arm64 (Apple Silicon), macOS x86_64 (Intel), Linux x86_64.

> **macOS users:** The binary is unsigned, so Gatekeeper may block it on first run. To allow it, either right-click the binary and choose **Open**, or run:
> ```sh
> xattr -d com.apple.quarantine ./instinctagents
> ```

## Usage

Run the binary inside any project that uses Claude Code, Codex, or OpenCode:

```sh
./instinctagents
```

The TUI opens on the **Add** tab. Use Tab / Shift+Tab to switch between **Add**, **Remove**, **List**, and **Update**.

- **Add** - browse the catalog and install one or more skills / `agents.md` snippets.
- **Remove** - uninstall items previously installed by `instinctagents`.
- **List** - read-only view of what's installed.
- **Update** - check whether a newer version of the binary is available.

The tool detects your harness automatically by looking for files like `CLAUDE.md`, `AGENTS.md`, or `opencode.json`.

## Repository layout

```
skills/                 # one folder per skill, each with manifest.yml + SKILL.md
agents.md/              # one folder per agents.md integration, each with manifest.yml + snippet.md
src/external/           # Rust source for the user-facing CLI/TUI binary
src/internal/           # Python scripts used by maintainer slash commands
mapping.yml             # registry catalog: every published skill and integration
.github/workflows/      # CI and release workflows
```

## For maintainers

Add a new skill or integration from a GitHub URL via the slash commands shipped in `.claude/commands/`, `.codex/skills/`, and `.opencode/commands/`:

```
/skill-add <github-url>
/agents-md-add <github-url>
```

These delegate to the Python scripts under `src/internal/`.

## License

See `LICENSE`.
