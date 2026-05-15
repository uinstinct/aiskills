# Internal ingest scripts

This folder holds the Python scripts that back the maintainer-facing slash
commands in this registry repo (`/skill-add`, `/skill-remove`, etc.; see
US-025 / US-026 / US-027 for the per-harness command wiring).

Every script:

- Uses `argparse` and supports `--help`.
- Exits with a non-zero status on failure and prints a one-line `error: ...`
  message to stderr.
- Imports shared helpers (mapping.yml read/write, repo-root lookup,
  catalog-entry shapes) from [`lib.py`](./lib.py) — keep ingest-specific
  one-offs out of `lib.py`.
- Lives at the top level of this folder so it can be invoked as
  `python src/internal/<script>.py ...` from the repo root.

## Setup

```sh
python3 -m pip install -r src/internal/requirements.txt
```

Only one runtime dep: `pyyaml`. Everything else is stdlib (`argparse`,
`json`, `pathlib`, ...).

## Scripts

All scripts are invoked with `python src/internal/<script>.py` from the
registry repo root. Pass `--help` to any of them for the canonical
arg list.

| Script                | Purpose                                              | Filled in by |
| --------------------- | ---------------------------------------------------- | ------------ |
| `skill_add.py`        | Ingest a skill from a GitHub URL                     | US-020       |
| `skill_remove.py`     | Remove a skill from the registry by name             | US-021       |
| `skill_list.py`       | Print every skill currently in the registry         | US-022       |
| `agents_md_add.py`    | Ingest an agents.md integration from a GitHub URL    | US-023       |
| `agents_md_remove.py` | Remove an agents.md integration from the registry    | US-024       |
| `agents_md_list.py`   | Print every agents.md integration in the registry    | US-024       |

### `skill_add.py`

```
skill_add.py [--force] <url>
```

- `url` (positional, required): GitHub URL — repo root, `/tree/<ref>/<path>`
  folder, or a raw/blob single-file URL.
- `--force`: overwrite an existing `skills/<name>/` entry without prompting.

### `skill_remove.py`

```
skill_remove.py --name <skill_name> [--dry-run]
```

- `--name` (required): name of the skill to remove. Must match the folder
  basename under `skills/` and the entry name in `mapping.yml`.
- `--dry-run`: print what would be deleted; do not touch disk.

### `skill_list.py`

```
skill_list.py [--json]
```

- No required arguments. Defaults to a text table with columns
  `NAME / VERSION / SOURCE_URL`.
- `--json`: emit a machine-readable JSON array of the `installed_skills`
  entries from `mapping.yml`.

### `agents_md_add.py`

```
agents_md_add.py [--force] <url>
```

- `url` (positional, required): GitHub URL — single-file raw/blob URL
  preferred, or a `/tree/<ref>/<path>` folder containing `snippet.md` and
  `manifest.yml`.
- `--force`: overwrite an existing `agents.md/<name>/` entry without
  prompting.

### `agents_md_remove.py`

```
agents_md_remove.py --name <name> [--dry-run]
```

- `--name` (required): name of the agents.md integration to remove.
- `--dry-run`: print what would be deleted; do not touch disk.

### `agents_md_list.py`

```
agents_md_list.py [--json]
```

- No required arguments. Text table by default; `--json` for the
  machine-readable form.

## Exit codes

- `0` — success.
- `1` — user-facing failure (printed as `error: <message>` on stderr).
- `2` — `argparse` rejected the invocation (e.g. missing `--name`).

## `lib.py`

Common helpers used by every script:

- `repo_root()` — locate the registry root by walking up to the directory
  containing `mapping.yml`. Throws `IngestError` if not inside the repo.
- `load_mapping()` / `save_mapping(data)` — read and write `mapping.yml`.
  `save_mapping` writes via a temp file + rename and sorts entries within
  each section by name so diffs stay readable.
- `find_entry(entries, name)` / `remove_entry(entries, name)` — list
  helpers for the two catalog sections.
- `entry_dir(kind, name)` / `install_path_for(kind, name)` — resolve the
  on-disk folder and the `install_path` string for a catalog entry.
  `kind` is `"skill"` or `"agents-md"`.
- `IngestError` — the only exception scripts raise for user-facing errors;
  the top-level handler catches it and exits with `fail(msg)`.

## Typecheck

The only check the scripts gate on is `py_compile`:

```sh
python -m py_compile src/internal/*.py
```

CI (US-028) runs this as the Python lint step.
