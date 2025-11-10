#!/usr/bin/env bash
# Source this helper to point builds at the locally built nogil Python.
#
#   source scripts/nogil/nogil_env.sh /path/to/python-build-or-prefix

if [[ "${BASH_SOURCE[0]-}" == "$0" ]]; then
    echo "nogil_env.sh must be sourced. Usage: source scripts/nogil/nogil_env.sh /path/to/python-build-or-prefix" >&2
    exit 1
fi

PYTHON_VERSION="${PYTHON_VERSION:-3.13}"
RAW_ROOT="${1:-${PYTHON_BUILD:-${PYTHON_PREFIX:-}}}"

if [[ -z "${RAW_ROOT}" ]] || ! ROOT="$(cd "${RAW_ROOT}" 2>/dev/null && pwd)"; then
    echo "Usage: source scripts/nogil/nogil_env.sh /path/to/python-build-or-prefix" >&2
    return 1
fi

for PYTHON_BIN in \
    "${ROOT}/bin/python${PYTHON_VERSION}" \
    "${ROOT}/bin/python${PYTHON_VERSION}t" \
    "${ROOT}/python.exe" \
    "${ROOT}/python"
do
    [[ -x "${PYTHON_BIN}" ]] && break
done

if [[ ! -x "${PYTHON_BIN}" ]]; then
    echo "nogil_env.sh: '${ROOT}' is neither a CPython build tree nor an installed Python prefix" >&2
    return 1
fi

if [[ "${PYTHON_BIN}" == "${ROOT}"/bin/* ]]; then
    export PATH="${ROOT}/bin:${PATH}"
fi

LIB_DIR="${ROOT}/lib"
[[ -d "${LIB_DIR}" ]] || LIB_DIR="${ROOT}"

unset PYTHONHOME PYTHONPATH
export DYLD_LIBRARY_PATH="${LIB_DIR}${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
export LD_LIBRARY_PATH="${LIB_DIR}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
export LIBRARY_PATH="${LIB_DIR}${LIBRARY_PATH:+:${LIBRARY_PATH}}"
export PYO3_PYTHON="${PYTHON_BIN}"
export PYTHONEXECUTABLE="${PYTHON_BIN}"
