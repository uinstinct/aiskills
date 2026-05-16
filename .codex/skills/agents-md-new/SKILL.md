---
name: agents-md-new
description: Scaffold a new local-only agents.md integration under assets/agents.md/
---

When the user asks to scaffold or create a new local agents.md integration (e.g. "run agents-md-new with name <name>", "create a new agents.md integration called <name>"), invoke the `agents_md_new.py` script with the user-supplied arguments.

Run:

```bash
uv run --project src/internal python src/internal/agents_md_new.py <ARGUMENTS>
```

where `<ARGUMENTS>` is the integration name the user provided, plus any optional flags they mentioned.

Arguments:

- One positional `<name>` is required. It will be sanitized to lowercase `[a-z0-9_-]` and used as the folder basename under `assets/agents.md/`.
- Optional `--force` to overwrite an existing entry (folder on disk or mapping row).

The script prompts interactively for:

1. `description` (required, non-empty)
2. `version` (default `0.1.0`, must parse as semver)
3. `harness_compatibility` (multi-select from `claude-code` / `codex` / `opencode` / `codebuff`; empty input = all harnesses)

It then writes `assets/agents.md/<name>/snippet.md` (a single `TODO: describe the integration` placeholder) and `assets/agents.md/<name>/manifest.yml`, and appends an entry to top-level `mapping.yml` with `source_url: local`.

Non-TTY invocation fails fast with a clear error — there are no flags to supply the fields headlessly.

Report the script's stdout summary back to the user. If the script exits non-zero, surface its stderr error message verbatim.
