"""Shared helpers for the instinctagents internal ingest scripts.

Every script under ``src/internal/`` calls into this module for the things
they all need: locating the registry repo root, reading and writing the
top-level ``mapping.yml`` deterministically, resolving on-disk folders
for catalog entries, talking to the GitHub API, and scaffolding new
manifests. Keeping the helpers here lets the per-operation scripts stay
small and focused on argparse plumbing + their one job.

See ``docs/mapping-schema.md`` for the catalog file format and
``docs/manifest-schema.md`` for the per-entry manifest.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlparse

import yaml

MAPPING_FILE_NAME = "mapping.yml"
SKILLS_DIR_NAME = "skills"
AGENTS_MD_DIR_NAME = "agents.md"
MANIFEST_FILE_NAME = "manifest.yml"

SKILLS_KEY = "installed_skills"
AGENTS_MD_KEY = "installed_agents_md"

ALLOWED_HARNESSES = ("claude-code", "codex", "opencode")

USER_AGENT = "instinctagents-ingest/1"
GITHUB_API = "https://api.github.com"
GITHUB_RAW = "https://raw.githubusercontent.com"
HTTP_TIMEOUT_SECONDS = 30


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


# ---------------------------------------------------------------------------
# Repo root + mapping.yml
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# Name sanitization
# ---------------------------------------------------------------------------

_NAME_INVALID_RE = re.compile(r"[^a-z0-9_-]+")
_NAME_COLLAPSE_RE = re.compile(r"-{2,}")


def sanitize_name(raw: str) -> str:
    """Coerce an arbitrary string into a safe registry entry name.

    Rules: lowercase, strip extension when the input looks like a
    filename, replace any character not in ``[a-z0-9_-]`` with ``-``,
    collapse repeated ``-``, strip leading/trailing ``-``. Raises
    ``IngestError`` if the result is empty.
    """
    if not raw:
        raise IngestError("cannot derive name from empty string")
    candidate = raw.strip().lower()
    if "." in candidate and "/" not in candidate:
        candidate = candidate.rsplit(".", 1)[0]
    candidate = _NAME_INVALID_RE.sub("-", candidate)
    candidate = _NAME_COLLAPSE_RE.sub("-", candidate).strip("-")
    if not candidate:
        raise IngestError(f"could not derive a valid name from {raw!r}")
    return candidate


# ---------------------------------------------------------------------------
# HTTP
# ---------------------------------------------------------------------------


def _build_request(url: str, accept: str | None = None) -> urllib.request.Request:
    req = urllib.request.Request(url)
    req.add_header("User-Agent", USER_AGENT)
    if accept:
        req.add_header("Accept", accept)
    token = os.environ.get("GITHUB_TOKEN")
    if token and url.startswith((GITHUB_API, "https://github.com", GITHUB_RAW)):
        req.add_header("Authorization", f"Bearer {token}")
    return req


def http_get_bytes(url: str, accept: str | None = None) -> bytes:
    """Fetch ``url`` and return the raw response body.

    Raises ``IngestError`` on any HTTP/network failure with a one-line
    message suitable for ``fail()``. Honors ``GITHUB_TOKEN`` env var for
    requests to github.com / api.github.com / raw.githubusercontent.com.
    """
    req = _build_request(url, accept=accept)
    try:
        with urllib.request.urlopen(req, timeout=HTTP_TIMEOUT_SECONDS) as resp:
            return resp.read()
    except urllib.error.HTTPError as exc:
        raise IngestError(f"GET {url} failed: HTTP {exc.code} {exc.reason}") from exc
    except urllib.error.URLError as exc:
        raise IngestError(f"GET {url} failed: {exc.reason}") from exc


def http_get_text(url: str, accept: str | None = None) -> str:
    return http_get_bytes(url, accept=accept).decode("utf-8")


def http_get_json(url: str) -> Any:
    body = http_get_bytes(url, accept="application/vnd.github+json")
    try:
        return json.loads(body.decode("utf-8"))
    except json.JSONDecodeError as exc:
        raise IngestError(f"GET {url} returned non-JSON body: {exc}") from exc


# ---------------------------------------------------------------------------
# GitHub URL parsing
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ParsedGithubUrl:
    """Structured view of a recognized GitHub URL.

    ``kind`` is one of:
      * ``"repo"`` — ``https://github.com/<owner>/<repo>`` (no ref/path)
      * ``"folder"`` — ``https://github.com/<owner>/<repo>/tree/<ref>/<path>``
      * ``"blob"`` — ``https://github.com/<owner>/<repo>/blob/<ref>/<path>``
      * ``"raw"`` — ``https://raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>``

    Limitation: refs containing ``/`` (e.g. ``feature/foo``) are not
    parsed correctly because the URL form is ambiguous without a tree
    listing. The first segment after ``tree``/``blob`` is taken as the
    ref; refs-with-slashes fall through into ``path`` and will fail
    later. Document this in the script's --help if it bites.
    """

    kind: str
    owner: str
    repo: str
    ref: str | None = None
    path: str = ""


def parse_github_url(url: str) -> ParsedGithubUrl:
    parsed = urlparse(url.strip())
    if parsed.scheme not in ("http", "https"):
        raise IngestError(f"unsupported URL scheme: {url}")
    host = parsed.netloc.lower()
    parts = [p for p in unquote(parsed.path).split("/") if p]

    if host == "raw.githubusercontent.com":
        if len(parts) < 4:
            raise IngestError(
                f"raw URL must be /<owner>/<repo>/<ref>/<path>: {url}"
            )
        owner, repo, ref, *rest = parts
        return ParsedGithubUrl("raw", owner, repo, ref, "/".join(rest))

    if host == "github.com":
        if len(parts) == 2:
            return ParsedGithubUrl("repo", parts[0], parts[1])
        if len(parts) >= 4 and parts[2] in ("tree", "blob"):
            owner, repo, kind_tok, ref, *rest = parts
            return ParsedGithubUrl(
                "folder" if kind_tok == "tree" else "blob",
                owner,
                repo,
                ref,
                "/".join(rest),
            )
    raise IngestError(f"unsupported GitHub URL: {url}")


# ---------------------------------------------------------------------------
# GitHub API + content fetch
# ---------------------------------------------------------------------------


def get_default_branch(owner: str, repo: str) -> str:
    info = http_get_json(f"{GITHUB_API}/repos/{owner}/{repo}")
    branch = info.get("default_branch")
    if not isinstance(branch, str) or not branch:
        raise IngestError(
            f"github.com/{owner}/{repo} did not report a default_branch"
        )
    return branch


def list_tree_recursive(owner: str, repo: str, ref: str) -> list[dict[str, Any]]:
    """Return every blob/tree entry reachable from ``ref`` in one call.

    Emits a stderr warning when GitHub truncates the response (very
    large repos). Callers that need exhaustive coverage in truncated
    cases should fall back to per-directory listing — for skill ingest,
    the warning is enough.
    """
    info = http_get_json(
        f"{GITHUB_API}/repos/{owner}/{repo}/git/trees/{ref}?recursive=1"
    )
    if info.get("truncated"):
        print(
            f"warning: tree for {owner}/{repo}@{ref} was truncated by GitHub; "
            "some entries may be missing",
            file=sys.stderr,
        )
    tree = info.get("tree", [])
    if not isinstance(tree, list):
        raise IngestError(f"unexpected /git/trees response for {owner}/{repo}@{ref}")
    return tree


def fetch_raw_file(owner: str, repo: str, ref: str, path: str) -> bytes:
    """Download a single file's bytes from raw.githubusercontent.com."""
    url = f"{GITHUB_RAW}/{owner}/{repo}/{ref}/{path}"
    return http_get_bytes(url)


