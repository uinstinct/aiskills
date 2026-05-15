---
name: agents-md-list
description: List all agents.md integrations currently in the registry
---

When the user asks to list the agents.md integrations currently in this registry (e.g. "run agents-md-list", "show me the agents.md integrations in the registry"), invoke the `agents_md_list.py` script with any flags the user supplied.

Run:

```bash
python src/internal/agents_md_list.py <ARGUMENTS>
```

Arguments (all optional):

- `--json` prints the list as a machine-readable JSON array. Default is a fixed-width table (NAME / VERSION / SOURCE_URL).

The script reads `mapping.yml` and prints `installed_agents_md`. Empty registries print `(no agents.md integrations in registry)` in table mode and `[]` in `--json` mode.

Show the script's stdout output to the user.
