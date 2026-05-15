---
name: skill-list
description: List all skills currently in the registry
---

When the user asks to list the skills currently in this registry (e.g. "run skill-list", "show me the skills in the registry"), invoke the `skill_list.py` script with any flags the user supplied.

Run:

```bash
python src/internal/skill_list.py <ARGUMENTS>
```

Arguments (all optional):

- `--json` prints the list as a machine-readable JSON array. Default is a fixed-width table (NAME / VERSION / SOURCE_URL).

The script reads `mapping.yml` and prints `installed_skills`. Empty registries print `(no skills in registry)` in table mode and `[]` in `--json` mode.

Show the script's stdout output to the user.