# ---------------------------------------------------------------------------
# Prompts
# ---------------------------------------------------------------------------


def require_tty(action: str) -> None:
    """Raise IngestError if stdin/stdout aren't a TTY.

    Prompts hang silently in CI / slash-command / pipe contexts; we'd
    rather fail loudly than wait forever for stdin.
    """
    if not sys.stdin.isatty():
        raise IngestError(f"no TTY available; cannot prompt for {action}")


def prompt_choice(question: str, choices: list[str]) -> str:
    """Prompt for a single choice from ``choices`` and return the pick."""
    require_tty(question)
    while True:
        print(question, file=sys.stderr)
        for idx, choice in enumerate(choices, start=1):
            print(f"  [{idx}] {choice}", file=sys.stderr)
        raw = input("choice> ").strip()
        if raw.isdigit():
            idx = int(raw)
            if 1 <= idx <= len(choices):
                return choices[idx - 1]
        if raw in choices:
            return raw
        print(f"invalid choice: {raw!r}", file=sys.stderr)


def prompt_text(question: str, default: str | None = None) -> str:
    """Prompt for a free-text answer; returns ``default`` on empty input."""
    require_tty(question)
    suffix = f" [{default}]" if default else ""
    raw = input(f"{question}{suffix}: ").strip()
    if not raw:
        if default is None:
            raise IngestError(f"no value provided for {question!r}")
        return default
    return raw


