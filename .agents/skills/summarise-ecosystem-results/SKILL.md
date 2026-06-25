---
name: summarise-ecosystem-results
description: Use when a user says "summarise ecosystem results", "summarize this ty ecosystem report", "what changed in this ecosystem run?", or asks to summarise or summarize ty ecosystem results for a Ruff PR from a PR number, PR URL, GitHub ecosystem-results comment, or detailed HTML report.
---

# Summarise Ecosystem Results

## Priorities

1. Reproduce every retained behavior with the exact environment used by the Actions run.
2. Lead the report with analysis of diagnostic changes and clear minimized examples.
3. Keep execution, audit, and traceability bookkeeping out of the report.

## Deliverable

Create `PR_<number>_ECOSYSTEM_SUMMARY.md` at the repository root by adapting [assets/report-template.md](assets/report-template.md). The finished artifact must be GitHub-flavored Markdown suitable for a GitHub comment, with each prose paragraph and list item on one source line.

Use the template's structure and omissions as the report contract. Remove all placeholders and HTML comments. Link external source locations with permalinks such as `[project file.py:123](permalink)`; never emit raw URLs.

If summarising an ecosystem report is the only thing you're asked to do in a Codex App thread, you should rename that thread to "PR <number> ecosystem summary".

## Workflow

1. **Locate the evidence.** Normalize the input to a PR number, find the ty ecosystem-results comment, open the linked detailed HTML report, and identify the exact Actions run that produced it. Use the comment as the change list and the detailed report as evidence.
2. **Reproduce from scratch.** Ignore retained memories and previous local artifacts. Load the `minimizing-ty-ecosystem-changes` skill, use its metadata helper and exact-run workflow, and reproduce each report entry before explaining or minimizing it.
3. **Minimize and curate.** Retain the smallest clear reproducer for each distinct behavior change. Group entries only when the same base-to-PR behavior, explanation, and reproducer account for every entry in the group.
4. **Write and verify.** Fill the report template, check every link and diagnostic, then run `uvx prek run --files PR_<number>_ECOSYSTEM_SUMMARY.md`. Present the Markdown file as the finished product.

When parallelizing step 2, read [references/subagent-handoff.md](references/subagent-handoff.md). Otherwise, keep batches small and work through them sequentially.
