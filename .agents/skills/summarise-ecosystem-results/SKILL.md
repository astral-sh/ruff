---
name: summarise-ecosystem-results
description: Use when a user says "summarise ecosystem results", "summarize this ty ecosystem report", "what changed in this ecosystem run?", or asks to summarise or summarize ty ecosystem results for a Ruff PR from a PR number, PR URL, GitHub ecosystem-results comment, or detailed HTML report.
---

# Summarise Ecosystem Results

Use this skill when asked to summarise ecosystem results for a Ruff PR with the `ty` label.

## Find the Report

Accept any of these inputs:

- A PR number.
- A GitHub PR URL.
- A GitHub comment URL on a PR, such as `https://github.com/astral-sh/ruff/pull/25342#issuecomment-4525002693`.
- A full detailed HTML ecosystem report URL.

Determine the PR number first. If the user gave only a PR number, open `https://github.com/astral-sh/ruff/pull/<number>`.

Find the ty ecosystem-results comment on the PR. Search PR comments for terms such as "ecosystem", "full report", "HTML report", and "detailed report". From that comment, open the linked full detailed HTML report.

Use the PR comment as the change list and the full detailed HTML report as the source of detailed evidence. When the report includes exact project revisions, use those revisions rather than current upstream checkouts.

## Minimize in Parallel

Before minimizing, load and apply the `minimizing-ty-ecosystem-changes` skill to each ecosystem change.

If possible, use subagents to parallelize this work. Decide how to batch changes so the overall task finishes as quickly as possible while still allowing each subagent to work methodically. Reasonable batching strategies include grouping related changes by project, diagnostic code, suspected cause, or report section, while keeping large groups split enough to avoid one slow subagent blocking the whole task. DO NOT spawn more subagents than you can run in parallel.

If subagents are not available, batch the minimization work manually and minimize the batches sequentially. Keep batches small enough that each pass can still be checked carefully.

Give each subagent a self-contained assignment:

- The PR number, PR URL, ecosystem comment URL, and detailed HTML report URL.
- The exact ecosystem changes assigned to that subagent.
- The requirement to use the `minimizing-ty-ecosystem-changes` process rigorously.
- The expected Markdown output format for each minimized change.

Each subagent should proceed methodically through all assigned changes. If a subagent moves on to a new change and that change appears very similar to one it has already minimized, it may skip the new change without completing the full minimisation skill, but it must record why the skipped change appears to demonstrate the same behavior.

## Collect Results

After all subagents finish, collect their minimizations into one Markdown file at the repository root:

```text
PR_<number>_ECOSYSTEM_SUMMARY.md
```

Remove minimizations that appear to demonstrate the same behavior change. Prefer the smallest and clearest minimized reproducer, especially one that is single-file and has fewer imports.

At the top of the file, add prose summarising the distinct behavior changes demonstrated by the retained minimizations. Then include the retained minimizations with enough detail for a reader to understand and reproduce them.

For each retained minimization, include:

- The original project and report entry.
- The diagnostic or behavior change on `main` versus the PR.
- The minimized code.
- The commands or comparison method used to verify the minimization.

ALL references to exact line numbers in source code should use GitHub/GitLab/Codeberg/etc. permalinks so that readers can jump to the exact line in the original source code that led to the diagnostic changing. The finished report should NEVER include "raw" URL links; it should ALWAYS use inline Markdown links with square brackets and parentheses.

Present `PR_<number>_ECOSYSTEM_SUMMARY.md` as the finished product.