# ---------------------------------------------------------------------------
# Manifest scaffolding
# ---------------------------------------------------------------------------


def write_manifest(dest_dir: Path, manifest: dict[str, Any]) -> Path:
    """Write a ``manifest.yml`` into ``dest_dir`` and return its path."""
    dest_dir.mkdir(parents=True, exist_ok=True)
    path = dest_dir / MANIFEST_FILE_NAME
    with path.open("w", encoding="utf-8") as fh:
        yaml.safe_dump(manifest, fh, sort_keys=False, default_flow_style=False)
    return path


_FRONTMATTER_RE = re.compile(
    r"\A---\s*\n(?P<body>.*?)\n---\s*(?:\n|\Z)", re.DOTALL
)


def parse_markdown_frontmatter(path: Path) -> dict[str, Any]:
    """Return the YAML frontmatter mapping from a markdown file, or ``{}``.

    SKILL.md and many agents.md ``snippet.md`` files lead with a YAML
    frontmatter block (``---\\n...\\n---``) that already carries ``name``
    and ``description``. Surfacing that lets the scaffolder skip prompts
    in headless contexts (slash commands, CI) where there is no TTY.

    Returns ``{}`` if the file is missing, has no frontmatter, or the
    frontmatter doesn't parse as a YAML mapping — callers should treat
    that as "no hint available" and fall back to their normal flow.
    """
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return {}
    match = _FRONTMATTER_RE.match(text)
    if not match:
        return {}
    try:
        data = yaml.safe_load(match.group("body"))
    except yaml.YAMLError:
        return {}
    return data if isinstance(data, dict) else {}


def scaffold_skill_manifest(
    dest_dir: Path,
    name: str,
    *,
    description: str | None = None,
    version: str | None = None,
) -> tuple[dict[str, Any], bool]:
    """Create a minimal ``manifest.yml`` for a skill if one is missing.

    Returns ``(manifest, scaffolded)`` — ``scaffolded`` is True iff a
    new ``manifest.yml`` was written to disk (so callers can include it
    in their confirmation summary). Prompts for description + version
    when not supplied.
    """
    target = dest_dir / MANIFEST_FILE_NAME
    if target.exists():
        with target.open("r", encoding="utf-8") as fh:
            existing = yaml.safe_load(fh) or {}
        if not isinstance(existing, dict):
            raise IngestError(f"{target} is not a YAML mapping")
        return existing, False
    # Many SKILL.md files carry name+description in YAML frontmatter —
    # prefer that over prompting so headless ingests (slash commands,
    # CI) work without a TTY.
    fm = parse_markdown_frontmatter(dest_dir / "SKILL.md")
    fm_description = fm.get("description") if isinstance(fm.get("description"), str) else None
    fm_version = fm.get("version") if isinstance(fm.get("version"), str) else None
    if description is None:
        if fm_description:
            description = fm_description
        elif sys.stdin.isatty():
            description = prompt_text(f"Description for skill '{name}'")
        else:
            raise IngestError(
                f"no description available for skill '{name}': "
                f"SKILL.md has no YAML frontmatter 'description' field "
                f"and no TTY is attached to prompt. Add a frontmatter "
                f"block to the source file or run the command "
                f"interactively."
            )
    if version is None:
        version = fm_version or "0.1.0"
    manifest = {
        "name": name,
        "description": description,
        "version": version,
        "harness_compatibility": [],
        "entrypoint": "SKILL.md",
    }
    write_manifest(dest_dir, manifest)
    return manifest, True


