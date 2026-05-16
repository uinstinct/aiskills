#!/usr/bin/env bash
# package-release-assets.sh — produce deterministic release tarballs for every
# skill under assets/skills/ and every agents.md integration under assets/agents.md/.
#
# Usage:
#   scripts/package-release-assets.sh           # writes ./dist/*.tar.gz
#   OUT_DIR=/tmp/foo scripts/package-release-assets.sh
#
# Each tarball is rooted at the entry's name (extracting skill-foo-1.0.0.tar.gz
# yields a single foo/ directory) and is byte-deterministic across runs.

set -euo pipefail

OUT_DIR="${OUT_DIR:-dist}"
OUT_DIR_ABS=""
TAR_BIN="tar"

err() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "required command not found: $1"
}

# GNU tar is required for the deterministic flags (--sort, --owner, --group,
# --numeric-owner, --mtime). bsdtar (macOS default) does not accept --sort.
require_gnu_tar() {
    if tar --version 2>/dev/null | grep -q 'GNU tar'; then
        return 0
    fi
    if command -v gtar >/dev/null 2>&1 && gtar --version 2>/dev/null | grep -q 'GNU tar'; then
        TAR_BIN="gtar"
        return 0
    fi
    err "GNU tar is required (got: $(tar --version 2>/dev/null | head -n1 || echo unknown))"
}

read_manifest_field() {
    # Usage: read_manifest_field <manifest_path> <field_name>
    # Returns the field value with surrounding quotes (if any) stripped.
    local manifest="$1" field="$2" value
    value="$(sed -n "s/^${field}:[[:space:]]*\(.*\)[[:space:]]*$/\1/p" "$manifest" | head -n 1)"
    value="${value%\"}"
    value="${value#\"}"
    value="${value%\'}"
    value="${value#\'}"
    printf '%s' "$value"
}

package_entry() {
    # Usage: package_entry <kind> <parent_dir> <name>
    # kind ∈ {skill, agents-md}; parent_dir is assets/skills/ or assets/agents.md/; name is the folder basename.
    local kind="$1" parent_dir="$2" name="$3"
    local manifest="${parent_dir}/${name}/manifest.yml"
    local version asset out_path

    [ -f "$manifest" ] || err "missing manifest: $manifest"

    version="$(read_manifest_field "$manifest" version)"
    [ -n "$version" ] || err "manifest $manifest has no version field"

    asset="${kind}-${name}-${version}.tar.gz"
    out_path="${OUT_DIR_ABS}/${asset}"

    # cd into the parent so the archive root is just <name>/, not <parent>/<name>/.
    # Pipe through `gzip -n` so the gzip header carries no original filename or
    # mtime — required for byte-deterministic tarballs across runs.
    (
        cd "$parent_dir" || exit 1
        "$TAR_BIN" \
            --sort=name \
            --owner=0 \
            --group=0 \
            --numeric-owner \
            --mtime='1970-01-01' \
            --format=ustar \
            -cf - \
            "$name" \
            | gzip -n -9 >"$out_path"
    )

    printf '  wrote %s\n' "${OUT_DIR}/${asset}"
}

main() {
    require_cmd tar
    require_cmd sed
    require_cmd gzip
    require_gnu_tar

    [ -d assets/skills ] || err "assets/skills/ directory not found (run from repo root)"
    [ -d "assets/agents.md" ] || err "assets/agents.md/ directory not found (run from repo root)"

    mkdir -p "$OUT_DIR"
    OUT_DIR_ABS="$(cd "$OUT_DIR" && pwd)"

    local entry name
    printf 'Packaging skills:\n'
    for entry in assets/skills/*/; do
        [ -d "$entry" ] || continue
        name="$(basename "$entry")"
        package_entry "skill" "assets/skills" "$name"
    done

    printf 'Packaging agents.md integrations:\n'
    for entry in assets/agents.md/*/; do
        [ -d "$entry" ] || continue
        name="$(basename "$entry")"
        package_entry "agents-md" "assets/agents.md" "$name"
    done

    printf 'Done. Output: %s/\n' "$OUT_DIR"
}

main "$@"
