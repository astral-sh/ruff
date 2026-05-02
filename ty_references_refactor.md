# ty references refactor notes

## Summary

This refactor changes find-references and rename from comparing navigation-oriented targets to comparing semantic definition results.

Previously, references converted each resolved symbol into `DefinitionTargets`/`NavigationTargets` early and then compared those targets. That meant the matching logic depended on the same representation used for “where should the editor jump?”.

The new approach keeps semantic definitions as the matching currency:

1. Resolve the target under the cursor to `ResolvedDefinition`s.
2. Convert those into `DefinitionMatch` values, which keep the original `ResolvedDefinition` plus cached metadata needed by references.
3. Resolve each candidate occurrence in the same way.
4. Treat the occurrence as a match when the two `ResolvedDefinition` sets intersect.
5. Convert to LSP-facing `ReferenceTarget`s only after a semantic match has been found.

## Old approach

The old approach used a single navigation-adjacent representation for multiple jobs:

- semantic identity matching
- declaration skipping
- goto/navigation output
- source range comparison

That made `DefinitionTarget` carry fields that were not naturally part of a navigation target, such as whether the target represented the origin occurrence. It also meant references compared locations that were designed for editor navigation, not for symbol identity.

The biggest issue was conceptual coupling: a navigation target answers “where should the user jump?”, while references needs to answer “does this occurrence resolve to the same symbol?”. Those are related, but not identical.

## New approach

The new reference path uses `ResolvedDefinition` as the semantic identity:

```rust
ResolvedDefinition::Definition(definition)
ResolvedDefinition::Module(file)
ResolvedDefinition::FileWithRange(file_range)
```

`DefinitionMatch` is now just a cached wrapper:

```rust
struct DefinitionMatch<'db> {
    resolved_definition: ResolvedDefinition<'db>,
    category: DefinitionCategory,
    focus_range: FileRange,
    full_range: FileRange,
}
```

The identity is explicitly `resolved_definition`. The other fields are derived metadata used by find-references:

- `category`: decides which declaration/binding to skip for `ReferencesSkipDeclaration`
- `focus_range`: sorts definitions and finds a parameter’s owning callable
- `full_range`: checks whether a store occurrence falls inside the declaration syntax

Goto still converts definitions to `NavigationTargets`, but that conversion is now late and local to the goto path.

## Comparison with rust-analyzer

rust-analyzer centers find-usages around a semantic `Definition` enum. References/usages are found by resolving candidate syntax and comparing semantic definitions, then converting results to IDE/LSP-facing ranges.

The new ty approach is closer to that model:

- Semantic identity is the primary matching key.
- Navigation targets are not used as the identity representation.
- LSP-facing output is produced late.

The main difference is that ty cannot use only `ty_python_core::definition::Definition<'db>` as the identity, because Python import resolution can also produce:

- a module file
- an arbitrary file range, such as a preserved import alias range

That is why ty uses `ResolvedDefinition` as the semantic identity rather than plain `Definition<'db>`.

## Comparison with pyrefly

pyrefly’s LSP path is more definition/range-oriented than navigation-oriented. It has concepts like `FindDefinitionItem`, `DefinitionMetadata`, `definition_range`, and APIs named around `local_references_from_definition`.

The new ty approach is similar in that references start from definitions rather than navigation targets. The difference is that ty’s matching is more explicitly semantic:

- ty compares `ResolvedDefinition` identity sets
- pyrefly often uses definition ranges, metadata, and indexes keyed by module/name/range

So the new ty approach is closer to rust-analyzer for identity matching, while still keeping some pyrefly-like cached metadata for range-based behavior.

## Benefits

- Clearer ownership: goto owns navigation conversion; references owns reference matching.
- Better identity model: semantic definitions are compared directly instead of comparing editor jump locations.
- Less awkward state: `DefinitionTarget::is_origin` and reference-only data no longer live in the navigation layer.
- Better behavior for multi-definition symbols: overloads, properties, imports, and co-definitions are matched by set intersection over semantic identities.
- Easier future changes: if navigation behavior changes, reference identity should not change accidentally.
- More reviewable invariants: `DefinitionMatch` is now just `ResolvedDefinition` plus cached metadata, not a second identity enum.

## Downsides and tradeoffs

- There is still a wrapper, `DefinitionMatch`, because references need cached category/range metadata.
- `ResolvedDefinition` now derives `Copy` and `Hash`, which slightly broadens its API contract.
- The reference path still recomputes candidate definitions by walking AST candidates and resolving each one; this refactor improves representation, not search strategy.
- The matching uses set intersection rather than a single canonical identity. That is necessary for co-definitions, but it remains more complex than rust-analyzer’s common “one `Definition` has usages” path.
- Some range behavior remains reference-specific, especially declaration skipping and parameter keyword-argument handling.

## How to review

Review this in layers rather than as one large diff.

1. Check the representation split.

   In `crates/ty_ide/src/lib.rs`, `DefinitionTarget`/`DefinitionTargets` should be gone. Navigation-facing types should no longer carry reference-matching state.

2. Check goto conversion.

   In `crates/ty_ide/src/goto.rs`, `Definitions` should stay as raw `ResolvedDefinition`s until the goto path explicitly converts them to `NavigationTargets`. Constructor filtering and stub mapping should still happen before navigation conversion.

3. Check reference identity.

   In `crates/ty_ide/src/references.rs`, matching should compare `DefinitionMatch::resolved_definition()` values, not navigation targets or ranges.

4. Check cached metadata.

   Confirm that `DefinitionMatch` stores `ResolvedDefinition` as identity, and only caches `category`, `focus_range`, and `full_range` for reference-specific behavior.

5. Check declaration skipping.

   Focus on `DefinitionMatches::declarations` and `LocalReferencesFinder::is_declaration_occurrence`. These preserve the old behavior of skipping the semantic declaration when present, or the first binding otherwise.

6. Check cross-file parameter keyword references.

   `parameter_owner_is_externally_visible` now uses the cached `focus_range` instead of a navigation target. This should still identify the parameter node and its owning function.

7. Check semantic type changes.

   `ResolvedDefinition` now derives `Copy` and `Hash`. Confirm that all variants are stable identity values and that deriving these traits is appropriate.

8. Ignore unrelated dirty files.

   The existing mdtest change in `crates/ty_python_semantic/resources/mdtest/intersection_types.md` is unrelated to this refactor.

## Tests to trust

The most relevant tests are:

- `cargo check -p ty_ide`
- `cargo nextest run -p ty_ide find_references`
- `cargo nextest run -p ty_ide rename`
- `uvx prek run -a`

The find-references tests cover declaration inclusion/skipping, imports, overloads, string annotations, pattern bindings, attributes, and keyword arguments. Rename exercises the same semantic matching path with edit generation layered on top.
