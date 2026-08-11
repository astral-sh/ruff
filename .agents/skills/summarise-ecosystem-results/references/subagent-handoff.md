# Subagent Handoff

Use this reference only when parallelizing reproduction and minimization.

## Primary-Agent Responsibilities

Prepare the frozen evidence snapshot, copied base binary, PR binary, PR ecosystem config, and one run-metadata manifest covering every affected project. Record the binaries' absolute paths as `TY_ECOSYSTEM_BASE_BINARY` and `TY_ECOSYSTEM_PR_BINARY`. Choose an absolute `TY_ECOSYSTEM_CONFIG_HOME` and install the copied config once at `$TY_ECOSYSTEM_CONFIG_HOME/ty/ty.toml`. Treat the snapshot, binaries, manifest, copied config, and installed configuration as read-only shared inputs. Batch related entries without creating more assignments than can run concurrently.

## Assignment Checklist

Give each subagent:

- The PR and detailed report links, plus the ecosystem comment link when available.
- The paths to the frozen detailed report and available diagnostics shards, plus the frozen comment path when available and the selected Actions run and attempt; use these captured inputs instead of refetching live evidence.
- The exact report entries assigned to it.
- The copied-config and metadata-manifest paths, plus the shared `TY_ECOSYSTEM_BASE_BINARY`, `TY_ECOSYSTEM_PR_BINARY`, and `TY_ECOSYSTEM_CONFIG_HOME` values.
- The instruction to follow the `minimizing-ty-ecosystem-changes` skill using a unique temporary directory.
- The instruction to preserve a verified reduction chain and underlying trigger, or return the original source explicitly marked as unminimized.
- Permission to build an exact-revision debug binary on demand for causal inspection, using an isolated worktree if necessary and retaining the profiling binaries as the behavioral oracle.
- The instruction not to rebuild profiling binaries, regenerate the supplied manifest, rewrite the installed configuration, switch shared Ruff refs, overwrite shared artifacts, trust previous local reproductions, or substitute current dependency metadata.

## Required Return

Request:

- Report-ready GitHub-flavored Markdown describing the exact base-versus-PR behavior and minimized code.
- Separate working notes covering the original source permalink, reproduction, accepted reductions, both binaries' results, any necessary causal fingerprint, and the import audit.

If a later entry has exactly the same behavior change and cause as an already minimized entry, the subagent may classify it as a duplicate instead of repeating the full minimization, but it must explain the match.
