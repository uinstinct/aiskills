---
description: List all skills currently in the registry
allowed-tools: Bash, Read
---

Run the skill-list script with the user-provided arguments.

Invoke:

```bash
uv run --project src/internal python src/internal/skill_list.py $ARGUMENTS
```

Arguments (all optional):

- `--json` prints the list as a machine-readable JSON array. Default is a fixed-width table (NAME / VERSION / SOURCE_URL).

The script reads `mapping.yml` and prints `installed_skills`. Empty registries print `(no skills in registry)` in table mode and `[]` in `--json` mode.

Show the script's stdout output to the user.
