---
name: summarise-ecosystem-results
description: Use when a user says "summarise ecosystem results", "summarize this ty ecosystem report", "what changed in this ecosystem run?", or asks to summarise or summarize ty ecosystem results for a Ruff PR from a PR number, PR URL, GitHub ecosystem-results comment, or detailed HTML report.
---

# Summarise Ecosystem Results

## Priorities

1. Reproduce every retained source-attributable behavior with the exact environment used by the Actions run.
2. For every distinct source-attributable behavior change, produce the smallest provenance-preserving reproducer obtainable through the complete advanced-minimization workflow.
3. Eliminate every third-party import unless identified ty behavior depends on that library's identity or third-party search-path classification, and eliminate every unnecessary standard-library import. Retain an import only after verifying that neither removing it nor inlining its definitions preserves the underlying behavior.
4. Lead the report with new or meaningfully changed project failures, including intermittent severe failures, then cover stable diagnostic changes and fully minimized examples.
5. Keep execution, audit, and traceability bookkeeping out of the report.

## GitHub CLI Telemetry

Prefix every direct or indirect `gh` invocation with `GH_TELEMETRY=false`, including `GH_TELEMETRY=false uv run --script scripts/collect_ty_ecosystem_run_metadata.py ...`. Require the same of subagents. Codex tool calls may start separate shells, so an `export` in an earlier call is insufficient.

## Deliverable

Create `PR_<number>_ECOSYSTEM_SUMMARY.md` at the repository root by adapting [assets/report-template.md](assets/report-template.md). The finished artifact must be GitHub-flavored Markdown suitable for a GitHub comment, with each prose paragraph and list item on one source line.

Use the template's structure and omissions as the report contract. Remove all placeholders and HTML comments. Link external source locations with permalinks such as `[project file.py:123](permalink)`; never emit raw URLs.

If summarising an ecosystem report is the only thing you're asked to do in a Codex App thread, you should rename that thread to "PR <number> ecosystem summary".

## Reporting Policy

- Focus on new or meaningfully changed behavior relative to the merge base. Evaluate individual diagnostics and failure outcomes, not a project's overall flaky or persistent status.
- Omit flaky diagnostic changes, unchanged failures, and frequency fluctuations that leave the observed outcomes unchanged.
- Report new, fixed, or meaningfully changed panics, crashes, overflows, and timeouts, including merge-base and PR run frequencies when intermittent behavior is involved.

## Workflow

1. **Freeze the evidence.** Preserve any report URL or ecosystem-results comment explicitly supplied by the user before identifying the PR. For PR-only input, find its ecosystem-results comment and linked detailed report. Capture the matching Actions run and attempt as described in [references/evidence-acquisition.md](references/evidence-acquisition.md); never replace a supplied report with the PR's current report. Recover exact-run metadata promptly, then prepare both exact-revision profiling binaries and the shared configuration before assigning subagent work. Ignore later comment edits, PR updates, and workflow runs. Prefer the selected attempt's validated `full-report/diff.json` as the authoritative structured change inventory, retain its matching frozen HTML report, and use the comment for orientation when available. Fall back to the frozen HTML report if the JSON report is unavailable.
2. **Identify changed outcomes.** Inspect the structured diff for added, removed, and modified projects; stable diagnostic additions, removals, and rewrites; project failures; and intermittent exit-status changes. Preserve diagnostic levels, duplicate occurrences, source permalinks, project strictness, panic evidence, and observed run frequencies. Exclude flaky diagnostics and frequency-only noise without excluding stable diagnostics or changed severe failures from flaky projects. Use the matching HTML report for visual context, or as the primary evidence when structured JSON cannot be obtained safely.
3. **Reproduce from scratch.** Ignore retained memories and previous local artifacts. Load the `minimizing-ty-ecosystem-changes` skill, collect exact-run metadata once, and reproduce every retained, source-attributable diagnostic or panic before explaining or minimizing it. Reproduce intermittent severe failure changes with the reported merge-base and PR run counts. Verify retained outcomes without recoverable source against their captured statuses, stderr, panic evidence, and run frequencies.
4. **Minimize to completion with provenance.** For each distinct source-attributable behavior change, follow the complete advanced-minimization workflow until an exhaustive pass finds no further reduction. Derive the reproducer from a cited ecosystem entry through a verified reduction chain; never replace that entry with an independently invented example demonstrating superficially similar behavior. Before accepting a reproducer, attempt to remove every import, inline every third-party definition, and inline relevant standard-library definitions. Retain a third-party import only when identified ty behavior depends on that library's identity or third-party search-path classification and neither removing the import nor inlining its definitions preserves the underlying behavior. If a genuine external blocker prevents completion, report that blocker to the user and identify the task as incomplete. Do not silently substitute an unminimized excerpt or present a partially minimized report as finished.
5. **Group by cause.** Group entries only when the same base-to-PR behavior, underlying trigger, explanation, and reproducer account for every entry. Identical diagnostic text or displayed `@Todo` types do not establish equivalence.
6. **Find existing ty issues.** When a diagnostic change exposes a pre-existing shortcoming in ty, search the `astral-sh/ty` issue tracker for the precise underlying behavior. Link matching issues directly from the relevant report section; do not mistake incorrect or incomplete third-party annotations for ty shortcomings.
7. **Write and verify.** Fill the report template and verify that every source-attributable behavior change has a fully minimized, provenance-preserving reproducer. Check every change number, link, diagnostic, retained import, reproducer's source provenance, and causal fingerprint when required. Verify that every retained third-party import is essential to identified ty behavior that depends on that library's identity or third-party search-path classification, that no avoidable standard-library import remains, and that no source-attributable section contains an unminimized excerpt. Then run `GH_TELEMETRY=false uv run --only-group dev --locked prek run --files PR_<number>_ECOSYSTEM_SUMMARY.md`. Present the Markdown file as the finished product only after these checks pass.

## Parallel execution

This skill explicitly requests subagents when the report contains multiple affected projects or independently investigable entries.

Once the exact-run metadata, both profiling binaries, and shared configuration are ready, spawn as many subagents as the available concurrency budget and independent work allow, reserving one slot for the primary agent. Keep available slots occupied by assigning further work as subagents finish.

Assign disjoint projects or explicit report entries. Apparent similarity may guide scheduling, but does not establish causal equivalence. Follow all existing requirements for exhaustive reproduction, verified reduction chains, exhaustive minimization, and grouping by verified cause.

The primary agent owns the frozen evidence, shared profiling binaries, configuration, coordination, and final report. Follow [references/subagent-handoff.md](references/subagent-handoff.md) for handoff and shared-artifact requirements.

If multiple independent assignments exist but no subagents are spawned, record the specific reason.
