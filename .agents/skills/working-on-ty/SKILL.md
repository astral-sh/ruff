---
name: working-on-ty
description: Use when a user says "work on ty", "fix this ty issue", "debug ty", "update this ty PR", or asks about ty type checker changes, issues, branches, failing tests, or PRs in the Ruff repository.
---

# Working on ty

## Related skills

When the task matches a more specific ty workflow, also read and follow that skill from the repository root:

- Diagnostic changes, diagnostic message changes, or diagnostic reviews: `.agents/skills/adding-ty-diagnostics/SKILL.md`.
- Ecosystem report summaries: `.agents/skills/summarise-ecosystem-results/SKILL.md`.
- Reproducing, investigating, or minimizing ecosystem or primer differences: `.agents/skills/minimizing-ty-ecosystem-changes/SKILL.md`.

## PR conventions

When working on ty, PR titles should start with `[ty]`. Add the `ty` GitHub label if you have permission to do so;
if you don't, however, automation should add it anyway, so there's no need to worry about it. Similarly, add the `server`
label if your change only affects the LSP server and you have permission to add that label.

## The `db` parameter

For free functions and associated functions without a `self` parameter, `db` should be the first parameter. For methods with a `self` parameter, `db` should come immediately after `self`.

## Salsa tips

### Tracked functions and methods

Adding `#[salsa::tracked]` to a function or method means that the Salsa framework will cache the function/method.
This can sometimes be done for performance reasons, and can also be done to ensure incremental computation in an
IDE context.

ANY method that accesses `.node()` must be `#[salsa::tracked]`, or it will break incrementality.
Prefer higher-level semantic APIs over raw AST access.

### Reduce memory usage where possible

For Salsa-cached values, avoid retaining excess collection capacity. Prefer boxed slices; otherwise shrink collections that may have spare capacity before returning them. In particular, inspect `HashMap` and `HashSet` values constructed via `extend`, `collect`, explicit reservation, or removal, since those operations can leave capacity that insert-only construction does not.

Salsa caching can occur due to a function/method having `#[salsa::tracked]` on it, or due to a struct with `#[salsa::interned]` being constructed.
