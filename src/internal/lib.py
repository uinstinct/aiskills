"""Shared helpers for the instinctagents internal ingest scripts.

Every script under ``src/internal/`` calls into this module for the things
they all need: locating the registry repo root, reading and writing the
top-level ``mapping.yml`` deterministically, and resolving the on-disk
folder for a catalog entry. Keeping the helpers here lets the per-operation
scripts (``skill_add.py``, ``skill_remove.py``, ...) stay small and focused
on argparse plumbing + their one job.

See ``docs/mapping-schema.md`` for the catalog file format and
``docs/manifest-schema.md`` for the per-entry manifest.
"""

from __future__ import annotations

import shutil
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

MAPPING_FILE_NAME = "mapping.yml"
SKILLS_DIR_NAME = "skills"
AGENTS_MD_DIR_NAME = "agents.md"
MANIFEST_FILE_NAME = "manifest.yml"

SKILLS_KEY = "installed_skills"
AGENTS_MD_KEY = "installed_agents_md"

ALLOWED_HARNESSES = ("claude-code", "codex", "opencode")


class IngestError(Exception):
    """Raised when an ingest script hits a recoverable, user-facing error.

    Scripts catch this at the top level and exit nonzero with the message
    on stderr. Anything else propagates as an uncaught exception (a
    programmer bug) so the traceback is preserved.
    """


@dataclass(frozen=True)
class CatalogEntry:
    name: str
    version: str
    source_url: str
    install_path: str


def repo_root() -> Path:
    """Return the registry repo root, located via ``mapping.yml``.

    Walks up from this file until a directory containing ``mapping.yml`` is
    found. The internal scripts live at ``<root>/src/internal/`` so this is
    always two parents up, but the walk is more resilient if the layout
    ever shifts.
    """
    here = Path(__file__).resolve().parent
    for candidate in (here, *here.parents):
        if (candidate / MAPPING_FILE_NAME).is_file():
            return candidate
    raise IngestError(
        f"could not find {MAPPING_FILE_NAME} above {here}; "
        "are you running from inside the registry repo?"
    )


def mapping_path(root: Path | None = None) -> Path:
    return (root or repo_root()) / MAPPING_FILE_NAME


def load_mapping(root: Path | None = None) -> dict[str, list[dict[str, Any]]]:
    """Load ``mapping.yml`` and return it as a plain dict.

    Guarantees both top-level keys exist as lists (possibly empty), so
    callers can mutate ``data[SKILLS_KEY]`` / ``data[AGENTS_MD_KEY]``
    without first checking the shape.
    """
    path = mapping_path(root)
    try:
        with path.open("r", encoding="utf-8") as fh:
            raw = yaml.safe_load(fh)
    except FileNotFoundError as exc:
        raise IngestError(f"{path} not found") from exc
    except yaml.YAMLError as exc:
        raise IngestError(f"{path} is not valid YAML: {exc}") from exc

    if raw is None:
        raw = {}
    if not isinstance(raw, dict):
        raise IngestError(f"{path} top-level must be a mapping")

    for key in (SKILLS_KEY, AGENTS_MD_KEY):
        value = raw.get(key, [])
        if value is None:
            value = []
        if not isinstance(value, list):
            raise IngestError(f"{path}: '{key}' must be a list")
        raw[key] = value
    return raw


def save_mapping(
    data: dict[str, list[dict[str, Any]]],
    root: Path | None = None,
) -> None:
    """Write ``mapping.yml`` back, sorted by name within each section.

    Sorts inside each list so diffs stay readable. Preserves key order at
    the top level (``installed_skills`` before ``installed_agents_md``) by
    rebuilding the dict explicitly.
    """
    path = mapping_path(root)
    ordered: dict[str, list[dict[str, Any]]] = {
        SKILLS_KEY: sorted(data.get(SKILLS_KEY, []), key=lambda e: e["name"]),
        AGENTS_MD_KEY: sorted(data.get(AGENTS_MD_KEY, []), key=lambda e: e["name"]),
    }
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("w", encoding="utf-8") as fh:
        yaml.safe_dump(ordered, fh, sort_keys=False, default_flow_style=False)
    tmp.replace(path)


def find_entry(
    entries: list[dict[str, Any]],
    name: str,
) -> dict[str, Any] | None:
    for entry in entries:
        if entry.get("name") == name:
            return entry
    return None


def remove_entry(entries: list[dict[str, Any]], name: str) -> bool:
    """Remove the first entry matching ``name``. Returns True if found."""
    for idx, entry in enumerate(entries):
        if entry.get("name") == name:
            del entries[idx]
            return True
    return False


def entry_dir(kind: str, name: str, root: Path | None = None) -> Path:
    """Return the on-disk folder for a catalog entry.

    ``kind`` is ``"skill"`` or ``"agents-md"``.
    """
    root = root or repo_root()
    if kind == "skill":
        return root / SKILLS_DIR_NAME / name
    if kind == "agents-md":
        return root / AGENTS_MD_DIR_NAME / name
    raise IngestError(f"unknown entry kind: {kind!r}")


def install_path_for(kind: str, name: str) -> str:
    """Return the ``install_path`` string for a catalog entry."""
    if kind == "skill":
        return f"{SKILLS_DIR_NAME}/{name}/"
    if kind == "agents-md":
        return f"{AGENTS_MD_DIR_NAME}/{name}/"
    raise IngestError(f"unknown entry kind: {kind!r}")


def delete_dir(path: Path) -> None:
    """Recursively delete a directory. No-op if it doesn't exist."""
    if path.exists():
        shutil.rmtree(path)


def fail(message: str, code: int = 1) -> None:
    """Print ``message`` to stderr and exit with ``code``.

    Scripts call this from their top-level ``IngestError`` handler so the
    user-facing failure surface is one line + nonzero exit.
    """
    print(f"error: {message}", file=sys.stderr)
    sys.exit(code)
