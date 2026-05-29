---
name: minimizing-ty-ecosystem-changes
description: Use when a user says "minimize this ty ecosystem change", "reproduce this ecosystem result", "investigate a primer difference", "investigate a mypy_primer difference", "investigate a mypy-primer difference", or asks to reproduce, investigate, or minimize behavior changes in ty ecosystem/primer/mypy_primer/mypy-primer projects.
---

# Minimizing Ty Ecosystem Changes

Use this skill when asked to reproduce a ty ecosystem change, investigate a behavior difference in a primer project, or minimize a reproducer from a ty ecosystem project.

## Building ty

If a primary agent supplies paths to a copied merge-base ty binary, copied PR ty binary, and copied PR ecosystem config that it freshly prepared at the start of the current task, reuse those artifacts as read-only inputs. Do not rebuild ty, switch refs in the Ruff checkout, or overwrite the supplied artifacts. Use a separate transient ecosystem-project directory from any concurrent subagents. If a supplied artifact is missing, stop and report that to the primary agent instead of mutating the Ruff checkout.

Otherwise, prepare the artifacts as follows.

Build ty on both the PR branch and the PR's merge base by running `CARGO_PROFILE_PROFILING_DEBUG=line-tables-only cargo build --package ty --profile profiling` from the repository root on both refs, matching ecosystem CI. Copy each built executable to a stable path before checking out the other ref; otherwise, the second build overwrites `target/profiling/ty`.

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
export CARGO_PROFILE_PROFILING_DEBUG=line-tables-only

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

From the repository root, export the ecosystem config that was copied from the PR branch, then use the following script to clone the ecosystem project, check out the relevant revision, and install its dependencies into `.venv`. Keep `TY_CONFIG_FILE` set when running ty comparisons; this applies the same rule configuration as ecosystem CI without overwriting the user's ty config. Shell exports may not persist between agent tool calls, so re-export `TY_CONFIG_FILE` in every new shell before invoking either ty binary. The examples below use the default artifact paths; substitute supplied artifact paths when a primary agent provides them.

When preparing artifacts yourself, if `target/ty-ecosystem-bins/ty-ecosystem.toml` does not exist yet, check out the PR branch and copy `.github/ty-ecosystem.toml` from there before continuing. Do not copy `.github/ty-ecosystem.toml` while on the merge base, because ecosystem CI compares both binaries with the PR branch's config.

```bash
set -euo pipefail

export TY_CONFIG_FILE="$PWD/target/ty-ecosystem-bins/ty-ecosystem.toml"
test -f "$TY_CONFIG_FILE" || { echo "missing $TY_CONFIG_FILE" >&2; exit 1; }
uv run --no-project scripts/setup_primer_project.py <project-name> <some-temp-dir> --revision <report-revision> --exclude-newer <report-timestamp>
```

ALWAYS confirm the behavior difference reproduces before minimizing or explaining it.
ALWAYS use this script before attempting to reproduce the change.
NEVER try to confirm the behaviour difference without first setting up the project's virtual environment using this script.

When comparing the copied ty binaries, use the project-specific ty command printed by `setup_primer_project.py`. That command includes the project's mypy-primer metadata such as `paths` and any custom `ty_cmd`, matching ecosystem CI. Keep `TY_CONFIG_FILE` exported in the same shell that runs the comparison, and set `ty_binary` to each copied binary before running the printed command:

```bash
export TY_CONFIG_FILE="$PWD/target/ty-ecosystem-bins/ty-ecosystem.toml"
test -f "$TY_CONFIG_FILE" || { echo "missing $TY_CONFIG_FILE" >&2; exit 1; }

project_dir="$PWD/<some-temp-dir>"
ty_base="$PWD/target/ty-ecosystem-bins/ty-base"
ty_pr="$PWD/target/ty-ecosystem-bins/ty-pr"

cd "$project_dir"
ty_binary="$ty_base"
<project-specific ty command printed by setup_primer_project.py>
ty_binary="$ty_pr"
<project-specific ty command printed by setup_primer_project.py>
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

Use a systematic loop. You do not need to apply every tool in every iteration, but keep looping until none of the available minimization tools can reduce the reproducer further while preserving the behavior difference. Your clones of ecosystem projects and/or their dependencies should be treated as transient artifacts of the investigation: they should be deleted after the minimization is complete, and you SHOULD NOT ask for permission to modify them during the minimization process. If the user has asked for an ecosystem change to be minimized, ANY of the changes below should be understood as automatically having been approved by the user.

Available minimization tools include:

1. Remove files that are not needed for the difference.
2. Strip imports, definitions, and statements from the remaining files.
3. Cut and paste definitions from one file into another file to reduce first-party imports.
4. Copy installed third-party dependency files from the project's `.venv` into the project as first-party files so that you can convert third-party imports into first-party imports. If you must clone a dependency instead, use the exact installed version rather than the dependency's current default branch.
5. Inline the relevant parts of stdlib definitions from ty's vendored typeshed stubs into first-party code to reduce stdlib imports. ty's vendored typeshed stubs are the single source of truth for ty regarding the Python standard library, and they are found in `./crates/ty_vendored`.

After applying a minimization tool, re-run the comparison between the PR branch and the PR's merge base.
Keep a reduction only when the behavior difference still reproduces.

DO NOT stop looping until no further reductions could be applied without making the behavior difference disappear.

Before retaining the final reproducer, perform an import audit: attempt to delete every import, inline third-party definitions where possible, and explain why each surviving third-party import is necessary.

If the minimized behavior change is a diagnostic change, the final minimized Python snippet MUST include the full diagnostic message and error code on both branches, as a comment above or on the relevant line of code.

ANY reference to a line number in an external project MUST use a permalink in the form `[project file.py:123](permalink)`. Referring to an external line number without a permalink is unacceptable.
