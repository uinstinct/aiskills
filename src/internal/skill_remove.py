#!/usr/bin/env python3
"""Remove a skill from the registry by name.

Deletes ``skills/<name>/`` and prunes the corresponding entry from
``mapping.yml``. US-019 lands the argparse skeleton; the deletion logic is
fleshed out in US-021.
"""

from __future__ import annotations

import argparse
import sys

from lib import IngestError, fail


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="skill_remove",
        description="Remove a skill from the registry by name.",
    )
    parser.add_argument(
        "--name",
        required=True,
        help="Name of the skill to remove (must match its folder under skills/).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview what would be deleted without touching disk.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        raise IngestError(
            f"skill_remove not yet implemented (name: {args.name}, "
            f"dry_run: {args.dry_run}); skeleton in US-019, behavior in US-021"
        )
    except IngestError as exc:
        fail(str(exc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
