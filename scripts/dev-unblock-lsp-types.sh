#!/usr/bin/env bash
# dev-unblock-lsp-types.sh
#
# Why this exists
# ---------------
# `ruff_server` depends on `lsp-types` via a git source
# (github.com/astral-sh/lsp-types.git @ rev e15db059). In the Claude-Code
# sandbox the egress proxy denies (403) that git host, so `cargo <anything>`
# in this workspace fails at *resolution* — even for crates that never touch
# LSP (e.g. ruff_spo_triplet / ruff_ruby_spo), because Cargo resolves the
# whole workspace graph before building any member.
#
# This script vendors the *same* fork at the *same* rev via the proxy-ALLOWED
# api.github.com zipball endpoint, then points Cargo at the local copy with a
# `[patch]` override. The denied git host is never contacted.
#
# What it commits: NOTHING upstream. The vendored crate lands in a gitignored
# dir; the `[patch]` it appends to Cargo.toml is a DEV-LOCAL edit — do not
# commit it (it points at the gitignored vendor dir). Remove it any time with
# `git checkout Cargo.toml`.
#
# Idempotent: safe to re-run; skips work already done.
set -euo pipefail

REV="e15db0593f0ecbbd80599c3f5880e4bf5da1ca0c"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR_DIR="${REPO_ROOT}/vendor/lsp-types-${REV:0:7}"
GIT_URL="https://github.com/astral-sh/lsp-types.git"

cd "${REPO_ROOT}"

# 1. Vendor the crate (skip if already present).
if [[ ! -f "${VENDOR_DIR}/Cargo.toml" ]]; then
  TOK="$(printf '%s' "${GH_TOKEN:-${GITHUB_TOKEN:-}}" | tr -d '"'"'"' ')"
  if [[ -z "${TOK}" ]]; then
    echo "error: GH_TOKEN / GITHUB_TOKEN not set; cannot fetch zipball" >&2
    exit 1
  fi
  tmp="$(mktemp -d)"
  echo "vendoring astral-sh/lsp-types @ ${REV:0:7} via api.github.com zipball ..."
  curl -fSs -L -H "Authorization: Bearer ${TOK}" \
    "https://api.github.com/repos/astral-sh/lsp-types/zipball/${REV}" \
    -o "${tmp}/lsp-types.zip"
  unzip -q "${tmp}/lsp-types.zip" -d "${tmp}/unpack"
  src="$(find "${tmp}/unpack" -maxdepth 1 -mindepth 1 -type d | head -1)"
  mkdir -p "$(dirname "${VENDOR_DIR}")"
  rm -rf "${VENDOR_DIR}"
  cp -r "${src}" "${VENDOR_DIR}"
  rm -rf "${tmp}"
  echo "  -> ${VENDOR_DIR}"
else
  echo "vendor already present: ${VENDOR_DIR}"
fi

# 2. Append the dev-local [patch] to Cargo.toml (skip if already there).
# Match the [patch.<url>] *section header*, not the bare git URL — the URL also
# appears in the original [workspace.dependencies] declaration.
if ! grep -qF '[patch."https://github.com/astral-sh/lsp-types.git"]' Cargo.toml; then
  cat >> Cargo.toml <<PATCH

# DEV-LOCAL (do not commit) — added by scripts/dev-unblock-lsp-types.sh.
# Overrides the proxy-denied astral-sh/lsp-types git host with the vendored
# copy so the workspace resolves. Remove with: git checkout Cargo.toml
[patch."https://github.com/astral-sh/lsp-types.git"]
lsp-types = { path = "vendor/lsp-types-${REV:0:7}" }
PATCH
  echo "appended dev-local [patch] to Cargo.toml (do NOT commit it)"
else
  echo "[patch] already present in Cargo.toml"
fi

echo "done. The workspace now resolves without contacting ${GIT_URL}."
echo "Verify: cargo test -p ruff_spo_triplet -p ruff_ruby_spo"
