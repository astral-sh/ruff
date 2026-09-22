<!--
Replace every placeholder and remove all HTML comments before presenting the report. Keep each prose paragraph and list item on one source line. Number retained change subsections consecutively within each section, restarting at 1 for each section. Try very hard to keep the report below 100 change subsections, but exceed that target when necessary for clear, exhaustive coverage. Combine related causes under a shared theme with separate explanations and examples wherever their behavior differs; do not conflate distinct causes merely to reduce the subsection count.

Order diagnostic sections and subsections by descending total ecosystem hit count. Include per-rule counts in every title and example label, for example, "Callback argument checking (18 invalid-argument-type; 6 no-matching-overload)". Count represented diagnostic occurrences, including duplicates, rather than projects or examples. Each added or removed occurrence contributes one hit for its rule; do not net additions against removals. A verified same-rule rewrite, such as a message change, contributes one changed hit rather than one hit per revision. Replacing one invalid-argument-type diagnostic with one no-matching-overload diagnostic contributes one removed hit for the former and one added hit for the latter: two hits in total. Compute diagnostic totals by summing the per-rule counts.

For failure titles, count one hit per affected project and distinct reported failure outcome, regardless of how many runs exhibit it. A crash newly observed in three of ten runs contributes one failure hit; retain the 3/10 frequency in the affected-project entry alongside the merge-base frequency. Keep failure counts separate from diagnostic totals, and include per-rule counts if a failure subsection also covers diagnostic changes.

Keep each subsection's exhaustive bulleted entry inventory inside a details block; keep its explanation and examples outside. Identify each diagnostic's source permalink, rule, and whether it was added, removed, or changed. Preserve duplicate occurrences with explicit multiplicities. For failures without source diagnostics, list the affected project outcomes. Reconcile each subsection's inventory and examples with its title counts, and all subsection inventories with the retained report inventory so that every retained entry is represented exactly once.

Do not mention the absence of new panics, overflows, or timeouts. Do not add change-count tables, bot-update timestamps, reproduction-completeness bookkeeping, import-audit details, exhaustive traceability appendices, raw URLs, or artifact hashes.
-->

# [PR #<number>](https://github.com/astral-sh/ruff/pull/<number>) ecosystem summary

<Summarize meaningful changes to project failures and diagnostic behavior, including changes involving intermittent severe failures, along with their significance. Lead with the analysis readers need; do not describe how the report was generated.>

<!-- Omit this entire section if no stable project failures changed. Repeat its numbered subsection for each distinct failure. -->

## Project failures (<per-outcome and per-rule hit counts, as applicable>)

### 1. <New, fixed, or changed project failure> (<per-outcome and per-rule hit counts, as applicable>)

<details>
<summary>Affected projects</summary>

- [<project>](<project-url>): merge base: `<base outcome>`; PR: `<PR outcome>`.

</details>

<Explain the crash, panic, overflow, timeout, or abnormal exit, including relevant stderr where applicable.>

<!-- Include the verified minimized reproducer when the failure can be attributed to source code. Omit this code block when source evidence cannot be recovered. -->

```python
<minimal reproducer>
```

<!-- Omit this entire section if no severe failure involving intermittent outcomes changed. Repeat its numbered subsection for each distinct change. -->

## Intermittent severe failures (<per-outcome and per-rule hit counts, as applicable>)

### 1. <New or changed intermittent panic, crash, overflow, or timeout> (<per-outcome and per-rule hit counts, as applicable>)

<details>
<summary>Affected projects</summary>

- [<project>](<project-url>): merge base: `<base outcome and count/runs, or not present>`; PR: `<PR outcome and count/runs, or not present>`.

</details>

<Explain the change in failure behavior and relevant stderr. Do not include unchanged failures or frequency-only fluctuations.>

<!-- Include the verified minimized reproducer when the failure can be attributed to source code. Omit this code block when source evidence cannot be recovered. -->

```python
<minimal reproducer>
```

<!-- Omit this entire section if no stable diagnostic behavior changed. Organize related causes into thematic subsections. -->

## Diagnostic changes (<count> <rule>; <count> <other-rule>)

### 1. <Common theme or behavior change> (<count> <rule>; <count> <other-rule>)

<details>
<summary>Report entries (<total> diagnostic hits)</summary>

- [<project1 file1.py:line>](<permalink>): <added, removed, or changed> `<rule>`.
- [<project1 file2.py:line>](<permalink>): <added, removed, or changed> `<other-rule>` (<count> duplicate occurrences).
- [<project2 file1.py:line>](<permalink>): <added, removed, or changed> `<rule>`.

</details>

<Explain the common theme and exact behavior on the merge base and PR. Distinguish related causes with separate explanations and examples, and identify which entries each example explains. Cover every changed rule with an example; one example may cover multiple rules when the same cause and explanation account for all of them.>

<!-- If this diagnostic change exposes an existing ty shortcoming, search astral-sh/ty for issues covering that exact shortcoming. Include the following paragraph only when a matching issue exists. -->

**Existing ty issues:** [ty#<issue-number>](https://github.com/astral-sh/ty/issues/<issue-number>)

<!--
A minimal reproducer should annotate every line with a new, changed, or removed diagnostic using comments immediately above that line. Include the full error messages and error codes from both revisions, including duplicates.

For example:

```python
from typing import Final

# Merge base: `[error-code-1] "Some error message"`
# PR: no diagnostic
x: Final = 42

if x:
    # Merge base: `[error-code-2] "Some error message"`
    # PR: `[error-code-2] "Some other error message"`
    Y = 56
```
-->

**<Example description> (<count> <rule>; <count> <other-rule>)**

```python
<minimal reproducer>
```

<!-- Add examples only for changed rules or distinct causes not already covered. Do not repeat an equivalent reproducer merely to give another rule a separate example. Use prose labels rather than extra subsection headings. -->

## Reproduction

- Detailed report: [ecosystem-analyzer report](<report-url>)
- Actions run: [run <id>, attempt <attempt>](<run-url>)
- Ruff comparison: [`<merge-base>`](https://github.com/astral-sh/ruff/commit/<merge-base>) to [`<pr-revision>`](https://github.com/astral-sh/ruff/commit/<pr-revision>)
- `ecosystem-analyzer`: [`<revision>`](https://github.com/astral-sh/ecosystem-analyzer/commit/<revision>)
- `mypy-primer`: [`<revision>`](https://github.com/hauntsaninja/mypy_primer/commit/<revision>)
- Dependency cutoff: `<EXCLUDE_NEWER>`
- Project Python: `<project: version, ...>`
- Python target platform: `<project: effective platform, ...>`
- Execution environment: `<OS and architecture used for reproduction; uv version; relevant checker environment settings, including TY_UV set or unset; any difference from the CI runner>`
- Checker deadline: `<deadline in seconds; analyzer profile>`
- Project analysis mode: `<project: strict or non-strict, ...>`
- Comparison method: `<concise exact commands or method used to run both copied ty binaries, including --config analysis.strict-equality-semantics=true and --config analysis.strict-generic-narrowing=true for strict projects>`
