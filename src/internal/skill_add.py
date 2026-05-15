#!/usr/bin/env python3
"""Ingest a skill from a GitHub URL into the registry.

Reads a GitHub URL (repo root, folder, or single raw file), fetches the
contents, and adds them under ``skills/<name>/`` in this registry. The
mapping.yml catalog is updated to record the new entry.

Implemented incrementally; see US-020 for the full behavior. US-019 lands
the argparse skeleton — actual ingest logic lives in the later story.
"""

from __future__ import annotations

import argparse
import sys

from lib import IngestError, fail


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="skill_add",
        description="Ingest a skill from a GitHub URL into the registry.",
    )
    parser.add_argument(
        "url",
        help=(
            "GitHub URL: repo root, /tree/<ref>/<path> folder, "
            "or a raw/blob single file URL."
        ),
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite an existing skills/<name>/ entry without prompting.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        raise IngestError(
            f"skill_add not yet implemented (URL: {args.url}); "
            "this script ships its skeleton in US-019 and is filled in by US-020"
        )
    except IngestError as exc:
        fail(str(exc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
