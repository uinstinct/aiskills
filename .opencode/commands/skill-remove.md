---
description: Remove a skill from the registry by name
---

Run the skill-remove script with the user-provided arguments.

!`uv run --project src/internal python src/internal/skill_remove.py $ARGUMENTS`

Arguments:

- `--name <skill_name>` is required. Identifies the entry in `mapping.yml`'s `installed_skills` section.
- Optional `--dry-run` previews what would be deleted without mutating the repo.

The script deletes `skills/<name>/` (no-op with stderr warning if the folder is already missing) and removes the entry from `mapping.yml`'s `installed_skills` section.

If the name is not in `mapping.yml`, the script exits 1 with a clear error. Report the script's stdout summary back to the user. On a non-zero exit, surface the stderr error message verbatim.
