---
name: minimizing-ty-ecosystem-changes
description: Use when a user says "minimize this ty ecosystem change", "reproduce this ecosystem result", "investigate a primer difference", "investigate a mypy_primer difference", "investigate a mypy-primer difference", or asks to reproduce, investigate, or minimize behavior changes in ty ecosystem/primer/mypy_primer/mypy-primer projects.
---

# Minimizing Ty Ecosystem Changes

## Invariants

1. Use the exact Ruff revisions, user-level PR config, dependency cutoff, mypy-primer revision, project Python version, and strictness settings from the Actions run.
2. Reproduce the reported project difference before explaining it or writing a smaller example.
3. Treat copied binaries and config as read-only, and verify every reduction against both binaries.
4. Derive every candidate from the preceding verified candidate; NEVER substitute an independently constructed example.
5. Preserve the underlying trigger, not merely the diagnostic rule, message, or displayed type.

Start each investigation from fresh artifacts. Do not trust retained memories, previous minimizations, current upstream project state, or the helper script's default lockfile.

## Collect Exact-Run Metadata

If a primary agent supplied an existing run-metadata manifest, verify that its run ID and attempt match the frozen report and that it contains each assigned project. Reuse the manifest without modifying it.

Otherwise, run the bundled helper with the Actions run ID or URL, matching attempt, and every affected mypy-primer project name:

```bash
scripts/collect_ty_ecosystem_run_metadata.py \
  <actions-run> <project-name>... \
  --attempt <actions-attempt> \
  --output target/ty-ecosystem-run.json
```

The manifest contains the analyzed Ruff revisions, Actions `EXCLUDE_NEWER`, ecosystem-analyzer and mypy-primer revisions, and each project's CI Python version. Stop if the helper cannot determine a unique value; never substitute a comment timestamp or local default.

The current workflow splits compilation into `Build ty (base)` and `Build ty (pr)`. The helper reads the base job, which records both the merge base and PR merge revision, and still supports historical runs with a single `Build ty` job.

## Prepare ty

If a primary agent supplied freshly copied base and PR profiling binaries plus the PR ecosystem config, preserve their absolute paths as `TY_ECOSYSTEM_BASE_BINARY` and `TY_ECOSYSTEM_PR_BINARY`, verify they exist, and reuse them. Do not rebuild those binaries, switch shared Ruff refs, or overwrite the shared artifacts. An agent may build an exact-revision debug binary on demand to identify an ambiguous internal type, using an isolated worktree if necessary; the profiling binaries remain the behavioral oracle.

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

Create a unique temporary directory for each project and use its absolute path. Read its Python version and the pinned mypy-primer revision from the manifest. Obtain the project revision from the `/blob/<commit>/` component of the original diagnostic's source permalink, and check that links for the same project agree. If no diagnostic permalink exists, inspect the matching diagnostics shard or Actions logs; if the exact revision cannot be recovered, explicitly report that limitation. Then bypass the adjacent script lockfile:

```bash
uv run \
  --python <project-python> \
  --with "mypy-primer @ git+https://github.com/hauntsaninja/mypy_primer@<mypy-primer-revision>" \
  --no-project \
  python scripts/setup_primer_project.py \
  <project-name> <absolute-temporary-directory> \
  --revision <report-project-revision> \
  --exclude-newer <EXCLUDE_NEWER>
```

Use the ecosystem config as user-level configuration, matching CI without replacing project-level config discovery, and re-export `XDG_CONFIG_HOME` in each new shell. If a primary agent supplied `TY_ECOSYSTEM_CONFIG_HOME`, reuse its installed config without modifying it; otherwise, install the copied config locally. Read the project's `strict` or `non-strict` label from the frozen detailed report, or its `strict_settings` value from the matching diagnostics shard. Preserve that mode when running either binary:

```bash
if [[ -n "${TY_ECOSYSTEM_CONFIG_HOME:-}" ]]; then
  export XDG_CONFIG_HOME="$TY_ECOSYSTEM_CONFIG_HOME"
  test -f "$XDG_CONFIG_HOME/ty/ty.toml" || exit 1
else
  export XDG_CONFIG_HOME="$PWD/target/ty-ecosystem-config"
  mkdir -p "$XDG_CONFIG_HOME/ty"
  cp "$PWD/target/ty-ecosystem-bins/ty-ecosystem.toml" "$XDG_CONFIG_HOME/ty/ty.toml"
fi
unset TY_CONFIG_FILE

project_dir="<absolute-temporary-directory>"
ty_base="${TY_ECOSYSTEM_BASE_BINARY:-$PWD/target/ty-ecosystem-bins/ty-base}"
ty_pr="${TY_ECOSYSTEM_PR_BINARY:-$PWD/target/ty-ecosystem-bins/ty-pr}"
test -x "$ty_base" && test -x "$ty_pr" || exit 1
ecosystem_analysis_mode="<strict-or-non-strict-from-detailed-report>"

if [[ "$ecosystem_analysis_mode" != strict && "$ecosystem_analysis_mode" != non-strict ]]; then
  echo "Unknown ecosystem analysis mode: $ecosystem_analysis_mode" >&2
  exit 1
fi

run_ecosystem_ty() {
  if [[ "$ecosystem_analysis_mode" == strict ]]; then
    <project-specific command printed by setup_primer_project.py> \
      --config analysis.strict-equality-semantics=true \
      --config analysis.strict-generic-narrowing=true
  else
    <project-specific command printed by setup_primer_project.py>
  fi
}

cd "$project_dir"
ty_binary="$ty_base"
base_exit_status=0
run_ecosystem_ty || base_exit_status=$?
ty_binary="$ty_pr"
pr_exit_status=0
run_ecosystem_ty || pr_exit_status=$?
```

Confirm the detailed report's difference exactly, including duplicate diagnostics and both exit statuses. Ordinary diagnostics can produce exit status 1; do not mistake that for a failed reproduction.

## Minimize

Reduce the reproduced project toward a self-contained single-file reproducer with minimal code and dependencies. A reduction is trivial only when the difference already occurs in one self-contained file and can be preserved solely by deleting obviously unrelated code. Multiple files, imports or dependencies, inlining, replacing language constructs, ambiguous types such as `@Todo`, or an uncertain cause make a reduction nontrivial. Before attempting any nontrivial reduction, read and follow [references/advanced-minimization.md](references/advanced-minimization.md). If in doubt, treat the reduction as nontrivial.

Matching diagnostics or displayed types do not establish a shared cause. When the output is ambiguous, identify the original and minimized triggers using exact-revision debug output, a targeted `reveal_type`, or the producing Rust call site.

Record the original source permalink, accepted reductions, both binaries' results, and any causal fingerprint. If source provenance or a matching cause cannot be established, return the original project excerpt explicitly marked as unminimized.

## Return

Provide the original permalinked report entry, exact base and PR behavior, minimal code, full diagnostic messages and error codes, and the manifest/commands needed to reproduce it. When called from the summary workflow, return import-audit and reduction notes separately from report-ready Markdown.
