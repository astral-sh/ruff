---
name: minimizing-ty-ecosystem-changes
description: Use when a user says "minimize this ty ecosystem change", "reproduce this ecosystem result", "investigate a primer difference", "investigate a mypy_primer difference", "investigate a mypy-primer difference", or asks to reproduce, investigate, or minimize behavior changes in ty ecosystem/primer/mypy_primer/mypy-primer projects.
---

# Minimizing Ty Ecosystem Changes

## Invariants

1. Use the exact Ruff revisions, user-level PR config, dependency cutoff, mypy-primer revision, project Python version, checker environment and deadline, effective target platform, and strictness settings from the Actions run. Match its execution platform when dependency installation or runtime behavior depends on it.
2. Reproduce the reported project difference before explaining it or writing a smaller example.
3. Treat copied binaries and config as read-only, and verify every reduction against both binaries.
4. Derive every candidate from the preceding verified candidate; NEVER substitute an independently constructed example.
5. Preserve the underlying trigger, not merely the diagnostic rule, message, or displayed type.

Start each investigation from fresh artifacts. Do not trust retained memories, previous minimizations, current upstream project state, or the helper script's default lockfile.

Prefix every direct or indirect `gh` invocation with `GH_TELEMETRY=false`; each Codex tool call may start a new shell.

## Collect Exact-Run Metadata

If the primary agent supplied an immutable `TY_ECOSYSTEM_RUN_METADATA` manifest, verify that its run ID and attempt match the frozen report and that it contains each assigned project and the reviewed runtime settings described below. All subagents reuse the same read-only manifest; ask the primary agent to supply missing information rather than modifying it or generating another shared manifest.

Otherwise, run the bundled helper once with the Actions run ID or URL, matching attempt, and every affected mypy-primer project name:

```bash
export TY_ECOSYSTEM_RUN_METADATA="$PWD/target/ty-ecosystem-run.json"
GH_TELEMETRY=false uv run --script scripts/collect_ty_ecosystem_run_metadata.py \
  <actions-run> <project-name>... \
  --attempt <actions-attempt> \
  --output "$TY_ECOSYSTEM_RUN_METADATA"
```

The helper collects the analyzed Ruff revisions, Actions `EXCLUDE_NEWER`, ecosystem-analyzer and mypy-primer revisions, each project's CI Python version, and the original ecosystem config as `ty_config`. With `--output`, it also saves analysis-job logs and structured job metadata in an adjacent `<output filename>.evidence` directory; `analysis_jobs` records their paths and source URLs. Stop if the helper cannot determine a unique core value; never substitute a comment timestamp or local default.

The current workflow splits compilation into `Build ty (base)` and `Build ty (pr)`. The helper reads the base job, which records both the merge base and PR merge revision, and still supports historical runs with a single `Build ty` job.

### Checker Environment and Platforms

The helper preserves runtime evidence but does not interpret it. Before publishing the manifest as immutable, the primary agent must inspect the saved analysis-job logs and runner metadata, consulting the selected workflow revision when needed. Determine the environment of the actual analysis command, including any shell overrides, rather than copying settings from an earlier setup step. Read the installed uv version, runner OS and architecture, and effective analyzer profile from the available evidence, checking the default when no profile is specified; a tool-cache hit need not have a download message. Compare the relevant shards and resolve any conflicting or missing information without guessing from the local machine.

Record the verified findings in a `runtime` object in the manifest: `runner_os`, `runner_arch`, `default_python_platform`, `uv_version`, `analyzer_profile`, and `checker_env`. Include `TY_UV`, `UV_LOCKED`, and `RUST_BACKTRACE` in `checker_env`, using strings for values and `null` only when the setting is determined to be unset. A missing or unfamiliar log line is not proof that a variable was unset. Add brief `evidence` notes identifying the job and log section or workflow setting supporting the findings, then freeze the completed manifest for all workers.

Use the reproduction runner below to restore `runtime.checker_env` for each binary, including unsetting variables whose recorded value is `null`, and use the recorded uv version. Ensure any `UV` executable override selects that version too. For runs that enable PEP 723 script environments with `TY_UV=scripts`, omitting that value disables script dependency discovery.

