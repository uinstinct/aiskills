#!/usr/bin/env python3
"""Ingest an agents.md integration from a GitHub URL into the registry.

Accepts one of three URL shapes:

* **Raw file** ``https://github.com/<owner>/<repo>/blob/<ref>/<path>``
  or ``https://raw.githubusercontent.com/...`` — downloads the file as
  ``snippet.md`` (preferred input per AC). When the file is a
  ``SKILL.md`` the entry name is taken from its parent folder (the
  skill directory) rather than from the literal filename.
* **Folder** ``https://github.com/<owner>/<repo>/tree/<ref>/<path>`` —
  imports the subtree (expected to contain ``snippet.md`` and
  optionally ``manifest.yml``).
* **Repo root** ``https://github.com/<owner>/<repo>`` — walks the
  recursive tree for top-level folders containing ``snippet.md`` or
  ``manifest.yml`` (or those files at the repo root) and prompts to
  pick when there's more than one.

In every case the script:

1. Writes contents under ``agents.md/<name>/`` (snippet.md + manifest.yml).
2. Scaffolds ``manifest.yml`` if the source didn't provide one,
   prompting for ``description``, ``version``, and
   ``harness_compatibility``.
3. Adds or replaces the entry in top-level ``mapping.yml``.
4. Refuses to clobber an existing entry without ``--force``.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Any

import yaml

from lib import (
    AGENTS_MD_KEY,
    MANIFEST_FILE_NAME,
    IngestError,
    ParsedGithubUrl,
    delete_dir,
    entry_dir,
    fail,
    fetch_folder_to_disk,
    fetch_raw_file,
    find_entry,
    get_default_branch,
    install_path_for,
    list_tree_recursive,
    load_mapping,
    parse_github_url,
    prompt_choice,
    repo_root,
    sanitize_name,
    scaffold_agents_md_manifest,
    upsert_catalog_entry,
)

SNIPPET_FILE_NAME = "snippet.md"


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="agents_md_add",
        description="Ingest an agents.md integration from a GitHub URL.",
    )
    parser.add_argument(
        "url",
        help=(
            "GitHub URL: raw/blob single-file URL (preferred), "
            "/tree/<ref>/<path> folder containing snippet.md + manifest.yml, "
            "or repo root for tree discovery. "
            "Refs containing '/' are not currently supported."
        ),
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite an existing agents.md/<name>/ entry.",
    )
    return parser


# ---------------------------------------------------------------------------
# Top-level dispatch
# ---------------------------------------------------------------------------


def _has_agents_md_marker(tree: list[dict[str, Any]], prefix: str) -> bool:
    """Return True if ``tree`` has snippet.md or manifest.yml at ``prefix``."""
    targets = {
        f"{prefix}{SNIPPET_FILE_NAME}" if prefix else SNIPPET_FILE_NAME,
        f"{prefix}{MANIFEST_FILE_NAME}" if prefix else MANIFEST_FILE_NAME,
    }
    return any(
        e.get("type") == "blob" and e.get("path") in targets for e in tree
    )


def _find_top_level_candidates(tree: list[dict[str, Any]]) -> list[str]:
    """Return paths of agents.md folders at depth 1 (and "" for repo-root)."""
    candidates: set[str] = set()
    if _has_agents_md_marker(tree, ""):
        candidates.add("")
    for entry in tree:
        path = entry.get("path", "")
        if entry.get("type") != "blob":
            continue
        parts = path.split("/")
        if len(parts) == 2 and parts[1] in (SNIPPET_FILE_NAME, MANIFEST_FILE_NAME):
            candidates.add(parts[0])
    return sorted(candidates)


def _ingest_from_repo(
    parsed: ParsedGithubUrl, source_url: str, force: bool
) -> int:
    ref = get_default_branch(parsed.owner, parsed.repo)
    tree = list_tree_recursive(parsed.owner, parsed.repo, ref)
    candidates = _find_top_level_candidates(tree)
    if not candidates:
        raise IngestError(
            f"no agents.md integration found in {parsed.owner}/{parsed.repo} "
            "(looked for snippet.md or manifest.yml at the root or one level deep)"
        )
    if len(candidates) == 1:
        chosen = candidates[0]
    else:
        labels = [c if c else "(repo root)" for c in candidates]
        picked = prompt_choice(
            f"Multiple agents.md candidates in {parsed.owner}/{parsed.repo}:",
            labels,
        )
        chosen = "" if picked == "(repo root)" else picked
    name = sanitize_name(chosen or parsed.repo)
    return _ingest_folder(
        parsed.owner,
        parsed.repo,
        ref,
        chosen,
        name,
        source_url,
        force,
        tree=tree,
    )


def _ingest_from_folder(
    parsed: ParsedGithubUrl, source_url: str, force: bool
) -> int:
    last_segment = parsed.path.rstrip("/").rsplit("/", 1)[-1] or parsed.repo
    name = sanitize_name(last_segment)
    assert parsed.ref is not None
    return _ingest_folder(
        parsed.owner,
        parsed.repo,
        parsed.ref,
        parsed.path,
        name,
        source_url,
        force,
    )


def _ingest_from_single_file(
    parsed: ParsedGithubUrl, source_url: str, force: bool
) -> int:
    assert parsed.ref is not None
    filename = parsed.path.rsplit("/", 1)[-1]
    if not filename:
        raise IngestError(f"raw URL points to a directory, not a file: {source_url}")
    if filename.lower() == "skill.md":
        # SKILL.md is a marker filename — every skill has one, so deriving
        # the name from it would collapse every ingest onto "skill". Use the
        # parent folder (the skill directory) instead, or the repo name when
        # SKILL.md sits at the repo root.
        parent = parsed.path.rsplit("/", 1)[0] if "/" in parsed.path else ""
        name_hint = parent.rsplit("/", 1)[-1] if parent else parsed.repo
    else:
        name_hint = filename.rsplit(".", 1)[0] if "." in filename else filename
    name = sanitize_name(name_hint)
    body = fetch_raw_file(parsed.owner, parsed.repo, parsed.ref, parsed.path)
    _install_single_file(name, body, source_url, force)
    return 0


# ---------------------------------------------------------------------------
# Folder ingest path
# ---------------------------------------------------------------------------


def _check_collision(name: str, force: bool) -> None:
    mapping = load_mapping()
    has_entry = find_entry(mapping[AGENTS_MD_KEY], name) is not None
    dest = entry_dir("agents-md", name)
    has_dir = dest.exists()
    if (has_entry or has_dir) and not force:
        raise IngestError(
            f"agents.md integration {name!r} already exists "
            f"(mapping entry: {has_entry}, folder on disk: {has_dir}); "
            "re-run with --force to overwrite"
        )


def _ingest_folder(
    owner: str,
    repo: str,
    ref: str,
    src_path: str,
    name: str,
    source_url: str,
    force: bool,
    *,
    tree: list[dict[str, Any]] | None = None,
) -> int:
    _check_collision(name, force)
    dest = entry_dir("agents-md", name)
    if dest.exists():
        delete_dir(dest)
    written = fetch_folder_to_disk(owner, repo, ref, src_path, dest, tree=tree)
    if SNIPPET_FILE_NAME not in written:
        raise IngestError(
            f"folder {owner}/{repo}@{ref}:{src_path or '/'} does not "
            f"contain {SNIPPET_FILE_NAME}; cannot ingest as agents.md integration"
        )
    manifest, scaffolded = scaffold_agents_md_manifest(dest, name)
    if scaffolded:
        written.append(MANIFEST_FILE_NAME)
    if manifest.get("name") != name:
        manifest["name"] = name
        _rewrite_manifest(dest, manifest)
    version = str(manifest.get("version") or "0.1.0")
    upsert_catalog_entry("agents-md", name, version, source_url)
    _print_summary(name, dest, written, source_url, version)
    return 0


def _install_single_file(
    name: str, body: bytes, source_url: str, force: bool
) -> None:
    _check_collision(name, force)
    dest = entry_dir("agents-md", name)
    if dest.exists():
        delete_dir(dest)
    dest.mkdir(parents=True, exist_ok=True)
    (dest / SNIPPET_FILE_NAME).write_bytes(body)
    written = [SNIPPET_FILE_NAME]
    manifest, scaffolded = scaffold_agents_md_manifest(dest, name)
    if scaffolded:
        written.append(MANIFEST_FILE_NAME)
    if manifest.get("name") != name:
        manifest["name"] = name
        _rewrite_manifest(dest, manifest)
    version = str(manifest.get("version") or "0.1.0")
    upsert_catalog_entry("agents-md", name, version, source_url)
    _print_summary(name, dest, written, source_url, version)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _rewrite_manifest(dest: Path, manifest: dict[str, Any]) -> None:
    with (dest / MANIFEST_FILE_NAME).open("w", encoding="utf-8") as fh:
        yaml.safe_dump(manifest, fh, sort_keys=False, default_flow_style=False)


def _print_summary(
    name: str,
    dest: Path,
    written: list[str],
    source_url: str,
    version: str,
) -> None:
    root = repo_root()
    rel_dest = dest.relative_to(root)
    print(f"added agents-md {name!r} (version {version}) from {source_url}")
    print(f"  install_path: {install_path_for('agents-md', name)}")
    print(f"  wrote {len(written)} file(s) to {rel_dest}/:")
    for rel in sorted(written):
        print(f"    - {rel}")


# ---------------------------------------------------------------------------
# Entrypoint
# ---------------------------------------------------------------------------


_DISPATCH = {
    "repo": _ingest_from_repo,
    "folder": _ingest_from_folder,
    "blob": _ingest_from_single_file,
    "raw": _ingest_from_single_file,
}


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        parsed = parse_github_url(args.url)
        handler = _DISPATCH.get(parsed.kind)
        if handler is None:
            raise IngestError(f"unsupported URL kind: {parsed.kind}")
        return handler(parsed, args.url, args.force)
    except IngestError as exc:
        fail(str(exc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
