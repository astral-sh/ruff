<!-- Replace every placeholder and remove all HTML comments before presenting the report. Keep each prose paragraph and list item on one source line. Do not add change-count tables, bot-update timestamps, reproduction-completeness bookkeeping, import-audit details, exhaustive traceability appendices, raw URLs, or artifact hashes. -->

# [PR #<number>](https://github.com/astral-sh/ruff/pull/<number>) ecosystem summary

<Summarize the distinct diagnostic behavior changes and their significance. Lead with the analysis readers need; do not describe how the report was generated.>

## <Distinct behavior change>

**Report entries:** [<project file.py:line>](<permalink>)

<Explain the exact behavior on the merge base and PR. Group additional entries here only when the same explanation and minimized reproducer account for all of them.>

```python
# Merge base: <every full diagnostic message and error code, including duplicates, or no diagnostic>
# PR: <every full diagnostic message and error code, including duplicates, or no diagnostic>
<minimal reproducer>
```

## Reproduction

- Detailed report: [ecosystem-analyzer report](<report-url>)
- Actions run: [run <id>](<run-url>)
- Ruff comparison: [`<merge-base>`](https://github.com/astral-sh/ruff/commit/<merge-base>) to [`<pr-revision>`](https://github.com/astral-sh/ruff/commit/<pr-revision>)
- `ecosystem-analyzer`: [`<revision>`](https://github.com/astral-sh/ecosystem-analyzer/commit/<revision>)
- `mypy-primer`: [`<revision>`](https://github.com/hauntsaninja/mypy_primer/commit/<revision>)
- Project Python: `<project: version, ...>`
- Dependency cutoff: `<EXCLUDE_NEWER>`
- Comparison method: `<concise exact commands or method used to run both copied ty binaries>`
