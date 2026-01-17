#!/usr/bin/env bash
set -euo pipefail

# Build a free-threaded Python 3.13 with the GIL disabled and libpython shared.
# Usage:
#   ./scripts/build_python_3_13_nogil.sh [/path/to/prefix]
# Environment variables:
#   PYTHON_VERSION  Override the version (default: 3.13.0)
#   BUILD_JOBS      Override the job count for make (default: nproc)

PYTHON_VERSION="${PYTHON_VERSION:-3.13.0}"
PREFIX="${1:-${PYTHON_PREFIX:-"$HOME/python-${PYTHON_VERSION}-nogil"}}"

if command -v nproc >/dev/null 2>&1; then
    DEFAULT_JOBS="$(nproc)"
elif command -v sysctl >/dev/null 2>&1; then
    DEFAULT_JOBS="$(sysctl -n hw.ncpu)"
else
    DEFAULT_JOBS=1
fi

JOBS="${BUILD_JOBS:-${DEFAULT_JOBS}}"

TARBALL="Python-${PYTHON_VERSION}.tgz"
SOURCE_DIR="Python-${PYTHON_VERSION}"
DOWNLOAD_URL="https://www.python.org/ftp/python/${PYTHON_VERSION}/${TARBALL}"

echo "Downloading ${DOWNLOAD_URL}..."
curl -L "${DOWNLOAD_URL}" -o "${TARBALL}"

echo "Extracting ${TARBALL}..."
rm -rf "${SOURCE_DIR}"
tar -xzf "${TARBALL}"

pushd "${SOURCE_DIR}" >/dev/null

echo "Configuring Python ${PYTHON_VERSION} (prefix=${PREFIX})..."
./configure \
    --prefix="${PREFIX}" \
    --disable-gil \
    --enable-shared \
    --with-ensurepip=install

echo "Building with ${JOBS} job(s)..."
make -j"${JOBS}"

echo "Installing to ${PREFIX}..."
make install

popd >/dev/null

cat <<EOF

Python ${PYTHON_VERSION} (nogil) has been installed to:
  ${PREFIX}

Remember to export the following when building PyO3 projects:
  export PYO3_PYTHON="${PREFIX}/bin/python3.13"
  export PYTHONHOME="${PREFIX}"
  export PYTHONPATH="${PREFIX}/lib/python3.13"
EOF
