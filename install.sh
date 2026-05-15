#!/usr/bin/env bash
# instinctagents installer — downloads the latest release binary for the
# detected OS and architecture into the current directory as ./instinctagents.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/uinstinct/aiskills/main/install.sh | bash

set -euo pipefail

REPO="${INSTINCTAGENTS_REPO:-uinstinct/aiskills}"
DEST="./instinctagents"

err() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || err "required command not found: $1"
}

check_platform() {
    local os arch
    os="$(uname -s 2>/dev/null || echo unknown)"
    arch="$(uname -m 2>/dev/null || echo unknown)"

    case "${os}/${arch}" in
        Linux/x86_64|Linux/amd64)
            printf '%s' "instinctagents-linux-x86_64"
            ;;
        Darwin/arm64|Darwin/aarch64)
            printf '%s' "instinctagents-macos-arm64"
            ;;
        Darwin/x86_64|Darwin/amd64)
            printf '%s' "instinctagents-macos-x86_64"
            ;;
        *)
            err "unsupported platform: os=${os} arch=${arch}"
            ;;
    esac
}

latest_tag() {
    local api_url body tag
    api_url="https://api.github.com/repos/${REPO}/releases/latest"
    body="$(curl -fsSL -H 'Accept: application/vnd.github+json' "$api_url")" \
        || err "failed to query GitHub Releases API at $api_url"
    tag="$(printf '%s' "$body" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    [ -n "$tag" ] || err "could not parse tag_name from GitHub Releases API response"
    printf '%s' "$tag"
}

download_asset() {
    local tag="$1" asset="$2"
    local url="https://github.com/${REPO}/releases/download/${tag}/${asset}"
    printf 'Downloading %s from %s\n' "$asset" "$url"
    curl -fL --progress-bar -o "$DEST" "$url" \
        || err "failed to download $url"
}

main() {
    require_cmd curl
    require_cmd uname

    local asset tag
    asset="$(check_platform)"

    tag="$(latest_tag)"
    printf 'Latest release: %s\n' "$tag"

    download_asset "$tag" "$asset"
    chmod +x "$DEST"

    printf 'Installed %s (%s).\n' "$DEST" "$tag"
    printf 'Run ./instinctagents to start.\n'
}

main "$@"
