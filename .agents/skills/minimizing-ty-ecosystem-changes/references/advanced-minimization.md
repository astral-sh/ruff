# Advanced Minimization

Use this reference after the reported difference reproduces against the copied base and PR binaries.

## Target

Prefer a single-file reproducer with no avoidable third-party imports, few definitions, and the least complex typing or language features that still demonstrate the difference. Keep special modules such as `typing`, `abc`, `enum`, `types`, and `typing_extensions` only when neither removing them nor inlining their definitions preserves the behavior; retain a third-party import only after identifying ty behavior that depends on that library's identity or third-party search-path classification.

## Reduction Loop

Work systematically from the reproduced project. NEVER skip ahead to an explanation, hand-written reproducer, or a guessed subset of relevant code. Follow the stages below in order and exhaust each stage before advancing. Try one controlled reduction at a time, run both copied ty binaries after every change, and keep the reduction only if the original difference and underlying trigger remain. After every successful reduction, restart at step 1 because it may make earlier reductions possible.

1. Delete unrelated files.
2. Remove imports, definitions, decorators, annotations, statements, and branches.
3. Inline first-party definitions into the reproducer.
4. For each required third-party dependency, copy the entire installed dependency into the source tree as first-party code, including every package directory and module it provides. Do this before attempting to minimize any part of the dependency. Adjust imports, verify that the difference still reproduces with the complete copy, and only then begin deleting files or definitions from it. If the complete copy changes the behavior because ty special-cases that library or distinguishes first-party from third-party search paths, identify the relevant ty implementation at the matching analyzed Ruff revision before retaining the original import. Never start by copying only apparently relevant files or definitions. If cloning a dependency is unavoidable, use the exact installed revision or version and copy the complete dependency into the source tree before reducing it.
5. Inline the relevant standard-library definitions from the analyzed revision of `crates/ty_vendored`, using `git -C <ruff-checkout> show <exact-revision>:crates/ty_vendored/<path>`; compare the merge-base and PR definitions when they differ.
6. Replace complex constructs with simpler equivalents, such as removing a walrus expression or replacing a protocol when the difference survives.

Repeat the full loop until an exhaustive pass through every stage finds no further reduction that preserves the difference. Do not stop merely because the likely cause is understood or the reproducer is already small.

## Final Audit

Attempt to remove every remaining import and inline its definitions, including remaining third-party and standard-library definitions. Retain an import only after verifying that neither removal nor inlining preserves the underlying behavior. For a third-party import, additionally verify that its module identity or third-party search-path classification is essential and identify the relevant ty implementation. Convenience, familiar APIs, matching class names, or preserving the diagnostic's module spelling do not justify keeping an import. Record why any surviving import is essential and, for a third-party import, where ty implements the relevant behavior. Keep these notes as working evidence; the caller decides whether they belong in its final artifact.

Verify that the recorded reduction chain connects the final reproducer to the original ecosystem entry and, when diagnostic output is ambiguous, preserves the original causal fingerprint. If a required check fails, continue investigating; if a genuine external blocker prevents completion, report the blocker and mark the minimization as incomplete instead of presenting an unrelated example or original excerpt as a minimized result.

Delete transient project and dependency copies after the investigation.
