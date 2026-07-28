# Advanced Minimization

Use this reference after the reported difference reproduces against the copied base and PR binaries.

## Target

Prefer a single-file reproducer with no third-party imports, few definitions, and the least complex typing or language features that still demonstrate the difference. Keep special modules such as `typing`, `abc`, `enum`, `types`, and `typing_extensions` only when removing them changes the behavior.

## Reduction Loop

Work systematically from the reproduced project. Do not skip ahead to an explanation, hand-written reproducer, or a guessed subset of relevant code. Follow the stages below in order and exhaust each stage before advancing. Try one controlled reduction at a time, run both copied ty binaries after every change, and keep the reduction only if the original difference remains. After every successful reduction, restart at step 1 because it may make earlier reductions possible.

1. Delete unrelated files.
2. Remove imports, definitions, decorators, annotations, statements, and branches.
3. Inline first-party definitions into the reproducer.
4. For each required third-party dependency, copy the entire installed dependency into the source tree as first-party code, including every package directory and module it provides. Do this before attempting to minimize any part of the dependency. Adjust imports, verify that the difference still reproduces with the complete copy, and only then begin deleting files or definitions from it. Never start by copying only apparently relevant files or definitions. If cloning a dependency is unavoidable, use the exact installed revision or version and copy the complete dependency into the source tree before reducing it.
5. Inline the relevant standard-library definitions from `crates/ty_vendored`, which is ty's source of truth for stdlib types.
6. Replace complex constructs with simpler equivalents, such as removing a walrus expression or replacing a protocol when the difference survives.

Repeat the full loop until an exhaustive pass through every stage finds no further reduction that preserves the difference. Do not stop merely because the likely cause is understood or the reproducer is already small.

## Final Audit

Attempt to remove every remaining import and inline every remaining third-party definition. Record why any surviving import is essential. Keep these notes as working evidence; the caller decides whether they belong in its final artifact.

Delete transient project and dependency copies after the investigation.
