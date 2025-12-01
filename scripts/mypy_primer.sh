#!/usr/bin/env bash
set -eu

echo "Enabling mypy primer specific configuration overloads (see .github/mypy-primer-ty.toml)"
mkdir -p ~/.config/ty
cp .github/mypy-primer-ty.toml ~/.config/ty/ty.toml

PRIMER_SELECTOR="$(paste -s -d'|' "${PRIMER_SELECTOR}")"

echo "new commit"
git rev-list --format=%s --max-count=1 "${GITHUB_SHA}"

# https://github.com/astral-sh/ruff/pull/21722
git checkout -b base_commit 56d3173da5c4eaa597efb010d20130069985e7e2
echo "base commit"
git rev-list --format=%s --max-count=1 base_commit

cd ..

echo "Project selector: ${PRIMER_SELECTOR}"
# Allow the exit code to be 0 or 1, only fail for actual mypy_primer crashes/bugs
uvx \
  --from="git+https://github.com/hauntsaninja/mypy_primer@a81360123447a9409ab72f6b4f9684c02a9768e7" \
  mypy_primer \
  --repo ruff \
  --type-checker ty \
  --cargo-profile profiling \
  --old base_commit \
  --new "${GITHUB_SHA}" \
  --project-selector "/($PRIMER_SELECTOR)\$" \
  --output concise \
  --debug > "${DIFF_FILE}" || [ $? -eq 1 ]

# Output diff with ANSI color codes
cat "${DIFF_FILE}"

# Remove ANSI color codes before uploading
sed -ie 's/\x1b\[[0-9;]*m//g' "${DIFF_FILE}"