Set `UV_NO_BUILD=1` and `UV_NO_BINARY=0` for script preparation and checker runs, matching ecosystem-analyzer's current overrides. These prevent source-distribution builds while allowing binary distributions. Apply them in addition to the recorded Actions environment, as shown below.

The current analyzer uses a 30-second checker deadline with `--profile profiling` or `--profile release`, and 180 seconds otherwise; its default profile is `dev`. Script preparation has a 180-second deadline. The analyzer profile is independent of the binary's build profile, so prebuilt profiling binaries still receive the default 180-second deadline unless the invocation selects another profile. Preserve these deadlines during reproduction and minimization, and classify expiration separately from an ordinary exit code. Verify the settings when investigating a historical run whose analyzer behavior differs from these current defaults.

Install the copied ecosystem config as user-level configuration so project settings and original command-line arguments retain their precedence. Fill a missing `environment.python-platform` in that copy with the verified CI default before publishing it, as described below. For example, a Linux default must not override a project's explicit `win32` or `all` setting. When moving a minimized example outside its original project configuration, preserve that project's effective target platform as well.

Choose the execution environment before building binaries or installing dependencies. A ty target-platform option affects type analysis; it does not make uv resolve Linux dependency markers, install Linux wheels, or run Linux build steps on macOS. When platform-dependent dependencies, native extensions, installation commands, or PEP 723 script environments affect the result, run project setup and both compatible ty binaries in an environment matching the selected runner's OS and architecture, such as a container or remote host. Create its virtualenvs there instead of copying virtualenvs from another platform. Local native binaries are suitable when these differences do not affect the reproduction; still preserve the effective checker target and verify the reported difference exactly. Report an unavailable matching environment as a reproduction limitation rather than claiming equivalence.

## Prepare ty

If a primary agent supplied freshly copied base and PR profiling binaries plus the PR ecosystem config, preserve their absolute paths as `TY_ECOSYSTEM_BASE_BINARY` and `TY_ECOSYSTEM_PR_BINARY`, verify they exist, and reuse them. Do not rebuild those binaries, switch shared Ruff refs, or overwrite the shared artifacts. If an exact-revision debug binary is needed to identify an ambiguous internal type, request it from the primary agent; the profiling binaries remain the behavioral oracle.

Otherwise, require a clean working tree, remember its original ref, and build both exact revisions before assigning any subagent work. Reuse the checkout's existing Cargo target directory, copy the profiling binaries and PR ecosystem config to `target/ty-ecosystem-bins`, and restore the original ref when finished:

Fetch the PR revision explicitly because pull-request runs usually use a synthetic GitHub merge commit that a normal clone does not contain:

```bash
set -euo pipefail

test -z "$(git status --short)" || { git status --short; exit 1; }
original_ref="$(git symbolic-ref --quiet --short HEAD || git rev-parse HEAD)"
GH_TELEMETRY=false git fetch https://github.com/astral-sh/ruff.git <pr-revision>
mkdir -p target/ty-ecosystem-bins
trap 'git checkout "$original_ref"' EXIT

artifact_dir="$PWD/target/ty-ecosystem-bins"
build_target_dir="${CARGO_TARGET_DIR:-target}"
export CARGO_PROFILE_PROFILING_DEBUG=line-tables-only
jq -er '.ty_config' "$TY_ECOSYSTEM_RUN_METADATA" > "$artifact_dir/ty-ecosystem.toml"

git checkout --detach <merge-base>
cargo build --package ty --profile profiling
cp "$build_target_dir/profiling/ty" "$artifact_dir/ty-base"

git checkout --detach <pr-revision>
cargo build --package ty --profile profiling
cp "$build_target_dir/profiling/ty" "$artifact_dir/ty-pr"
```

After restoring the original ref, inspect vendored definitions and Rust implementations with `git -C <ruff-checkout> show <exact-revision>:<repository-relative-path>`, selecting the merge-base or PR revision from the immutable manifest. Never assume working-tree files match either analyzed binary or switch the shared checkout's ref.

