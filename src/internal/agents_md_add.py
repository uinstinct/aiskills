#!/usr/bin/env python3
"""Ingest an agents.md integration from a GitHub URL into the registry.

Reads a GitHub URL (raw file or a folder containing snippet.md +
manifest.yml) and adds it under ``agents.md/<name>/``. The mapping.yml
catalog is updated. US-019 ships the argparse skeleton; US-023 wires up
the network + filesystem behavior.
"""

from __future__ import annotations

import argparse
import sys

from lib import IngestError, fail


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="agents_md_add",
        description="Ingest an agents.md integration from a GitHub URL.",
    )
    parser.add_argument(
        "url",
        help=(
            "GitHub URL: raw/blob single-file URL preferred, or a "
            "/tree/<ref>/<path> folder containing snippet.md + manifest.yml."
        ),
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite an existing agents.md/<name>/ entry without prompting.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        raise IngestError(
            f"agents_md_add not yet implemented (URL: {args.url}); "
            "skeleton in US-019, behavior in US-023"
        )
    except IngestError as exc:
        fail(str(exc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
