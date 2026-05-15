#!/usr/bin/env bash
# test-install.sh — tests for install.sh platform detection.
# Stubs uname and curl so no network calls are made.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

PASS=0
FAIL=0

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

stubs="$tmp/stubs"
mkdir -p "$stubs"

# Stub: uname — reads FAKE_OS (-s) and FAKE_ARCH (-m) from environment
cat > "$stubs/uname" << 'STUB'
#!/bin/sh
case "$1" in
    -s) printf '%s\n' "${FAKE_OS:-Linux}" ;;
    -m) printf '%s\n' "${FAKE_ARCH:-x86_64}" ;;
esac
STUB
chmod +x "$stubs/uname"

# Stub: curl — returns fake JSON for API calls; writes dummy binary for downloads.
# Records the download URL to ${CURL_LOG} for assertion.
cat > "$stubs/curl" << 'STUB'
#!/bin/sh
dest=""
url=""
skip_next=0

for arg in "$@"; do
    if [ "$skip_next" = "1" ]; then
        dest="$arg"
        skip_next=0
        continue
    fi
    case "$arg" in
        -o)  skip_next=1 ;;
        http*) url="$arg" ;;
    esac
done

if [ -n "$dest" ]; then
    printf '%s\n' "$url" >> "${CURL_LOG}"
    printf 'dummy-binary-content\n' > "$dest"
else
    printf '{"tag_name":"v0.0.1","assets":[]}\n'
fi
STUB
chmod +x "$stubs/curl"

run_test() {
    local name="$1" os="$2" arch="$3" expect_pass="$4" expected_asset="${5:-}"
    local curl_log stderr_file test_dir exit_code
    curl_log="$tmp/curl_${name}.log"
    stderr_file="$tmp/stderr_${name}.log"
    test_dir="$tmp/run_${name}"
    exit_code=0
    mkdir -p "$test_dir"

    (
        cd "$test_dir"
        FAKE_OS="$os" FAKE_ARCH="$arch" CURL_LOG="$curl_log" \
            PATH="$stubs:$PATH" INSTINCTAGENTS_REPO="test/repo" \
            bash "$REPO_ROOT/install.sh"
    ) > /dev/null 2> "$stderr_file" || exit_code=$?

    if [ "$expect_pass" = "true" ]; then
        if [ "$exit_code" != "0" ]; then
            printf 'FAIL [%s]: expected exit 0, got %d (stderr: %s)\n' \
                "$name" "$exit_code" "$(cat "$stderr_file")" >&2
            FAIL=$((FAIL + 1))
            return
        fi
        if [ -f "$curl_log" ] && grep -qE "/${expected_asset}$" "$curl_log"; then
            printf 'OK   [%s]\n' "$name"
            PASS=$((PASS + 1))
        else
            printf 'FAIL [%s]: download URL did not end in /%s (log: %s)\n' \
                "$name" "$expected_asset" "$(cat "$curl_log" 2>/dev/null || echo '<missing>')" >&2
            FAIL=$((FAIL + 1))
        fi
    else
        if [ "$exit_code" = "0" ]; then
            printf 'FAIL [%s]: expected non-zero exit, got 0\n' "$name" >&2
            FAIL=$((FAIL + 1))
            return
        fi
        local stderr_content
        stderr_content="$(cat "$stderr_file")"
        if printf '%s' "$stderr_content" | grep -q "error:" \
            && printf '%s' "$stderr_content" | grep -q "$os" \
            && printf '%s' "$stderr_content" | grep -q "$arch"; then
            printf 'OK   [%s]\n' "$name"
            PASS=$((PASS + 1))
        else
            printf 'FAIL [%s]: stderr missing "error:", OS, or arch (got: %s)\n' \
                "$name" "$stderr_content" >&2
            FAIL=$((FAIL + 1))
        fi
    fi
}

# Success cases
run_test "linux-x86_64"  Linux  x86_64  true  "instinctagents-linux-x86_64"
run_test "darwin-arm64"  Darwin arm64   true  "instinctagents-macos-arm64"
run_test "darwin-x86_64" Darwin x86_64  true  "instinctagents-macos-x86_64"

# Failure cases (unsupported platform)
run_test "linux-aarch64" Linux  aarch64 false
run_test "darwin-i386"   Darwin i386    false

if [ "$FAIL" -gt 0 ]; then
    printf 'FAILED: %d passed, %d failed\n' "$PASS" "$FAIL" >&2
    exit 1
fi

printf 'OK: all %d tests passed.\n' "$PASS"
