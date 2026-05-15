---
description: Ingest an agents.md integration from a GitHub URL into this registry
allowed-tools: Bash, Read
---

Run the agents-md-add script with the user-provided arguments.

Invoke:

```bash
python src/internal/agents_md_add.py $ARGUMENTS
```

Arguments:

- One positional URL is required. Accepted shapes:
  - Raw file: `https://raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>` (preferred for a single snippet)
  - Blob: `https://github.com/<owner>/<repo>/blob/<ref>/<path>`
  - Folder: `https://github.com/<owner>/<repo>/tree/<ref>/<path>` (must contain `snippet.md`)
  - Repo root: `https://github.com/<owner>/<repo>`
- Optional `--force` to overwrite an existing agents.md entry.

The script will:

1. Download the contents and write them under `agents.md/<name>/snippet.md` (plus any sibling files for folder ingests).
2. Scaffold `manifest.yml` if missing (may prompt for description, version, and `harness_compatibility` on a TTY).
3. Upsert the entry in `mapping.yml`'s `installed_agents_md` section.

Report the script's stdout summary back to the user. If the script exits non-zero, surface its stderr error message verbatim.
