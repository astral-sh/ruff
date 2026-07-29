---
name: minimizing-ty-ecosystem-changes
description: Use when a user says "minimize this ty ecosystem change", "reproduce this ecosystem result", "investigate a primer difference", "investigate a mypy_primer difference", "investigate a mypy-primer difference", or asks to reproduce, investigate, or minimize behavior changes in ty ecosystem/primer/mypy_primer/mypy-primer projects.
---

# Minimizing Ty Ecosystem Changes

## Invariants

1. Use the exact Ruff revisions, PR config, dependency cutoff, mypy-primer revision, and project Python version from the Actions run.
2. Reproduce the reported project difference before explaining it or writing a smaller example.
3. Treat copied binaries and config as read-only, and verify every reduction against both binaries.

Start each investigation from fresh artifacts. Do not trust retained memories, previous minimizations, current upstream project state, or the helper script's default lockfile.

## Collect Exact-Run Metadata

Run the bundled helper with the Actions run ID or URL and every affected mypy-primer project name:

```bash
scripts/collect_ty_ecosystem_run_metadata.py \
  <actions-run> <project-name>... \
  --output target/ty-ecosystem-run.json
```

The manifest contains the analyzed Ruff revisions, Actions `EXCLUDE_NEWER`, ecosystem-analyzer and mypy-primer revisions, and each project's CI Python version. Stop if the helper cannot determine a unique value; never substitute a comment timestamp or local default.

The current workflow splits compilation into `Build ty (base)` and `Build ty (pr)`. The helper reads the base job, which records both the merge base and PR merge revision, and still supports historical runs with a single `Build ty` job.

## Prepare ty

If a primary agent supplied freshly copied base and PR binaries plus the PR ecosystem config, verify the paths exist and reuse them. Do not rebuild, switch Ruff refs, or overwrite the shared artifacts.

Otherwise, require a clean working tree, copy `.github/ty-ecosystem.toml` from the PR revision, and build ty on the manifest's merge base and PR revision:

Fetch the PR revision explicitly because pull-request runs usually use a synthetic GitHub merge commit that a normal clone does not contain:

```bash
set -euo pipefail

test -z "$(git status --short)" || { git status --short; exit 1; }
git fetch origin <pr-revision>
mkdir -p target/ty-ecosystem-bins
export CARGO_PROFILE_PROFILING_DEBUG=line-tables-only

git checkout <merge-base>
cargo build --package ty --profile profiling
cp target/profiling/ty target/ty-ecosystem-bins/ty-base

git checkout <pr-revision>
cp .github/ty-ecosystem.toml target/ty-ecosystem-bins/ty-ecosystem.toml
cargo build --package ty --profile profiling
cp target/profiling/ty target/ty-ecosystem-bins/ty-pr
```

## Reproduce

Create a unique temporary directory for each project. Read its Python version and the pinned mypy-primer revision from the manifest, then bypass the adjacent script lockfile:

```bash
uv run \
  --python <project-python> \
  --with "mypy-primer @ git+https://github.com/hauntsaninja/mypy_primer@<mypy-primer-revision>" \
  --no-project \
  python scripts/setup_primer_project.py \
  <project-name> <temporary-directory> \
  --revision <report-project-revision> \
  --exclude-newer <EXCLUDE_NEWER>
```

Use absolute paths and re-export `TY_CONFIG_FILE` in every new shell before running either binary:

```bash
export TY_CONFIG_FILE="$PWD/target/ty-ecosystem-bins/ty-ecosystem.toml"
test -f "$TY_CONFIG_FILE"
project_dir="$PWD/<temporary-directory>"
ty_base="$PWD/target/ty-ecosystem-bins/ty-base"
ty_pr="$PWD/target/ty-ecosystem-bins/ty-pr"

cd "$project_dir"
ty_binary="$ty_base"
<project-specific command printed by setup_primer_project.py>
ty_binary="$ty_pr"
<project-specific command printed by setup_primer_project.py>
```

Confirm the detailed report's difference exactly, including duplicate diagnostics when present.

## Minimize

Reduce the reproduced project iteratively, using the base-versus-PR output as the oracle after every change. Prefer a single file, no third-party imports, and the least complex code that preserves the difference. For nontrivial reductions, follow [references/advanced-minimization.md](references/advanced-minimization.md).

## Return

Provide the original permalinked report entry, exact base and PR behavior, minimal code, full diagnostic messages and error codes, and the manifest/commands needed to reproduce it. When called from the summary workflow, return import-audit and reduction notes separately from report-ready Markdown.
