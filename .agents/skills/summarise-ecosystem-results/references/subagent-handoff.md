# Subagent Handoff

Use this reference only when parallelizing reproduction and minimization.

## Primary-Agent Responsibilities

Prepare the copied base binary, PR binary, PR ecosystem config, and run-metadata manifest once. Treat them as read-only shared inputs. Batch related entries without creating more assignments than can run concurrently.

## Assignment Checklist

Give each subagent:

- The PR, ecosystem comment, and detailed report links.
- The exact report entries assigned to it.
- The paths to the copied binaries, copied config, and metadata manifest.
- The instruction to follow the `minimizing-ty-ecosystem-changes` skill using a unique temporary directory.
- The instruction not to rebuild ty, switch Ruff refs, overwrite shared artifacts, trust previous local reproductions, or substitute current dependency metadata.

## Required Return

Request:

- Report-ready GitHub-flavored Markdown describing the exact base-versus-PR behavior and minimized code.
- Separate working notes covering reproduction, reductions, and the import audit.

If a later entry has exactly the same behavior change and cause as an already minimized entry, the subagent may classify it as a duplicate instead of repeating the full minimization, but it must explain the match.
