#!/usr/bin/env bash
# Source this helper to point builds at the locally built nogil Python.
#
#   source scripts/nogil/nogil_env.sh /path/to/python-3.13-nogil

nogil_env() {
    if [[ "${BASH_SOURCE[0]-}" == "$0" ]]; then
        echo "nogil_env.sh must be sourced. Usage: source scripts/nogil/nogil_env.sh /path/to/python-3.13-nogil" >&2
        return 1
    fi

    PYTHON_VERSION="${PYTHON_VERSION:-3.13}"
    RAW_PREFIX="${1:-}"

    if [[ -z "${RAW_PREFIX}" ]]; then
        echo "Usage: source scripts/nogil/nogil_env.sh /path/to/python-3.13-nogil" >&2
        return 1
    fi

    if ! PREFIX="$(cd "${RAW_PREFIX}" 2>/dev/null && pwd)"; then
        echo "nogil_env.sh: cannot resolve '${RAW_PREFIX}'" >&2
        return 1
    fi

    PYTHON_BIN="${PREFIX}/bin/python${PYTHON_VERSION}"
    STDLIB_DIR="${PREFIX}/lib/python${PYTHON_VERSION}"
    DYNLOAD_DIR="${STDLIB_DIR}/lib-dynload"
    LIB_DIR="${PREFIX}/lib"

    export PATH="${PREFIX}/bin:${PATH}"
    export PYTHONPATH="${STDLIB_DIR}:${DYNLOAD_DIR}${PYTHONPATH:+:${PYTHONPATH}}"
    export DYLD_LIBRARY_PATH="${LIB_DIR}${DYLD_LIBRARY_PATH:+:${DYLD_LIBRARY_PATH}}"
    export LD_LIBRARY_PATH="${LIB_DIR}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"

    export PYO3_PYTHON="${PYTHON_BIN}"
    export PYTHONEXECUTABLE="${PYTHON_BIN}"
    export PYTHONHOME="${PREFIX}"
}

nogil_env "$@"