Before using or publishing the copied `target/ty-ecosystem-bins/ty-ecosystem.toml`, inspect its `[environment]` settings. If `python-platform` is absent, add it with the verified `runtime.default_python_platform` value, creating the table if needed. Preserve an existing platform setting and all other options. Make this edit only to the reproduction copy; workers reuse the primary agent's prepared config without modifying it.

## Reproduce

Create a unique temporary directory for each project and use its absolute path. Read its Python version and the pinned mypy-primer revision from the shared manifest. Obtain the project revision from the `/blob/<commit>/` component of the original diagnostic's source permalink, and check that links for the same project agree. If no diagnostic permalink exists, inspect the matching diagnostics shard or Actions logs; if the exact revision cannot be recovered, explicitly report that limitation. Then bypass the adjacent script lockfile:

```bash
GH_TELEMETRY=false uv run \
  --python <project-python> \
  --with "mypy-primer @ git+https://github.com/hauntsaninja/mypy_primer@<mypy-primer-revision>" \
  --no-project \
  python scripts/setup_primer_project.py \
  <project-name> <absolute-temporary-directory> \
  --revision <report-project-revision> \
  --exclude-newer <EXCLUDE_NEWER>
```

Use the prepared ecosystem config as user-level configuration, matching CI without replacing project-level config discovery, and restore `XDG_CONFIG_HOME` in each new shell. If a primary agent supplied `TY_ECOSYSTEM_CONFIG_HOME`, reuse its installed config without modifying it; otherwise, install the prepared copy locally. Read the project's `strict` or `non-strict` label from the frozen detailed report, or its `strict_settings` value from the matching diagnostics shard. Preserve that mode and the original project command's platform options when running either binary; the user-level config supplies CI's platform only as a fallback.

Before timing either binary, reproduce the pinned analyzer's script-preparation phase when script integration is enabled. Follow that revision's `_install_script_dependencies` routine for script selection, Python environment, and warmup count; use `scripts/run_ty_ecosystem_repro.py --timeout-seconds 180` for its uv commands, with the environment overrides above. Keep dependency installation and warmup outside the checker deadline. For timeout changes, use comparable resources and concurrency to CI; a faster local machine or a longer deadline does not demonstrate that the timeout is fixed.

`scripts/run_ty_ecosystem_repro.py` restores the recorded checker environment, enforces the supplied deadline, and writes the command's outcome as JSON. Its own successful exit means the outcome was recorded, not that the checker succeeded. Read `timed_out` and `return_code`: timeout gives `true` and `null`, matching the analyzer's classification, and partial diagnostics from timed-out runs are discarded. Capture normal diagnostics from the JSON's `stdout` and `stderr` fields.

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
export UV_NO_BUILD=1
export UV_NO_BINARY=0

project_dir="<absolute-temporary-directory>"
repro_runner="$PWD/scripts/run_ty_ecosystem_repro.py"
repro_results="$(mktemp -d "${TMPDIR:-/tmp}/ty-repro-results.XXXXXX")"
ty_base="${TY_ECOSYSTEM_BASE_BINARY:-$PWD/target/ty-ecosystem-bins/ty-base}"
ty_pr="${TY_ECOSYSTEM_PR_BINARY:-$PWD/target/ty-ecosystem-bins/ty-pr}"
test -x "$ty_base" && test -x "$ty_pr" || exit 1
ecosystem_analysis_mode="<strict-or-non-strict-from-detailed-report>"
analyzer_profile="$(jq -er '.runtime.analyzer_profile' "$TY_ECOSYSTEM_RUN_METADATA")" || exit 1
case "$analyzer_profile" in
  profiling|release) checker_timeout=30 ;;
  *) checker_timeout=180 ;;
esac

if [[ "$ecosystem_analysis_mode" != strict && "$ecosystem_analysis_mode" != non-strict ]]; then
  echo "Unknown ecosystem analysis mode: $ecosystem_analysis_mode" >&2
  exit 1
fi

