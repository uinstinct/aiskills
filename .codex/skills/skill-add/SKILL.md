---
name: skill-add
description: Ingest a skill from a GitHub URL into this registry
---

When the user asks to add or ingest a skill from a GitHub URL (e.g. "run skill-add with URL <url>", "ingest this skill: <url>"), invoke the `skill_add.py` script with the user-supplied arguments.

Run:

```bash
uv run --project src/internal python src/internal/skill_add.py <ARGUMENTS>
```

where `<ARGUMENTS>` is the URL the user provided, plus any optional flags they mentioned.

Arguments:

- One positional URL is required. Accepted shapes:
  - Repo root: `https://github.com/<owner>/<repo>`
  - Folder: `https://github.com/<owner>/<repo>/tree/<ref>/<path>`
  - Blob: `https://github.com/<owner>/<repo>/blob/<ref>/<path>`
  - Raw: `https://raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>`
- Optional `--force` to overwrite an existing skill entry.

The script will:

1. Download the contents and write them under `skills/<name>/`.
2. Scaffold `manifest.yml` if missing (may prompt for description/version on a TTY).
3. Upsert the entry in top-level `mapping.yml`.

Report the script's stdout summary back to the user. If the script exits non-zero, surface its stderr error message verbatim.
