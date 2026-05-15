#!/usr/bin/env python3
"""Ingest a skill from a GitHub URL into the registry.

Accepts one of three URL shapes:

* **Repo root** ``https://github.com/<owner>/<repo>`` — fetches the
  recursive tree, looks for a top-level skill folder (containing
  ``SKILL.md`` or ``manifest.yml``) or for those files at the repo root.
  Prompts to pick if multiple top-level skills exist.
* **Folder** ``https://github.com/<owner>/<repo>/tree/<ref>/<path>`` —
  imports that subtree as a single skill named after the last path
  segment.
* **Raw file** ``https://github.com/<owner>/<repo>/blob/<ref>/<path>``
  or ``https://raw.githubusercontent.com/...`` — downloads the single
  file and prompts whether to add it as a skill, an agents.md
  integration, or both.

In every case the script:

1. Writes contents under ``skills/<name>/`` (and/or
   ``agents.md/<name>/`` for the "both" branch).
2. Scaffolds ``manifest.yml`` if the source didn't provide one,
   prompting for ``description`` and ``version``.
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
    SKILLS_KEY,
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
    prompt_text,
    repo_root,
    sanitize_name,
    scaffold_agents_md_manifest,
    scaffold_skill_manifest,
    upsert_catalog_entry,
)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="skill_add",
        description="Ingest a skill from a GitHub URL into the registry.",
    )
    parser.add_argument(
        "url",
        help=(
            "GitHub URL: repo root, /tree/<ref>/<path> folder, "
            "or a /blob/.../path or raw.githubusercontent.com single-file URL. "
            "Refs containing '/' are not currently supported."
        ),
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Overwrite existing skills/<name>/ or agents.md/<name>/ entries.",
    )
    parser.add_argument(
        "--as",
        dest="as_kind",
        choices=("skill", "agents-md", "both"),
        default=None,
        help=(
            "For single-file blob/raw URLs, install as a skill, an "
            "agents.md integration, or both. Without this flag the script "
            "prompts on a TTY and otherwise infers from the filename "
            "(SKILL.md -> skill, AGENTS.md/snippet.md -> agents-md)."
        ),
    )
    return parser


# ---------------------------------------------------------------------------
# Top-level dispatch
# ---------------------------------------------------------------------------


def _has_skill_marker(tree: list[dict[str, Any]], prefix: str) -> bool:
    """Return True if ``tree`` has SKILL.md or manifest.yml at ``prefix``."""
    targets = {
        f"{prefix}SKILL.md" if prefix else "SKILL.md",
        f"{prefix}manifest.yml" if prefix else "manifest.yml",
    }
    return any(
        e.get("type") == "blob" and e.get("path") in targets for e in tree
    )


def _find_top_level_skill_candidates(
    tree: list[dict[str, Any]],
) -> list[str]:
    """Return paths of skill folders at depth 1 (and "" for repo-root)."""
    candidates: set[str] = set()
    if _has_skill_marker(tree, ""):
        candidates.add("")
    for entry in tree:
        path = entry.get("path", "")
        if entry.get("type") != "blob":
            continue
        parts = path.split("/")
        if len(parts) == 2 and parts[1] in ("SKILL.md", "manifest.yml"):
            candidates.add(parts[0])
    return sorted(candidates)


def _ingest_from_repo(
    parsed: ParsedGithubUrl, source_url: str, force: bool
) -> int:
    ref = get_default_branch(parsed.owner, parsed.repo)
    tree = list_tree_recursive(parsed.owner, parsed.repo, ref)
    candidates = _find_top_level_skill_candidates(tree)
    if not candidates:
        raise IngestError(
            f"no skill folder found in {parsed.owner}/{parsed.repo} "
            "(looked for SKILL.md or manifest.yml at the root or one level deep)"
        )
    if len(candidates) == 1:
        chosen = candidates[0]
    else:
        labels = [c if c else "(repo root)" for c in candidates]
        picked = prompt_choice(
            f"Multiple skill candidates in {parsed.owner}/{parsed.repo}:",
            labels,
        )
        chosen = "" if picked == "(repo root)" else picked
    if chosen:
        skill_name = sanitize_name(chosen)
    else:
        skill_name = sanitize_name(parsed.repo)
    return _ingest_skill_folder(
        parsed.owner,
        parsed.repo,
        ref,
        chosen,
        skill_name,
        source_url,
        force,
        tree=tree,
    )


def _ingest_from_folder(
    parsed: ParsedGithubUrl, source_url: str, force: bool
) -> int:
    last_segment = parsed.path.rstrip("/").rsplit("/", 1)[-1] or parsed.repo
    skill_name = sanitize_name(last_segment)
    assert parsed.ref is not None
    return _ingest_skill_folder(
        parsed.owner,
        parsed.repo,
        parsed.ref,
        parsed.path,
        skill_name,
        source_url,
        force,
    )


def _infer_kind_from_filename(filename: str) -> str | None:
    """Return 'skill' or 'agents-md' if the filename is unambiguous, else None.

    SKILL.md is the canonical skill entrypoint; AGENTS.md and snippet.md
    are the canonical agents.md filenames. Anything else is ambiguous
    and the caller should prompt or fail.
    """
    lower = filename.lower()
    if lower == "skill.md":
        return "skill"
    if lower in ("agents.md", "snippet.md"):
        return "agents-md"
    return None


def _ingest_from_single_file(
    parsed: ParsedGithubUrl, source_url: str, force: bool, as_kind: str | None
) -> int:
    assert parsed.ref is not None
    filename = parsed.path.rsplit("/", 1)[-1]
    if not filename:
        raise IngestError(f"raw URL points to a directory, not a file: {source_url}")
    # For canonical entrypoint filenames (SKILL.md, AGENTS.md, snippet.md)
    # the filename itself is not a useful name — prefer the parent folder.
    parent = parsed.path.rsplit("/", 2)[-2] if "/" in parsed.path else ""
    if filename.lower() in ("skill.md", "agents.md", "snippet.md") and parent:
        name_hint = parent
    else:
        name_hint = filename.rsplit(".", 1)[0] if "." in filename else filename
    if as_kind is not None:
        target_kind = as_kind
    elif sys.stdin.isatty():
        target_kind = prompt_choice(
            f"Add {filename!r} as:",
            ["skill", "agents-md", "both"],
        )
    else:
        inferred = _infer_kind_from_filename(filename)
        if inferred is None:
            raise IngestError(
                f"cannot infer install kind for {filename!r} without a TTY; "
                "re-run with --as skill | --as agents-md | --as both"
            )
        target_kind = inferred
    body = fetch_raw_file(parsed.owner, parsed.repo, parsed.ref, parsed.path)
    actions: list[str] = (
        ["skill", "agents-md"] if target_kind == "both" else [target_kind]
    )
    for action in actions:
        skill_name = sanitize_name(name_hint)
        if action == "skill":
            _install_single_file_as_skill(
                skill_name, body, source_url, force, original_filename=filename
            )
        else:
            _install_single_file_as_agents_md(
                skill_name, body, source_url, force
            )
    return 0


# ---------------------------------------------------------------------------
# Folder ingest path
# ---------------------------------------------------------------------------


def _check_skill_collision(name: str, force: bool) -> None:
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


def _check_agents_md_collision(name: str, force: bool) -> None:
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


def _ingest_skill_folder(
    owner: str,
    repo: str,
    ref: str,
    src_path: str,
    skill_name: str,
    source_url: str,
    force: bool,
    *,
    tree: list[dict[str, Any]] | None = None,
) -> int:
    _check_skill_collision(skill_name, force)
    dest = entry_dir("skill", skill_name)
    if dest.exists():
        delete_dir(dest)
    written = fetch_folder_to_disk(
        owner, repo, ref, src_path, dest, tree=tree
    )
    manifest, scaffolded = scaffold_skill_manifest(dest, skill_name)
    if scaffolded:
        written.append("manifest.yml")
    # Ensure name matches folder; build.rs requires this invariant.
    if manifest.get("name") != skill_name:
        manifest["name"] = skill_name
        _rewrite_manifest(dest, manifest)
    version = str(manifest.get("version") or "0.1.0")
    upsert_catalog_entry("skill", skill_name, version, source_url)
    _print_summary("skill", skill_name, dest, written, source_url, version)
    return 0


def _install_single_file_as_skill(
    skill_name: str,
    body: bytes,
    source_url: str,
    force: bool,
    *,
    original_filename: str,
) -> None:
    _check_skill_collision(skill_name, force)
    dest = entry_dir("skill", skill_name)
    if dest.exists():
        delete_dir(dest)
    dest.mkdir(parents=True, exist_ok=True)
    target_filename = "SKILL.md" if original_filename.lower().endswith(".md") else original_filename
    (dest / target_filename).write_bytes(body)
    written = [target_filename]
    manifest, scaffolded = scaffold_skill_manifest(dest, skill_name)
    if scaffolded:
        written.append("manifest.yml")
    if manifest.get("name") != skill_name:
        manifest["name"] = skill_name
        _rewrite_manifest(dest, manifest)
    # If the source file wasn't named SKILL.md the manifest entrypoint must
    # point to whatever we wrote so build.rs still finds the entry.
    if target_filename != "SKILL.md":
        manifest["entrypoint"] = target_filename
        _rewrite_manifest(dest, manifest)
    version = str(manifest.get("version") or "0.1.0")
    upsert_catalog_entry("skill", skill_name, version, source_url)
    _print_summary("skill", skill_name, dest, written, source_url, version)


def _install_single_file_as_agents_md(
    name: str, body: bytes, source_url: str, force: bool
) -> None:
    _check_agents_md_collision(name, force)
    dest = entry_dir("agents-md", name)
    if dest.exists():
        delete_dir(dest)
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "snippet.md").write_bytes(body)
    written = ["snippet.md"]
    manifest, scaffolded = scaffold_agents_md_manifest(dest, name)
    if scaffolded:
        written.append("manifest.yml")
    if manifest.get("name") != name:
        manifest["name"] = name
        _rewrite_manifest(dest, manifest)
    version = str(manifest.get("version") or "0.1.0")
    upsert_catalog_entry("agents-md", name, version, source_url)
    _print_summary("agents-md", name, dest, written, source_url, version)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _rewrite_manifest(dest: Path, manifest: dict[str, Any]) -> None:
    with (dest / MANIFEST_FILE_NAME).open("w", encoding="utf-8") as fh:
        yaml.safe_dump(manifest, fh, sort_keys=False, default_flow_style=False)


def _print_summary(
    kind: str,
    name: str,
    dest: Path,
    written: list[str],
    source_url: str,
    version: str,
) -> None:
    root = repo_root()
    rel_dest = dest.relative_to(root)
    print(f"added {kind} {name!r} (version {version}) from {source_url}")
    print(f"  install_path: {install_path_for(kind, name)}")
    print(f"  wrote {len(written)} file(s) to {rel_dest}/:")
    for rel in sorted(written):
        print(f"    - {rel}")


# ---------------------------------------------------------------------------
# Entrypoint
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        parsed = parse_github_url(args.url)
        if parsed.kind == "repo":
            return _ingest_from_repo(parsed, args.url, args.force)
        if parsed.kind == "folder":
            return _ingest_from_folder(parsed, args.url, args.force)
        if parsed.kind in ("blob", "raw"):
            return _ingest_from_single_file(
                parsed, args.url, args.force, args.as_kind
            )
        raise IngestError(f"unsupported URL kind: {parsed.kind}")
    except IngestError as exc:
        fail(str(exc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
