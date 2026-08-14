---
name: summarise-ecosystem-results
description: Use when a user says "summarise ecosystem results", "summarize this ty ecosystem report", "what changed in this ecosystem run?", or asks to summarise or summarize ty ecosystem results for a Ruff PR from a PR number, PR URL, GitHub ecosystem-results comment, or detailed HTML report.
---

# Summarise Ecosystem Results

## Priorities

1. Reproduce every retained behavior with the exact environment used by the Actions run.
2. Lead the report with new or meaningfully changed project failures, including intermittent severe failures, then cover stable diagnostic changes and clear minimized examples.
3. Keep execution, audit, and traceability bookkeeping out of the report.

## Deliverable

Create `PR_<number>_ECOSYSTEM_SUMMARY.md` at the repository root by adapting [assets/report-template.md](assets/report-template.md). The finished artifact must be GitHub-flavored Markdown suitable for a GitHub comment, with each prose paragraph and list item on one source line.

Use the template's structure and omissions as the report contract. Remove all placeholders and HTML comments. Link external source locations with permalinks such as `[project file.py:123](permalink)`; never emit raw URLs.

If summarising an ecosystem report is the only thing you're asked to do in a Codex App thread, you should rename that thread to "PR <number> ecosystem summary".

## Reporting Policy

- Focus on new or meaningfully changed behavior relative to the merge base. Evaluate individual diagnostics and failure outcomes, not a project's overall flaky or persistent status.
- Omit flaky diagnostic changes, unchanged failures, and frequency fluctuations that leave the observed outcomes unchanged.
- Report new or changed panics, crashes, overflows, and timeouts, including merge-base and PR run frequencies when intermittent behavior is involved.

## Workflow

1. **Freeze the evidence.** Preserve any report URL or ecosystem-results comment explicitly supplied by the user before identifying the PR. For PR-only input, find its ecosystem-results comment and linked detailed report. Capture the matching Actions run and attempt as described in [references/evidence-acquisition.md](references/evidence-acquisition.md); never replace a supplied report with the PR's current report. Ignore later comment edits, PR updates, and workflow runs. Use the frozen detailed report as the authoritative change list and the comment for orientation when available.
2. **Identify changed outcomes.** Check the detailed report for new, fixed, or changed project failures, panics, overflows, timeouts, abnormal exits, and diagnostic changes, applying the reporting policy to each entry and outcome.
3. **Reproduce from scratch.** Ignore retained memories and previous local artifacts. Load the `minimizing-ty-ecosystem-changes` skill, use its metadata helper and exact-run workflow, and reproduce each report entry before explaining or minimizing it. Reproduce intermittent severe failure changes with the reported run counts.
4. **Minimize with provenance.** Include a standalone reproducer only when a verified reduction chain connects it to a cited ecosystem entry and preserves the same underlying trigger. If either cannot be verified, retain the original source excerpt and identify it as unminimized.
5. **Group by cause.** Group entries only when the same base-to-PR behavior, underlying trigger, explanation, and reproducer account for every entry. Identical diagnostic text or displayed `@Todo` types do not establish equivalence.
6. **Find existing ty issues.** When a diagnostic change exposes a pre-existing shortcoming in ty, search the `astral-sh/ty` issue tracker for the precise underlying behavior. Link matching issues directly from the relevant report section; do not mistake incorrect or incomplete third-party annotations for ty shortcomings.
7. **Write and verify.** Fill the report template, record each affected project's strict or non-strict analysis mode, and include both strict-analysis flags in the comparison method when applicable. Check every change number, link, diagnostic, reproducer's source provenance, and causal fingerprint when required, then run `uv run --only-group dev --locked prek run --files PR_<number>_ECOSYSTEM_SUMMARY.md`. Present the Markdown file as the finished product.

When parallelizing reproduction or minimization, read [references/subagent-handoff.md](references/subagent-handoff.md). Otherwise, keep batches small and work through them sequentially.
