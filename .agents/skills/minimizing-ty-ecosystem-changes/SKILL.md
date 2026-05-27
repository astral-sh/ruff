---
name: minimizing-ty-ecosystem-changes
description: Use when reproducing, investigating, or minimizing behavior changes in ty ecosystem or primer projects.
---

# Minimizing Ty Ecosystem Changes

Use this skill when asked to reproduce a ty ecosystem change, investigate a behavior difference in a primer project, or minimize a reproducer from a ty ecosystem project.

## Reproduce First

From the repository root, clone the project and install its dependencies into `.venv`:

```sh
uv run scripts/setup_primer_project.py <project-name> <some-temp-dir>
```

Confirm the behavior difference reproduces before minimizing or explaining it.

## Minimize

When asked to minimize an ecosystem change, start from the reproduced project. Reduce the Python code until only the smallest code needed to demonstrate the behavior difference remains. Even if the cause looks obvious, **do not skip ahead** to an explanation or a hand-written reproducer. Use a rigorous, systematic process and verify after each reduction that the behavior difference still reproduces.

An ideal minimized reproducer:

- Is a single file that is as small as possible.
- Has as few third-party imports as possible, ideally none.
- Uses smaller third-party packages with fewer dependencies when third-party imports are unavoidable. For example, prefer an import from `numpy` over one from `pandas`.
- Has as few first-party and standard-library imports as possible.
- Keeps imports from modules with highly special-cased symbols only when they are necessary (such modules may include `typing`, `abc`, `enum`, `types`, or `typing_extensions`).
- Uses an absolute minimum of "advanced"/complex typing or language features. For example, if the original code
  uses a walrus operator, but the behavior difference still reproduces without it, remove the walrus operator. If the original code uses a `Protocol` from `typing_extensions`, but the behavior difference still reproduces without it, remove the `Protocol`.

Use a systematic loop. You do not need to apply every tool in every iteration, but keep looping until none of the available minimization tools can reduce the reproducer further while preserving the behavior difference.

Available minimization tools include:

1. Remove files that are not needed for the difference.
2. Strip imports, definitions, and statements from the remaining files.
3. Cut and paste definitions from one file into another file to reduce first-party or third-party imports.
4. Inline the relevant parts of third-party dependencies into first-party code to reduce third-party imports.
5. Inline the relevant parts of stdlib definitions from typeshed into first-party code to reduce stdlib imports.

After applying a minimization tool, re-run the comparison between the feature branch and `main`.
Keep a reduction only when the behavior difference still reproduces.

Stop looping only when no further reductions could be applied without making the behavior difference disappear.
