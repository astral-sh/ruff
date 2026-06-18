# TypeVarTuple Explicit Specialization Design

## Goal

Add a reviewable TypeVarTuple foundation that recognizes and validates `TypeVarTuple` and
`Unpack`, constructs and displays variadic tuple types, and supports explicit specialization and
substitution. Do not infer a TypeVarTuple from call arguments, callable parameters, protocols,
overloads, or repeated pack occurrences.

## Representation

A TypeVarTuple occupies one generic-context slot. Its specialization is represented by one tuple
value:

- `C[()]` maps the pack to `tuple[()]`.
- `C[int, str]` maps the pack to `tuple[int, str]`.
- `C[*tuple[int, ...]]` maps the pack to `tuple[int, ...]`.
- An omitted pack without a default maps to `tuple[Unknown, ...]`.

When substituting the mapped value into `tuple[Prefix, *Ts, Suffix]`, splice the mapped tuple
between the fixed prefix and suffix. Never wrap it as a homogeneous tuple element.

## Supported behavior

- Legacy and PEP 695 TypeVarTuple definitions, including `typing_extensions.TypeVarTuple`.
- Definition, name, version, variance, bound, default, bare-use, and unpack validation.
- At most one TypeVarTuple per generic parameter list.
- Explicit specialization of classes and type aliases containing a TypeVarTuple.
- Empty, fixed, unbounded, prefixed, suffixed, and middle pack specializations.
- TypeVarTuple defaults and gradual fallback for omitted packs.
- Display, hover, go-to-definition, and safe malformed-code recovery.

Combinations with ParamSpec and callable aliases are deferred if supporting them requires callable
parameter inference or special matching behavior.

## Inference boundary

TypeVarTuple identities must not enter either generic constraint solver. Normal TypeVars in the
same signature may still be inferred. An unresolved TypeVarTuple is finalized as
`tuple[Unknown, ...]`, preserving fixed surrounding tuple structure after substitution.

The implementation must not aggregate `*args`, infer a pack from tuple arguments or callable
signatures, merge repeated inferred packs, or select overloads based on inferred pack length.

## Testing

Tests are written before implementation and divided into:

1. recognition and validation;
1. explicit specialization and tuple substitution;
1. defaults and gradual omitted packs;
1. display and recovery;
1. negative tests proving calls do not infer precise packs.

Previously passing typing-conformance tests must remain passing. Ecosystem changes must be limited
to recognition, validation, explicit specialization, defaults, or display; pack-inference-driven
diagnostic changes are not acceptable.