run_ecosystem_ty() {
  if [[ "$ecosystem_analysis_mode" == strict ]]; then
    uv run --script "$repro_runner" --metadata "$TY_ECOSYSTEM_RUN_METADATA" --timeout-seconds "$checker_timeout" --output "$repro_result" -- \
      <project-specific command printed by setup_primer_project.py> \
      --config analysis.strict-equality-semantics=true \
      --config analysis.strict-generic-narrowing=true
  else
    uv run --script "$repro_runner" --metadata "$TY_ECOSYSTEM_RUN_METADATA" --timeout-seconds "$checker_timeout" --output "$repro_result" -- \
      <project-specific command printed by setup_primer_project.py>
  fi
}

cd "$project_dir"
ty_binary="$ty_base"
repro_result="$repro_results/base.json"
run_ecosystem_ty || exit 1
base_exit_status="$(jq -r '.return_code' "$repro_result")"
ty_binary="$ty_pr"
repro_result="$repro_results/pr.json"
run_ecosystem_ty || exit 1
pr_exit_status="$(jq -r '.return_code' "$repro_result")"
```

Confirm the detailed report's difference exactly, including duplicate diagnostics and both exit outcomes. When reproducing an intermittent severe failure, repeat each side using its reported run count and deadline. Ordinary diagnostics can produce exit status 1; do not mistake that for a failed reproduction. For panics, identify the stable fingerprint by comparing the Rust panic site or decisive causal frame and panic payload; ignore checked Python-file paths and incidental backtrace differences.

## Minimize

The top priority is to make the reproducer as self-contained and isolated as feasible while preserving the original difference and underlying trigger. Ideally, a reader should be able to understand the behavior from the example and its explanation without consulting typeshed's stubs or third-party library definitions. Include the relevant definitions locally wherever feasible, then minimize them along with the original code. Prefer one file, with no avoidable imports or unnecessary definitions, annotations, branches, or advanced language features. Retain a third-party import only if identified ty behavior depends on that library's identity or third-party search-path classification.

Some strangeness or artificiality is expected after minimization. Keep enough meaningful structure and context that a reader can see how the pattern could arise in real code. Avoid examples that are extremely contrived or so generic that they leave no clue about that context, but continue reducing when this connection remains clear.

Removing avoidable imports and inlining relevant library or builtin definitions takes priority over both brevity and keeping the real-world pattern recognizable. If inlining preserves the original difference and underlying trigger, accept it even when it adds lines or makes the code less recognizable. This applies equally to builtins that require no import. Explain any lost real-world context in the accompanying prose.

Before minimizing any ecosystem change, read and follow [references/advanced-minimization.md](references/advanced-minimization.md). Exhaust its complete reduction loop, including third-party dependency and standard-library inlining, and retain an import only after verifying that neither removing it nor inlining its definitions preserves the underlying behavior.

Attempt to inline the relevant stub definitions for complex builtins such as `zip` and `map`, reducing those definitions to what the example needs, so readers do not need to consult typeshed to understand the trigger. Verify the inlined example against both exact-revision binaries and preserve the original cause. Retain the builtin when verified inlining cannot preserve the underlying trigger or base-to-PR difference, for example when the differing vendored stub definitions are themselves the cause. Such a retained builtin does not prevent completion of minimization; document the evidence in the final audit.

Matching diagnostics or displayed types do not establish a shared cause. When the output is ambiguous, identify and compare the original and minimized triggers using exact-revision debug output, a targeted `reveal_type`, or the producing Rust call site from the matching analyzed revision.

A minimization is complete only when a verified reduction chain connects the reproducer to the original ecosystem entry, the import audit passes, and an exhaustive pass finds no further reduction consistent with these priorities. If a genuine external blocker prevents completion, report the blocker and identify the minimization as incomplete; an original source excerpt is not a successfully minimized result.

## Return

Provide the original permalinked report entry, exact base and PR behavior, minimal code, full diagnostic messages and error codes or the panic fingerprint, and the manifest/commands needed to reproduce it. When called from the summary workflow, return import-audit and reduction notes separately from report-ready Markdown.
