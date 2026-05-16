---
description: Scaffold a new local-only agents.md integration under assets/agents.md/
---

Run the agents-md-new script with the user-provided arguments.

!`uv run --project src/internal python src/internal/agents_md_new.py $ARGUMENTS`

Arguments:

- One positional `<name>` is required. It will be sanitized to lowercase `[a-z0-9_-]` and used as the folder basename under `assets/agents.md/`.
- Optional `--force` to overwrite an existing entry (folder on disk or mapping row).

The script prompts for `description` (required), `version` (default `0.1.0`, semver), and `harness_compatibility` (multi-select from `claude-code` / `codex` / `opencode` / `codebuff`; empty = all). It writes `assets/agents.md/<name>/snippet.md` + `manifest.yml`, and appends a `source_url: local` row to top-level `mapping.yml`.

Non-TTY invocation fails fast — there are no flags to supply the fields headlessly.

Report the script's stdout summary back to the user. If the script exited non-zero, surface its stderr error message verbatim.