def scaffold_agents_md_manifest(
    dest_dir: Path,
    name: str,
    *,
    description: str | None = None,
    version: str | None = None,
    harness_compatibility: list[str] | None = None,
) -> tuple[dict[str, Any], bool]:
    """Create a minimal ``manifest.yml`` for an agents.md integration.

    Returns ``(manifest, scaffolded)`` (see ``scaffold_skill_manifest``).
    """
    target = dest_dir / MANIFEST_FILE_NAME
    if target.exists():
        with target.open("r", encoding="utf-8") as fh:
            existing = yaml.safe_load(fh) or {}
        if not isinstance(existing, dict):
            raise IngestError(f"{target} is not a YAML mapping")
        return existing, False
    # Many SKILL.md / snippet.md files carry name+description in YAML
    # frontmatter — prefer that over prompting so headless ingests
    # (slash commands, CI) work without a TTY.
    fm = parse_markdown_frontmatter(dest_dir / "snippet.md")
    fm_description = fm.get("description") if isinstance(fm.get("description"), str) else None
    fm_version = fm.get("version") if isinstance(fm.get("version"), str) else None
    if description is None:
        if fm_description:
            description = fm_description
        elif sys.stdin.isatty():
            description = prompt_text(
                f"Description for agents.md integration '{name}'"
            )
        else:
            raise IngestError(
                f"no description available for agents.md integration "
                f"'{name}': snippet.md has no YAML frontmatter "
                f"'description' field and no TTY is attached to prompt. "
                f"Add a frontmatter block to the source file or run the "
                f"command interactively."
            )
    if version is None:
        version = fm_version or "0.1.0"
    if harness_compatibility is None:
        if sys.stdin.isatty():
            raw = prompt_text(
                f"harness_compatibility for '{name}' "
                "(comma-separated subset of claude-code/codex/opencode; "
                "blank = all)",
                default="",
            )
            harness_compatibility = [t.strip() for t in raw.split(",") if t.strip()]
        else:
            # Empty list means "compatible with all harnesses" — safe default
            # when we can't ask.
            harness_compatibility = []
    for tag in harness_compatibility:
        if tag not in ALLOWED_HARNESSES:
            raise IngestError(
                f"invalid harness_compatibility entry: {tag!r}; "
                f"allowed: {', '.join(ALLOWED_HARNESSES)}"
            )
    manifest = {
        "name": name,
        "description": description,
        "version": version,
        "harness_compatibility": harness_compatibility,
        "snippet_file": "snippet.md",
    }
    write_manifest(dest_dir, manifest)
    return manifest, True


# ---------------------------------------------------------------------------
# Mapping.yml entry upsert
# ---------------------------------------------------------------------------


def upsert_catalog_entry(
    kind: str,
    name: str,
    version: str,
    source_url: str,
    *,
    root: Path | None = None,
) -> None:
    """Insert or replace a catalog entry in ``mapping.yml``."""
    if kind not in ("skill", "agents-md"):
        raise IngestError(f"unknown entry kind: {kind!r}")
    key = SKILLS_KEY if kind == "skill" else AGENTS_MD_KEY
    mapping = load_mapping(root)
    entries = mapping[key]
    remove_entry(entries, name)
    entries.append(
        {
            "name": name,
            "version": version,
            "source_url": source_url,
            "install_path": install_path_for(kind, name),
        }
    )
    save_mapping(mapping, root)


# ---------------------------------------------------------------------------
# Folder ingest
# ---------------------------------------------------------------------------


def _filter_under(tree: list[dict[str, Any]], prefix: str) -> list[dict[str, Any]]:
    if not prefix:
        return list(tree)
    prefix = prefix.rstrip("/") + "/"
    return [e for e in tree if e.get("path", "").startswith(prefix)]


def _relative_after(prefix: str, full_path: str) -> str:
    if not prefix:
        return full_path
    prefix = prefix.rstrip("/") + "/"
    return full_path[len(prefix):] if full_path.startswith(prefix) else full_path


def fetch_folder_to_disk(
    owner: str,
    repo: str,
    ref: str,
    src_path: str,
    dest_dir: Path,
    *,
    tree: list[dict[str, Any]] | None = None,
) -> list[str]:
    """Download every file under ``src_path`` into ``dest_dir``.

    Returns the list of relative paths (under ``dest_dir``) that were
    written. ``src_path`` may be empty to mean "the entire repo".
    Re-uses ``tree`` (from ``list_tree_recursive``) when supplied so
    callers don't pay for it twice.
    """
    if tree is None:
        tree = list_tree_recursive(owner, repo, ref)
    entries = _filter_under(tree, src_path) if src_path else list(tree)
    if not entries:
        raise IngestError(
            f"no files found at {owner}/{repo}@{ref}:{src_path or '/'}"
        )
    dest_dir.mkdir(parents=True, exist_ok=True)
    written: list[str] = []
    for entry in entries:
        if entry.get("type") != "blob":
            continue
        full = entry["path"]
        rel = _relative_after(src_path, full)
        if not rel:
            continue
        body = fetch_raw_file(owner, repo, ref, full)
        target = dest_dir / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(body)
        written.append(rel)
    if not written:
        raise IngestError(
            f"no files written from {owner}/{repo}@{ref}:{src_path or '/'} "
            "(only directory entries found)"
        )
    return written
