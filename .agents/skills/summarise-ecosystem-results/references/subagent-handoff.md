# Subagent Handoff

Use this reference whenever the summary skill delegates reproduction or minimization.

## Primary-Agent Responsibilities

Freeze the exact report, Actions run, and attempt. Run `scripts/collect_ty_ecosystem_run_metadata.py` once for all projects with retained source-attributable diagnostics or new, fixed, or meaningfully changed reproducible failure outcomes, then build and copy both exact-revision profiling binaries before assigning any subagent work.

Publish the immutable `TY_ECOSYSTEM_RUN_METADATA`, `TY_ECOSYSTEM_BASE_BINARY`, and `TY_ECOSYSTEM_PR_BINARY` absolute paths. Install the copied PR ecosystem config once at `$TY_ECOSYSTEM_CONFIG_HOME/ty/ty.toml` and publish the absolute `TY_ECOSYSTEM_CONFIG_HOME` path. Treat the snapshot, optional structured JSON, binaries, metadata, copied config, and installed configuration as read-only shared inputs.

If a subagent requests an exact-revision debug binary, only the primary agent may build it. Pause every active worker and wait for acknowledgment, verify the shared checkout is clean, remember its original ref, then build and copy the requested binary. Always restore the original ref before resuming workers, even if the build fails; publish the binary's immutable path only after a successful build and restoration. The profiling binaries remain the behavioral oracle.

Subagents must not regenerate shared manifests, rebuild shared profiling binaries, switch shared Ruff refs, rewrite shared configuration, or overwrite another agent's working files.

## Assignment Checklist

Give each subagent:

- The PR and detailed report links, plus the ecosystem comment link when available.
- The paths to the frozen HTML report, optional matching structured JSON, and available diagnostics shards, plus the frozen comment path when available and the selected Actions run and attempt; use these captured inputs instead of refetching live evidence.
- The exact assigned entries from the structured JSON when available, or from the frozen HTML report otherwise; distinguish source-attributable changes from outcomes without recoverable source, and provide the reported merge-base and PR run counts for intermittent severe failures.
- The immutable `TY_ECOSYSTEM_RUN_METADATA`, `TY_ECOSYSTEM_BASE_BINARY`, `TY_ECOSYSTEM_PR_BINARY`, and `TY_ECOSYSTEM_CONFIG_HOME` absolute paths.
- For assignments requiring reproduction, the instruction to use the `minimizing-ty-ecosystem-changes` skill with the shared manifest, copied profiling binaries, installed configuration, and a unique temporary directory; never generate another manifest.
- For source-attributable assignments, the instruction to produce a fully minimized, provenance-preserving reproducer by exhausting the complete advanced-minimization workflow, including third-party dependency inlining, standard-library inlining, and an audit of every remaining import.
- The instruction to inspect vendored definitions and Rust implementations with `git -C <ruff-checkout> show <exact-revision>:<repository-relative-path>`, using the analyzed revisions from the immutable manifest rather than the restored working tree.
- The instruction that an independently invented analogue does not satisfy a source-attributable assignment, and that a partially minimized example or an original source excerpt never satisfies any minimization assignment. If a genuine external blocker prevents minimization, return the blocker and mark the assignment as incomplete.
- For outcomes without recoverable source evidence, the instruction to verify and report the captured outcomes, stderr, panic evidence, and run frequencies without requiring a source reproducer or minimized code.
- The instruction to request any exact-revision debug binary from the primary agent instead of building one or switching shared Ruff refs.
- The instruction to stop all checkout-dependent work and subprocesses when the primary agent requests a pause, acknowledge only after they have stopped, and remain paused until explicitly resumed, even if the debug build fails.
- The instruction not to rebuild profiling binaries, regenerate published metadata, rewrite the installed configuration, switch shared Ruff refs, overwrite shared artifacts, trust previous local reproductions, or substitute current dependency metadata.

## Required Return

Request:

- For source-attributable assignments, report-ready GitHub-flavored Markdown describing the exact base-versus-PR behavior and minimized code, plus separate working notes covering the original source permalink, reproduction, accepted reductions, both binaries' results, per-side run counts for intermittent severe failures, any necessary causal fingerprint, and the import audit.
- For outcomes without recoverable source evidence, report-ready GitHub-flavored Markdown describing the verified project outcomes, relevant stderr, panic evidence, and run frequencies.

If a later entry has exactly the same behavior change and cause as an already minimized entry, the subagent may classify it as a duplicate instead of repeating the full minimization, but it must explain the match.
