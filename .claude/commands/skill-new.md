---
description: Scaffold a new local-only skill entry under assets/skills/
allowed-tools: Bash, Read
---

Run the skill-new script with the user-provided arguments.

Invoke:

```bash
uv run --project src/internal python src/internal/skill_new.py $ARGUMENTS
```

Arguments:

- One positional `<name>` is required. It will be sanitized to lowercase `[a-z0-9_-]` and used as the folder basename under `assets/skills/`.
- Optional `--force` to overwrite an existing entry (folder on disk or mapping row).

The script prompts interactively for:

1. `description` (required, non-empty)
2. `version` (default `0.1.0`, must parse as semver)
3. `harness_compatibility` (multi-select from `claude-code` / `codex` / `opencode` / `codebuff`; empty input = all harnesses)

It then writes `assets/skills/<name>/SKILL.md` (a single `TODO: describe the skill` placeholder) and `assets/skills/<name>/manifest.yml`, and appends an entry to top-level `mapping.yml` with `source_url: local`.

Non-TTY invocation fails fast with a clear error — there are no flags to supply the fields headlessly.

Report the script's stdout summary back to the user. If the script exits non-zero, surface its stderr error message verbatim.
