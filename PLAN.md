# Constraint-set TDD reduction plan

## Status markers and handoff rules

- `[ ]` not started
- `[~]` in progress
- `[x]` complete
- `[!]` blocked / needs a decision

Use this file as the ground truth for ordering, scope, and status. When resuming the work, first re-read this plan and then inspect the relevant code/tests to verify that the status markers still match reality. Update the markers and any learned details in the same revision as the work they describe.

Source-control expectations for future agents:

- Always create a new `jj` revision before editing (`jj new -A @` for normal feature work so downstream revisions include the changes).
- Describe each revision with a `[π]`-prefixed message.
- Keep revisions green. If a characterization test exposes behavior that should be fixed in a later focused revision, encode the current behavior as the passing expectation and add a clear TODO/xfail-style comment describing the intended behavior. Prefer that over ignored tests unless there is no practical alternative.
- Do not use `git` directly.

## Goal

Migrate `ty_python_semantic`'s constraint-set decision diagrams away from quasi-reduction and toward fully reduced TDDs, starting with a conservative set of sound, local reductions that do not require recursive `OR` computations during node construction.

Primary implementation target:

- `crates/ty_python_semantic/src/types/constraints.rs`

Related reference implementations / context:

- `crates/ty_python_core/src/narrowing_constraints.rs` has reduced TDD construction and cofactor absorption, but we do **not** plan to port its recursive/cofactor behavior in the initial reduction step.
- `crates/ty_python_core/src/reachability_constraints.rs` documents another reduced TDD variant.

## Decisions already made

- Constraint-set **display** semantics and ordering are not a constraint. Graph/display output may change freely.
- **Solved types must remain stable.** We do not want ordering churn in solved unions/intersections. `source_order` currently supports this by ordering constraints before solution extraction.
- `source_order` should remain part of node identity for this work. Removing/replacing it is a separate possible prerequisite/follow-up.
- Start with no-recursive-OR reductions only. Add a TODO near the reducer that a later step might handle `if_true == if_false` by returning `if_true ∪ if_uncertain`; that would compute the `OR` only after the rule has already engaged, not while deciding whether it applies.
- Testing and documentation are cross-cutting concerns. Each code revision should include the tests/comments/docs needed for that change, rather than deferring all tests/docs to a final cleanup phase.
- If preserving stable solved type ordering becomes awkward because of `source_order` interactions, raise that as a blocker. It may be better to first remove/replace `source_order` while retaining stable solved types, and then revisit reduction.

## Important semantics

Constraint-set TDD nodes use:

```text
n ? C : U : D = (n ∧ C) ∪ U ∪ (¬n ∧ D)
```

Therefore, the tempting rule `if_true == if_false -> if_uncertain` is **not** sound in general. It is equivalent to `if_true ∪ if_uncertain`.

Initial sound local reductions to implement/validate:

- `if_uncertain == ALWAYS_TRUE` -> `ALWAYS_TRUE`
- `if_true == if_false == if_uncertain` -> that shared node
- `if_true == if_false == ALWAYS_FALSE` -> `if_uncertain`
- `if_uncertain == ALWAYS_FALSE && if_true == if_false` -> `if_true`
- `if_true == if_uncertain && if_false == ALWAYS_FALSE` -> `if_uncertain`
- `if_false == if_uncertain && if_true == ALWAYS_FALSE` -> `if_uncertain`

The exact rule ordering can be chosen for clarity, but unit tests should cover the intended behavior.

## Current code areas to audit

Node construction and interning:

- `NodeId::with_uncertain`
- `NodeId::new`
- `Node::new_constraint`
- `Node::new_satisfied_constraint`
- `ConstraintSetBuilder::intern_interior_node`

Operations likely to be affected by reduced graph shapes:

- `InteriorNode::or`
- `InteriorNode::and`
- `InteriorNode::negate`
- `NodeId::ite_uncertain`
- `InteriorNode::restrict_one`
- `InteriorNode::abstract_one_inner`

