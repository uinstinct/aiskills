#!/usr/bin/env bash
# instinctagents installer — downloads the latest Linux x86_64 release binary
# into the current directory as ./instinctagents.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/uinstinct/aiskills/main/install.sh | bash

set -euo pipefail

REPO="${INSTINCTAGENTS_REPO:-uinstinct/aiskills}"
ASSET="instinctagents-linux-x86_64"
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

    if [ "$os" != "Linux" ]; then
        err "unsupported OS: $os (instinctagents currently ships only a Linux x86_64 binary)"
    fi

    case "$arch" in
        x86_64|amd64)
            ;;
        *)
            err "unsupported architecture: $arch (instinctagents currently ships only a Linux x86_64 binary)"
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
    local tag="$1"
    local url="https://github.com/${REPO}/releases/download/${tag}/${ASSET}"
    printf 'Downloading %s from %s\n' "$ASSET" "$url"
    curl -fL --progress-bar -o "$DEST" "$url" \
        || err "failed to download $url"
}

main() {
    require_cmd curl
    require_cmd uname
    check_platform

    local tag
    tag="$(latest_tag)"
    printf 'Latest release: %s\n' "$tag"

    download_asset "$tag"
    chmod +x "$DEST"

    printf 'Installed %s (%s).\n' "$DEST" "$tag"
    printf 'Run ./instinctagents to start.\n'
}

main "$@"
