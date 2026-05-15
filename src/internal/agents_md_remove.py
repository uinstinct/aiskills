#!/usr/bin/env python3
"""Remove an agents.md integration from the registry by name.

Deletes ``agents.md/<name>/`` and prunes ``installed_agents_md`` in
``mapping.yml``. US-019 ships the argparse skeleton; US-024 wires the
behavior.
"""

from __future__ import annotations

import argparse
import sys

from lib import IngestError, fail


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


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        raise IngestError(
            f"agents_md_remove not yet implemented (name: {args.name}, "
            f"dry_run: {args.dry_run}); skeleton in US-019, behavior in US-024"
        )
    except IngestError as exc:
        fail(str(exc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
