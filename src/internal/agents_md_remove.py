#!/usr/bin/env python3
"""Remove an agents.md integration from the registry by name.

Deletes ``agents.md/<name>/`` and prunes the corresponding entry from
``mapping.yml``. ``--dry-run`` previews the deletion without touching
disk or the catalog.
"""

from __future__ import annotations

import argparse
import sys

from lib import (
    AGENTS_MD_KEY,
    IngestError,
    delete_dir,
    entry_dir,
    fail,
    find_entry,
    load_mapping,
    remove_entry,
    repo_root,
    save_mapping,
)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="agents_md_remove",
        description="Remove an agents.md integration from the registry by name.",
    )
    parser.add_argument(
        "--name",
        required=True,
        help="Name of the integration to remove (matches the agents.md/<name>/ folder).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview what would be deleted without touching disk.",
    )
    return parser


def _collect_files(folder) -> list[str]:
    """Return a sorted list of file paths under ``folder``, relative to it.

    Returns an empty list if the folder doesn't exist.
    """
    if not folder.exists():
        return []
    files = [p for p in folder.rglob("*") if p.is_file()]
    return sorted(str(p.relative_to(folder)) for p in files)


def run(name: str, dry_run: bool) -> int:
    root = repo_root()
    mapping = load_mapping(root)
    entries = mapping[AGENTS_MD_KEY]

    entry = find_entry(entries, name)
    if entry is None:
        raise IngestError(
            f"agents.md integration {name!r} not found in mapping.yml; "
            "nothing to remove"
        )

    folder = entry_dir("agents-md", name, root)
    rel_folder = folder.relative_to(root)
    files = _collect_files(folder)

    if dry_run:
        print(f"DRY-RUN: would remove agents.md integration {name!r}")
        print(
            f"  mapping.yml entry: name={entry.get('name')} "
            f"version={entry.get('version')} "
            f"source_url={entry.get('source_url')}"
        )
        if folder.exists():
            print(f"  folder: {rel_folder}/ ({len(files)} file(s))")
            for rel in files:
                print(f"    - {rel}")
        else:
            print(
                f"  folder: {rel_folder}/ (already missing; "
                "only the mapping entry would be removed)"
            )
        return 0

    if folder.exists():
        delete_dir(folder)
    else:
        print(
            f"warning: {rel_folder}/ already missing; "
            "pruning mapping entry only",
            file=sys.stderr,
        )

    remove_entry(entries, name)
    save_mapping(mapping, root)

    print(f"Removed agents.md integration {name!r}")
    print(f"  version: {entry.get('version')}")
    print(f"  source_url: {entry.get('source_url')}")
    if files:
        print(f"  deleted folder: {rel_folder}/ ({len(files)} file(s))")
        for rel in files:
            print(f"    - {rel}")
    return 0


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return run(args.name, args.dry_run)
    except IngestError as exc:
        fail(str(exc))
        return 1


if __name__ == "__main__":
    sys.exit(main())
