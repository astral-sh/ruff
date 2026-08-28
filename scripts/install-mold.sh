#!/usr/bin/env bash
# Install mold linker and make it the default linker.

set -euo pipefail

MOLD_VERSION="${MOLD_VERSION:-2.41.0}"

arch="$(uname -m)"

# Release assets are mutable, so new versions require reviewed SHA-256 digests.
# https://github.com/rui314/mold/releases/tag/v2.41.0
case "${MOLD_VERSION}:${arch}" in
    2.41.0:aarch64)
        checksum="946de2774b06a71346bd59b55fddba610b65b8d93c3a4a1559cc84e103472710"
        ;;
    2.41.0:x86_64)
        checksum="a3696680d99e692970590a178bc3a33d78d60d1c6dc9db7a11b557b02b751f5d"
        ;;
    *)
        echo "No trusted mold checksum for version ${MOLD_VERSION} (${arch})" >&2
        exit 1
        ;;
esac

url="https://github.com/rui314/mold/releases/download/v${MOLD_VERSION}/mold-${MOLD_VERSION}-${arch}-linux.tar.gz"

echo "Installing mold ${MOLD_VERSION} (${arch})..."

archive="$(mktemp)"
trap 'rm -f "$archive"' EXIT

wget -O "$archive" \
    --timeout=10 \
    --tries=5 \
    --waitretry=3 \
    --retry-connrefused \
    --retry-on-http-error=429,500,502,503,504 \
    --progress=dot:mega \
    "$url"

printf '%s  %s\n' "$checksum" "$archive" | sha256sum -c -

if [ "$(whoami)" = root ]; then
    SUDO=""
else
    SUDO="sudo"
fi

$SUDO tar -C /usr/local --strip-components=1 --no-overwrite-dir -xzf "$archive"

# Make mold the default linker
current_ld="$(realpath /usr/bin/ld)"
if [ "$current_ld" != /usr/local/bin/mold ]; then
    $SUDO ln -sf /usr/local/bin/mold "$current_ld"
fi

echo "mold ${MOLD_VERSION} installed successfully"
mold --version
