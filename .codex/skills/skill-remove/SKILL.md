---
name: skill-remove
description: Remove a skill from the registry by name
---

When the user asks to remove a skill by name (e.g. "run skill-remove for <name>", "delete skill <name> from the registry"), invoke the `skill_remove.py` script with the user-supplied arguments.

Run:

```bash
uv run --project src/internal python src/internal/skill_remove.py <ARGUMENTS>
```

where `<ARGUMENTS>` carries the user's `--name <skill_name>` (and `--dry-run` if they want a preview).

Arguments:

- `--name <skill_name>` is required. Identifies the entry in `mapping.yml`'s `installed_skills` section.
- Optional `--dry-run` previews what would be deleted without mutating the repo.

The script will:

1. Delete `skills/<name>/` (no-op with stderr warning if the folder is already missing).
2. Remove the entry from `mapping.yml`'s `installed_skills` section.

If the name is not in `mapping.yml`, the script exits 1 with a clear error. Report the script's stdout summary back to the user. On a non-zero exit, surface the stderr error message verbatim.
