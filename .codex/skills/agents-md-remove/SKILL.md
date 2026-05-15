---
name: agents-md-remove
description: Remove an agents.md integration from the registry by name
---

When the user asks to remove an agents.md integration by name (e.g. "run agents-md-remove for <name>", "delete agents.md integration <name> from the registry"), invoke the `agents_md_remove.py` script with the user-supplied arguments.

Run:

```bash
python src/internal/agents_md_remove.py <ARGUMENTS>
```

where `<ARGUMENTS>` carries the user's `--name <integration_name>` (and `--dry-run` if they want a preview).

Arguments:

- `--name <integration_name>` is required. Identifies the entry in `mapping.yml`'s `installed_agents_md` section.
- Optional `--dry-run` previews what would be deleted without mutating the repo.

The script will:

1. Delete `agents.md/<name>/` (no-op with stderr warning if the folder is already missing).
2. Remove the entry from `mapping.yml`'s `installed_agents_md` section.

If the name is not in `mapping.yml`, the script exits 1 with a clear error. Report the script's stdout summary back to the user. On a non-zero exit, surface the stderr error message verbatim.
