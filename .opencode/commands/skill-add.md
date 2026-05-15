---
description: Ingest a skill from a GitHub URL into this registry
---

Run the skill-add script with the user-provided arguments.

!`python src/internal/skill_add.py $ARGUMENTS`

Arguments:

- One positional URL is required. Accepted shapes:
  - Repo root: `https://github.com/<owner>/<repo>`
  - Folder: `https://github.com/<owner>/<repo>/tree/<ref>/<path>`
  - Blob: `https://github.com/<owner>/<repo>/blob/<ref>/<path>`
  - Raw: `https://raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>`
- Optional `--force` to overwrite an existing skill entry.

The script downloads the contents under `skills/<name>/`, scaffolds `manifest.yml` if missing (may prompt for description/version on a TTY), and upserts the entry in top-level `mapping.yml`.

Report the script's stdout summary back to the user. If the script exited non-zero, surface its stderr error message verbatim.
