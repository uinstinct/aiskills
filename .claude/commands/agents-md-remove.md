---
description: Remove an agents.md integration from the registry by name
allowed-tools: Bash, Read
---

Run the agents-md-remove script with the user-provided arguments.

Invoke:

```bash
uv run --project src/internal python src/internal/agents_md_remove.py $ARGUMENTS
```

Arguments:

- `--name <integration_name>` is required. Identifies the entry in `mapping.yml`'s `installed_agents_md` section.
- Optional `--dry-run` previews what would be deleted without mutating the repo.

The script will:

1. Delete `agents.md/<name>/` (no-op with stderr warning if the folder is already missing).
2. Remove the entry from `mapping.yml`'s `installed_agents_md` section.

If the name is not in `mapping.yml`, the script exits 1 with a clear error. Report the script's stdout summary back to the user. On a non-zero exit, surface the stderr error message verbatim.
