#!/usr/bin/env python3
"""Scaffold a new local-only skill entry under ``assets/skills/<name>/``.

Interactive command that prompts for the manifest fields (description,
version, harness_compatibility), writes a starter ``SKILL.md`` and
``manifest.yml``, and registers the entry in top-level ``mapping.yml``
with ``source_url: local``.

Mirrors the shape of ``skill_add.py`` but does not touch the network —
the entry is locally authored, not ingested. Future ``skill_add --force``
or ``skill_remove`` calls treat it like any other registry entry.
"""

from __future__ import annotations

import argparse
import sys
from typing import Any

from lib import (
    ALLOWED_HARNESSES,
    MANIFEST_FILE_NAME,
    SKILLS_KEY,
    IngestError,
    delete_dir,
    entry_dir,
    fail,
    find_entry,
    install_path_for,
    is_semver,
    load_mapping,
    prompt_multi_choice,
    prompt_text,
    repo_root,
    sanitize_name,
    upsert_catalog_entry,
    write_manifest,
)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="skill_new",
        description=(
            "Create a new local-only skill entry under assets/skills/<name>/. "
            "Prompts for description, version, and harness_compatibility."
        ),
    )
    parser.add_argument(
        "name",
        help=(
            "Skill name. Will be sanitized to lowercase with [a-z0-9_-] "
            "characters only; matches the directory basename."
        ),
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite an existing assets/skills/<name>/ folder or mapping entry.",
    )
    return parser


def _require_interactive() -> None:
    """Fail with a clear error if invoked without a TTY.

    skill-new is interactive by design — every field is prompted for and
    there are no flags to supply them non-interactively. Bail early so
    the user gets one good error rather than a hang or a partial-write.
    """
    if not sys.stdin.isatty():
        raise IngestError(
            "skill-new requires an interactive TTY to prompt for "
            "description, version, and harness_compatibility; "
            "re-run from a terminal"
        )


def _prompt_description(name: str) -> str:
    while True:
        try:
            value = prompt_text(f"Description for skill '{name}'")
        except IngestError:
            print("description must not be empty", file=sys.stderr)
            continue
        if value.strip():
            return value.strip()
        print("description must not be empty", file=sys.stderr)


def _prompt_version() -> str:
    while True:
        value = prompt_text("Version", default="0.1.0")
        if is_semver(value):
            return value
        print(
            f"invalid semver: {value!r} (expected MAJOR.MINOR.PATCH)",
            file=sys.stderr,
        )


def _check_collision(name: str, force: bool) -> None:
    mapping = load_mapping()
    has_entry = find_entry(mapping[SKILLS_KEY], name) is not None
    dest = entry_dir("skill", name)
    has_dir = dest.exists()
    if (has_entry or has_dir) and not force:
        raise IngestError(
            f"skill {name!r} already exists "
            f"(mapping entry: {has_entry}, folder on disk: {has_dir}); "
            "re-run with --force to overwrite"
        )


def _write_skill_files(
    name: str,
    description: str,
    version: str,
    harness_compatibility: list[str],
) -> tuple[Any, list[str]]:
    dest = entry_dir("skill", name)
    if dest.exists():
        delete_dir(dest)
    dest.mkdir(parents=True, exist_ok=True)
    skill_md_body = f"# {name}\n\nTODO: describe the skill\n"
    (dest / "SKILL.md").write_text(skill_md_body, encoding="utf-8")
    manifest = {
        "name": name,
        "description": description,
        "version": version,
        "harness_compatibility": harness_compatibility,
        "entrypoint": "SKILL.md",
    }
    write_manifest(dest, manifest)
    return dest, ["SKILL.md", MANIFEST_FILE_NAME]


def _print_summary(
    name: str,
    dest: Any,
    written: list[str],
    version: str,
) -> None:
    root = repo_root()
    rel_dest = dest.relative_to(root)
    print(f"created skill {name!r} (version {version}) from local")
    print(f"  install_path: {install_path_for('skill', name)}")
    print(f"  wrote {len(written)} file(s) to {rel_dest}/:")
    for rel in sorted(written):
        print(f"    - {rel}")


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        _require_interactive()
        name = sanitize_name(args.name)
        _check_collision(name, args.force)
        description = _prompt_description(name)
        version = _prompt_version()
        harness_compatibility = prompt_multi_choice(
            f"harness_compatibility for '{name}' "
            "(comma-separated indices or labels; empty = all harnesses)",
            list(ALLOWED_HARNESSES),
        )
        dest, written = _write_skill_files(
            name, description, version, harness_compatibility
        )
        upsert_catalog_entry("skill", name, version, "local")
        _print_summary(name, dest, written, version)
    except IngestError as exc:
        fail(str(exc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
