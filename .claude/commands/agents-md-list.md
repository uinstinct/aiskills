---
description: List all agents.md integrations currently in the registry
allowed-tools: Bash, Read
---

Run the agents-md-list script with the user-provided arguments.

Invoke:

```bash
python src/internal/agents_md_list.py $ARGUMENTS
```

Arguments (all optional):

- `--json` prints the list as a machine-readable JSON array. Default is a fixed-width table (NAME / VERSION / SOURCE_URL).

The script reads `mapping.yml` and prints `installed_agents_md`. Empty registries print `(no agents.md integrations in registry)` in table mode and `[]` in `--json` mode.

Show the script's stdout output to the user.
