<!-- Replace every placeholder and remove all HTML comments before presenting the report. Keep each prose paragraph and list item on one source line. Omit project-specific reproduction details when there are no affected projects. Do not add change-count tables, bot-update timestamps, reproduction-completeness bookkeeping, import-audit details, exhaustive traceability appendices, raw URLs, or artifact hashes. -->

# [PR #<number>](https://github.com/astral-sh/ruff/pull/<number>) ecosystem summary

<Summarize the distinct failure, flakiness, and diagnostic behavior changes and their significance. Lead with the analysis readers need; do not describe how the report was generated.>

<!-- Omit this entire section if no project failures or meaningful flaky outcomes changed. -->

## <New, fixed, or changed project failure or flaky outcome>

**Affected projects:**

- [<project>](<project-url>): merge base: `<base outcome>`; PR: `<PR outcome>`.

<Explain the crash, panic, timeout, abnormal exit, or flaky change, including relevant stderr and observed base/PR run frequencies where applicable.>

<!-- Omit this entire section if no diagnostic behavior changed. -->

## <Distinct behavior change>

**Report entries:**

- [<project1 file1.py:line>](<permalink>)
- [<project1 file2.py:line>](<permalink>)
- [<project2 file1.py:line>](<permalink>)

<Explain the exact behavior on the merge base and PR. Group additional entries here only when the same explanation and minimized reproducer account for all of them.>

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

```python
<minimal reproducer>
```

## Reproduction

- Detailed report: [ecosystem-analyzer report](<report-url>)
- Actions run: [run <id>, attempt <attempt>](<run-url>)
- Ruff comparison: [`<merge-base>`](https://github.com/astral-sh/ruff/commit/<merge-base>) to [`<pr-revision>`](https://github.com/astral-sh/ruff/commit/<pr-revision>)
- `ecosystem-analyzer`: [`<revision>`](https://github.com/astral-sh/ecosystem-analyzer/commit/<revision>)
- `mypy-primer`: [`<revision>`](https://github.com/hauntsaninja/mypy_primer/commit/<revision>)
- Dependency cutoff: `<EXCLUDE_NEWER>`
- Project Python: `<project: version, ...>`
- Project analysis mode: `<project: strict or non-strict, ...>`
- Comparison method: `<concise exact commands or method used to run both copied ty binaries, including --config analysis.strict-equality-semantics=true and --config analysis.strict-generic-narrowing=true for strict projects>`
