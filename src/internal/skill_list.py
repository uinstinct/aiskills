#!/usr/bin/env python3
"""List every skill currently in the registry.

Reads ``mapping.yml`` and prints a tabular (default) or JSON
(``--json``) listing of the ``installed_skills`` section.
"""

from __future__ import annotations

import argparse
import json
import sys

from lib import SKILLS_KEY, IngestError, fail, load_mapping


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="skill_list",
        description="Print every skill currently in the registry.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable JSON instead of a text table.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        mapping = load_mapping()
        entries = mapping[SKILLS_KEY]
        if args.json:
            json.dump(entries, sys.stdout, indent=2, sort_keys=True)
            sys.stdout.write("\n")
            return 0
        if not entries:
            print("(no skills in registry)")
            return 0
        print(f"{'NAME':<32} {'VERSION':<12} SOURCE_URL")
        for entry in entries:
            print(
                f"{entry.get('name', ''):<32} "
                f"{entry.get('version', ''):<12} "
                f"{entry.get('source_url', '')}"
            )
        return 0
    except IngestError as exc:
        fail(str(exc))
        return 1


if __name__ == "__main__":
    sys.exit(main())
