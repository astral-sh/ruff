---
name: minimizing-ty-ecosystem-changes
description: Use when a user says "minimize this ty ecosystem change", "reproduce this ecosystem result", "investigate a primer difference", or asks to reproduce, investigate, or minimize behavior changes in ty ecosystem or primer projects.
---

# Minimizing Ty Ecosystem Changes

Use this skill when asked to reproduce a ty ecosystem change, investigate a behavior difference in a primer project, or minimize a reproducer from a ty ecosystem project.

## Building ty

Build ty on both the PR branch and the PR's merge base by running `cargo build --package ty --profile profiling` from the repository root on both refs, matching ecosystem CI. Copy each built executable to a stable path before checking out the other ref; otherwise, the second build overwrites `target/profiling/ty`.

From the PR branch, also copy `.github/ty-ecosystem.toml` to a stable path before checking out another ref. Use that copied file for every comparison, matching ecosystem CI's behavior of reusing the PR config.

Before checking out another ref, inspect the working tree. If `git status --short` shows any staged, unstaged, or untracked changes, abort immediately and ask the user how to proceed.

For example:

```bash
set -euo pipefail

if [[ -n "$(git status --short)" ]]; then
    git status --short
    echo "working tree is not clean; stop and ask the user how to proceed" >&2
    exit 1
fi

mkdir -p target/ty-ecosystem-bins
cp .github/ty-ecosystem.toml target/ty-ecosystem-bins/ty-ecosystem.toml
export TY_CONFIG_FILE="$PWD/target/ty-ecosystem-bins/ty-ecosystem.toml"

git checkout <merge-base>
cargo build --package ty --profile profiling
cp target/profiling/ty target/ty-ecosystem-bins/ty-base

git checkout <pr-branch-or-sha>
cargo build --package ty --profile profiling
cp target/profiling/ty target/ty-ecosystem-bins/ty-pr
```

On Windows, adapt the shell commands and executable paths as necessary.

You should only need to build ty twice at the start of the investigation. Reuse the copied executables when determining if the behavior difference still reproduces.

## Reproduce First

From the repository root, export the ecosystem config that was copied from the PR branch, then use the following script to clone the ecosystem project, check out the relevant revision, and install its dependencies into `.venv`. Keep `TY_CONFIG_FILE` set when running ty comparisons; this applies the same rule configuration as ecosystem CI without overwriting the user's ty config. Shell exports may not persist between agent tool calls, so re-export `TY_CONFIG_FILE` in every new shell before invoking either ty binary.

If `target/ty-ecosystem-bins/ty-ecosystem.toml` does not exist yet, check out the PR branch and copy `.github/ty-ecosystem.toml` from there before continuing. Do not copy `.github/ty-ecosystem.toml` while on the merge base, because ecosystem CI compares both binaries with the PR branch's config.

```bash
set -euo pipefail

export TY_CONFIG_FILE="$PWD/target/ty-ecosystem-bins/ty-ecosystem.toml"
test -f "$TY_CONFIG_FILE" || { echo "missing $TY_CONFIG_FILE" >&2; exit 1; }
uv run --no-project scripts/setup_primer_project.py <project-name> <some-temp-dir> --revision <report-revision> --exclude-newer <report-timestamp>
```

ALWAYS confirm the behavior difference reproduces before minimizing or explaining it.
ALWAYS use this script before attempting to reproduce the change.
NEVER try to confirm the behaviour difference without first setting up the project's virtual environment using this script.

When comparing the copied ty binaries, pass the cloned project's virtual environment explicitly with `--python`:

```bash
project_dir="$PWD/<some-temp-dir>"
ty_base="$PWD/target/ty-ecosystem-bins/ty-base"
ty_pr="$PWD/target/ty-ecosystem-bins/ty-pr"

cd "$project_dir"
"$ty_base" check --python .venv --output-format concise
"$ty_pr" check --python .venv --output-format concise
```

If the ecosystem report gives an exact project revision, pass it to the script with `--revision`. If the report gives a timestamp, pass it to the script with `--exclude-newer`. Do not install dependencies before checking out the report revision. Do not use current upstream or the current mypy-primer revision as evidence for a historical report when the report provides a pinned revision. If running `ecosystem-analyzer` directly, also pass the report timestamp as `--exclude-newer`, matching ecosystem CI.

## Minimize

When asked to minimize an ecosystem change, start from the reproduced project. Reduce the Python code until only the smallest code needed to demonstrate the behavior difference remains. DO NOT look at conversation or analysis in other PR comments: your analysis should start from a clean slate. Even if the cause looks obvious, DO NOT skip ahead to an explanation or a hand-written reproducer. Use a rigorous, systematic process and verify after each reduction that the behavior difference still reproduces.

An ideal minimized reproducer:

- Is a single file that is as small as possible.
- Has as few third-party imports as possible, ideally none.
- Uses smaller third-party packages with fewer dependencies when third-party imports are unavoidable. For example, prefer an import from `numpy` over one from `pandas`.
- Has as few first-party and standard-library imports as possible.
- Keeps imports from modules with highly special-cased symbols only when they are necessary (such modules may include `typing`, `abc`, `enum`, `types`, or `typing_extensions`).
- Uses an absolute minimum of "advanced"/complex typing or language features. For example, if the original code
  uses a walrus operator, but the behavior difference still reproduces without it, remove the walrus operator. If the original code uses a `Protocol` from `typing_extensions`, but the behavior difference still reproduces without it, remove the `Protocol`.

Use a systematic loop. You do not need to apply every tool in every iteration, but keep looping until none of the available minimization tools can reduce the reproducer further while preserving the behavior difference.

Available minimization tools include:

1. Remove files that are not needed for the difference.
2. Strip imports, definitions, and statements from the remaining files.
3. Cut and paste definitions from one file into another file to reduce first-party imports.
4. Copy installed third-party dependency files from the project's `.venv` into the project as first-party files so that you can convert third-party imports into first-party imports. If you must clone a dependency instead, use the exact installed version rather than the dependency's current default branch.
5. Inline the relevant parts of stdlib definitions from ty's vendored typeshed stubs into first-party code to reduce stdlib imports. ty's vendored typeshed stubs are the single source of truth for ty regarding the Python standard library, and they are found in `./crates/ty_vendored`.

After applying a minimization tool, re-run the comparison between the PR branch and the PR's merge base.
Keep a reduction only when the behavior difference still reproduces.

DO NOT stop looping until no further reductions could be applied without making the behavior difference disappear.

If the minimized behavior change is a diagnostic change, the final minimized Python snippet MUST include the full diagnostic message and error code on both branches, as a comment above or on the relevant line of code.
