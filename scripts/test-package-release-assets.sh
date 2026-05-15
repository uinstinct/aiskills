#!/usr/bin/env bash
# test-package-release-assets.sh — smoke test for package-release-assets.sh.
# Runs the packaging script against the repo and asserts the expected tarballs
# for the US-001 example entries are produced, byte-deterministic, and have the
# expected root directory name on extraction.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

OUT_DIR="$tmp/dist1" ./scripts/package-release-assets.sh >/dev/null

skill_asset="$tmp/dist1/skill-example-skill-0.1.0.tar.gz"
agents_md_asset="$tmp/dist1/agents-md-example-integration-0.1.0.tar.gz"

[ -f "$skill_asset" ] || { printf 'FAIL: missing %s\n' "$skill_asset" >&2; exit 1; }
[ -f "$agents_md_asset" ] || { printf 'FAIL: missing %s\n' "$agents_md_asset" >&2; exit 1; }

# Extraction yields a single top-level directory matching the entry name.
mkdir -p "$tmp/extract-skill" "$tmp/extract-agents-md"
tar -xzf "$skill_asset" -C "$tmp/extract-skill"
tar -xzf "$agents_md_asset" -C "$tmp/extract-agents-md"

[ -d "$tmp/extract-skill/example-skill" ] || { printf 'FAIL: skill tarball root is not example-skill/\n' >&2; exit 1; }
[ -f "$tmp/extract-skill/example-skill/SKILL.md" ] || { printf 'FAIL: extracted skill missing SKILL.md\n' >&2; exit 1; }
[ -f "$tmp/extract-skill/example-skill/manifest.yml" ] || { printf 'FAIL: extracted skill missing manifest.yml\n' >&2; exit 1; }

[ -d "$tmp/extract-agents-md/example-integration" ] || { printf 'FAIL: agents.md tarball root is not example-integration/\n' >&2; exit 1; }
[ -f "$tmp/extract-agents-md/example-integration/snippet.md" ] || { printf 'FAIL: extracted integration missing snippet.md\n' >&2; exit 1; }
[ -f "$tmp/extract-agents-md/example-integration/manifest.yml" ] || { printf 'FAIL: extracted integration missing manifest.yml\n' >&2; exit 1; }

# Determinism: a second run must produce byte-identical tarballs.
OUT_DIR="$tmp/dist2" ./scripts/package-release-assets.sh >/dev/null
for asset in skill-example-skill-0.1.0.tar.gz agents-md-example-integration-0.1.0.tar.gz; do
    a="$(sha256sum "$tmp/dist1/$asset" | awk '{print $1}')"
    b="$(sha256sum "$tmp/dist2/$asset" | awk '{print $1}')"
    if [ "$a" != "$b" ]; then
        printf 'FAIL: %s is not deterministic (%s vs %s)\n' "$asset" "$a" "$b" >&2
        exit 1
    fi
done

printf 'OK: package-release-assets.sh produces expected, deterministic tarballs.\n'