Solving and order stability audit points:

- `PathBounds::compute` — sorts path constraints by `source_order` before building solution bounds.
- `NodeId::for_each_path` / `for_each_path_inner`
- `NodeId::for_each_unique_constraint`
- `PathAssignments`
- `SequentMap`
- `NodeId::satisfied_by_all_typevars`
- `NodeId::exists` / `exists_one`
- `NodeId::remove_noninferable`

Caller behavior to watch:

- `Solutions::Unconstrained` vs. `Solutions::Constrained(vec![...])` fallout.
- Callers in `crates/ty_python_semantic/src/types/generics.rs` and call binding/inference paths that use `solutions` / `solutions_with` / `PathBounds::solve`.

## Revision-sized phases

### [x] Phase 0 — Draft this implementation plan

Revision purpose: planning only.

Deliverables:

- `PLAN.md` with decisions, risks, and revision-sized phases.

### [x] Phase 1 — Add characterization tests for solved-type stability

Revision purpose: establish green guardrails before changing reduction behavior.

Testing focus:

- Compare solution results, not graph shape.
- Exercise equivalent constraint sets built with different BDD variable orderings while preserving the same logical/source order.
- Include cases where future reductions might drop redundant nodes.
- Cover both lower-bound union construction and upper-bound intersection construction in solved types.
- Include mutually constrained typevars / sequent-derived relationships, because these are sensitive to constraint ordering.

Completed details:

- Initially added implementation-level unit tests that compared rendered `Solutions` across different BDD pre-interning orders. PR review established that user-visible mdtests are preferable to tests coupled to constraint-set construction details, so those unit tests and their scaffolding were removed.
- Existing generic-function mdtests cover stable lower-bound union ordering and constrained-TypeVar relationships.
- Added a generic-function mdtest showing that upper-bound-only inference constructs intersections in call-site order.
- Deliberately dropped direct coverage of redundant uncertain wrappers and reversed BDD pre-interning; those are internal graph-shape details rather than user-visible behavior.

Validation:

- `CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- constraints`

### [x] Phase 2 — Route standalone node constructors through the central constructor

Revision purpose: preparatory refactor with little/no intended behavior change.

Implementation completed:

- Updated `Node::new_constraint` and `Node::new_satisfied_constraint` to call `NodeId::with_uncertain` instead of directly interning `InteriorNodeData`.
- Kept existing quasi-reduction semantics in `NodeId::with_uncertain`; no local reduction helper was introduced yet.
- No extra tests were needed: existing graph/semantic tests, plus the Phase 1 solved-type stability tests, cover the standalone constructor behavior and remained green.

Validation:

- `cargo fmt --check`
- `CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- constraints`

### [x] Phase 3 — Implement conservative local node reductions

Revision purpose: main behavior change for this feature slice.

Implementation completed:

- Centralized local reduction in `NodeId::with_uncertain` via `local_reduction(...) -> Option<NodeId>`.
- Applied only the sound local rules listed above.
- Performed reduction before computing `max_source_order` and before interning an `InteriorNodeData`.
- Kept `source_order` as part of identity for nodes that are still interned.
- Added a TODO near the remaining `if_true == if_false` case noting the possible follow-up reduction to return `if_true ∪ if_uncertain`.
- Updated comments that described constraint-set nodes as quasi-reduced.

Tests/docs:

- Rule-by-rule `NodeId::with_uncertain` unit tests were initially added, then removed during PR review because they only checked basic helper behavior and internal graph shapes.
- Existing graph-shape tests did not need expectation updates.
- User-visible solver effects are covered by generic-function and TypedDict mdtests.

Validation:

- `CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- constraints`

### [x] Phase 4 — Audit/fix solver and caller fallout in focused revisions

Revision purpose: handle behavioral fallout exposed by Phase 3.

Completed details:

