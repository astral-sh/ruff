---
name: summarise-ecosystem-results
description: Use when a user says "summarise ecosystem results", "summarize this ty ecosystem report", "what changed in this ecosystem run?", or asks to summarise or summarize ty ecosystem results for a Ruff PR from a PR number, PR URL, GitHub ecosystem-results comment, or detailed HTML report.
---

# Summarise Ecosystem Results

## Priorities

1. **Produce a readable document that pulls out common themes and patterns across the ecosystem.** Explain what changed, why it changed, and which recurring code patterns account for the effects. Exhaustive reproduction, minimized examples, and complete entry inventories provide the evidence for this synthesis.
2. Reproduce every retained source-attributable behavior with the exact environment used by the Actions run.
3. Minimize every distinct source-attributable behavior change using the [minimizing-ty-ecosystem-changes skill](../minimizing-ty-ecosystem-changes/SKILL.md).
4. Highlight new or meaningfully changed project failures, including intermittent severe failures, in the opening summary.
5. Keep execution, audit, and traceability bookkeeping out of the report, except for the entry inventories and concise reproduction information required by the template.

## GitHub CLI Telemetry

Prefix every direct or indirect `gh` invocation with `GH_TELEMETRY=false`, including `GH_TELEMETRY=false uv run --script scripts/collect_ty_ecosystem_run_metadata.py ...`. Require the same of subagents. Codex tool calls may start separate shells, so an `export` in an earlier call is insufficient.

## Deliverable

Create `PR_<number>_ECOSYSTEM_SUMMARY.md` at the repository root by adapting [assets/report-template.md](assets/report-template.md). The finished artifact must be GitHub-flavored Markdown suitable for a GitHub comment, with each prose paragraph and list item on one source line.

Use the template's structure and omissions as the report contract. Remove all placeholders and HTML comments. Link external source locations with permalinks such as `[project file.py:123](permalink)`; never emit raw URLs.

If summarising an ecosystem report is the only thing you're asked to do in a Codex App thread, you should rename that thread to "PR <number> ecosystem summary".

## Reporting Policy

- Focus on new or meaningfully changed behavior relative to the merge base. Evaluate individual diagnostics and failure outcomes, not a project's overall flaky or persistent status.
- Ignore all `unknown-rule` diagnostic changes from ecosystem-analyzer. Exclude them from reproduction assignments, report entries, and hit counts.
- Omit flaky diagnostic changes, unchanged failures, and frequency fluctuations that leave the observed outcomes unchanged.
- Report new, fixed, or meaningfully changed panics, crashes, overflows, and timeouts, including merge-base and PR run frequencies when intermittent behavior is involved.

## Workflow

1. **Freeze the evidence.** Preserve any report URL or ecosystem-results comment explicitly supplied by the user before identifying the PR. For PR-only input, find its ecosystem-results comment and linked detailed report. Capture the matching Actions run and attempt as described in [references/evidence-acquisition.md](references/evidence-acquisition.md); never replace a supplied report with the PR's current report. Recover exact-run metadata promptly, review runtime evidence as described in the minimizing skill, then prepare both exact-revision profiling binaries and the shared configuration in the chosen execution environment before assigning subagent work. Ignore later comment edits, PR updates, and workflow runs. Prefer the selected attempt's validated `full-report/diff.json` as the authoritative structured change inventory, retain its matching frozen HTML report, and use the comment for orientation when available. Fall back to the frozen HTML report if the JSON report is unavailable.
2. **Identify changed outcomes.** Inspect the structured diff for added, removed, and modified projects; stable diagnostic additions, removals, and rewrites; project failures; and intermittent exit-status changes. Preserve diagnostic levels, duplicate occurrences, source permalinks, project strictness, panic evidence, and observed run frequencies. Apply Reporting Policy above without excluding stable diagnostics or changed severe failures merely because they come from flaky projects. Use the matching HTML report for visual context, or as the primary evidence when structured JSON cannot be obtained safely.
3. **Reproduce from scratch.** Ignore retained memories and previous local artifacts. Load the `minimizing-ty-ecosystem-changes` skill, collect exact-run metadata once, and reproduce every retained, source-attributable diagnostic or panic before explaining or minimizing it. Reproduce intermittent severe failure changes with the reported merge-base and PR run counts. Verify retained outcomes without recoverable source against their captured statuses, stderr, panic evidence, and run frequencies.
4. **Minimize to completion with provenance.** For each distinct source-attributable behavior change, complete the minimizing skill's advanced-minimization workflow and final audit. If a genuine external blocker prevents completion, report that blocker to the user and identify the report as incomplete.
5. **Deduplicate and synthesize.** After reproducing every retained diagnostic, deduplicate reproducers only when the same base-to-PR behavior, underlying trigger, explanation, and reproducer account for every represented entry. Identical diagnostic text or displayed `@Todo` types do not establish equivalence. Review the complete set of findings together, including results from different subagents, to identify recurring code patterns and shared causes across projects. Build the report around those themes, explaining the connections between representative examples and the broader ecosystem effects, following the report template.
6. **Find existing ty issues.** When a diagnostic change exposes a pre-existing shortcoming in ty, search the `astral-sh/ty` issue tracker for the precise underlying behavior. Link matching issues directly from the relevant report section; do not mistake incorrect or incomplete third-party annotations for ty shortcomings.
7. **Write and verify.** First verify that a reader can understand the main ecosystem patterns and their significance from the narrative and representative examples. Check the report template's presentation and coverage requirements. Verify that every source-attributable behavior change has a reproducer that satisfies the minimizing skill's completion criteria. The primary agent must verify that every retained import is necessary: neither removing it nor inlining its definitions preserves the underlying behavior. For retained third-party imports, also verify that the library's identity or third-party search-path classification is essential to identified ty behavior. Check every change number, link, diagnostic, reproducer's source provenance, and causal fingerprint when required. Then run `GH_TELEMETRY=false uv run --only-group dev --locked prek run --files PR_<number>_ECOSYSTEM_SUMMARY.md`. Present the Markdown file as the finished product only after these checks pass.

## Parallel execution

This skill explicitly requests subagents when the report contains multiple affected projects or independently investigable entries.

Once the exact-run metadata, both profiling binaries, and shared configuration are ready, spawn as many subagents as the available concurrency budget and independent work allow, reserving one slot for the primary agent. Keep available slots occupied by assigning further work as subagents finish.

Assign disjoint projects or explicit report entries. Apparent similarity may guide scheduling, but does not establish causal equivalence. Follow all existing requirements for exhaustive reproduction, verified reduction chains, exhaustive minimization, and grouping by verified cause.

The primary agent owns the frozen evidence, shared profiling binaries, configuration, coordination, and final report. Follow [references/subagent-handoff.md](references/subagent-handoff.md) for handoff and shared-artifact requirements.

If multiple independent assignments exist but no subagents are spawned, record the specific reason.