- A full `ty_python_semantic` test run exposed changed constrained-TypeVar inference through TypedDict protocol inference after redundant TDD paths were removed.
- An initial response made `PathBounds::default_solve` choose a stable best match whenever multiple declared constraints were compatible. Although that improved several static ecosystem cases, PR review found that it arbitrarily narrowed gradual inputs and caused false positives in Hydpy.
- Kept best-match selection for fully static evidence. When a legacy TypeVar match selects a concrete declared constraint for the old hash map, `SpecializationBuilder` now stores the original evidence separately in its pending constraint set. The resulting `PathBound` therefore retains gradual lower/upper bounds just like constraint-set-native inference, and the solver uses one mechanism for both paths. Temporary `UnspecializedTypeVar` placeholders are not treated as gradual evidence.
- When gradual evidence is compatible with multiple constraints, the TypeVar remains unsolved instead of choosing an arbitrary concrete constraint. Static evidence for the same TypeVar still permits best-match selection because the accumulated bound becomes static.
- The review reproduction with an `Any` argument therefore continues to reveal `Unknown` instead of arbitrarily choosing `int`. Preserving `Any` would be preferable, but the current solver only uses the gradual bound to detect ambiguity; carrying it through as the constrained TypeVar's result is deferred to a separate change.
- Existing mdtests continue to cover exact constrained-TypeVar matches and TypeVar-to-TypeVar relationships. New sections cover static best-match selection, upper-bound-only selection, the direct gradual ambiguity fallback, and gradual evidence nested in a callable constraint-set-native path. The gradual cases include TODOs for eventual `Any` results.
- Removed the temporary `GenericContextSpecializationBuilder::common_typed_dict_protocol_constraints` throwaway-builder workaround.
- The reduced TDD still makes `get_value(ValueA | ValueB)` reveal `int` while rejecting `str`, without changing the general multiple-compatible-constraint fallback.
- A local Hydpy ecosystem comparison against `main` produced identical diagnostics (1,160 on each), confirming that the gradual-evidence fallback removes the review's false positives.

Validation:

- `CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- constraints`
- Focused `typed_dict.md` mdtest run for the rejected common-constraint probe case.
- `MDTEST_TEST_FILTER='Passing a constrained TypeVar' CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::generics/legacy/functions.md mdtest::generics/pep695/functions.md`
- `CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic`

### [ ] Phase 5 — Broaden order-independence coverage as issues are found

Revision purpose: strengthen coverage without tying assertions to graph shape.

This is not a final “test cleanup” phase; do this opportunistically as part of the focused revisions above. Track remaining useful cases here if they are discovered but deferred.

Candidate cases:

- Equivalent constraints constructed with reversed pre-interning order.
- Equivalent generic functions with type parameters declared in different orders.
- Tautologies such as `C ∪ ¬C` solving as unconstrained.
- `ALWAYS_TRUE ∪ C` solving as unconstrained.
- Contradictions such as `C ∧ ¬C` solving as unsatisfiable.
- Mutually constrained typevars where sequents infer transitive relationships.

## Blockers / escalation criteria

Raise this as blocked before proceeding if:

- Stable solved type ordering changes and cannot be fixed cleanly.
- Fixing stable solved ordering requires retaining fake `source_order`s for reduced-away nodes.
- We discover that solving genuinely requires typevar evidence from semantically redundant constraints and there is no small, principled solution-domain/provenance mechanism.
- The conservative local reductions are not enough to satisfy the intended feature semantics, and moving to cofactor reductions would require recursive `OR`s during node construction.

## Suggested validation commands

For focused Rust unit/mdtest work, prefer targeted runs first, then broaden:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- constraints
```

For a single mdtest file:

```sh
CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- mdtest::type_properties/implies_subtype_of.md
```

If `cargo nextest` is unavailable, use the fallback `cargo test` commands from `AGENTS.md`. After snapshot-updating runs, inspect any changed snapshots or `.pending-snap` files.
