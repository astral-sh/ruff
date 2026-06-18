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

- Added unit tests in `constraints.rs` that render `Solutions` and compare solved-type results, avoiding graph-shape assertions.
- Added lower-bound union coverage with `int` then `str` source order and reversed BDD variable order from pre-interning.
- Added upper-bound intersection coverage with `Iterable` then `Awaitable` source order and reversed BDD variable order from pre-interning.
- Added a redundant uncertain-wrapper test (`n ? never : U-lower-str : never`) to verify that solved types are unaffected when a future local reduction drops the wrapper node.
- Added mutually constrained typevar coverage for `T = U` and `U ≤ int`, with reversed pre-interning to exercise typevar/constraint ordering and sequent-derived relationships.

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

- Added unit tests covering each local reduction rule.
- Existing graph-shape tests did not need expectation updates.
- Re-ran the Phase 1 solved-type stability tests as part of the targeted `constraints` run.

Validation:

- `CARGO_PROFILE_DEV_OPT_LEVEL=1 INSTA_FORCE_PASS=1 INSTA_UPDATE=always CARGO_PROFILE_DEV_DEBUG="line-tables-only" MDTEST_UPDATE_SNAPSHOTS=1 cargo nextest run -p ty_python_semantic -- constraints`

### [x] Phase 4 — Audit/fix solver and caller fallout in focused revisions

Revision purpose: handle behavioral fallout exposed by Phase 3.

Completed details:

- A full `ty_python_semantic` test run exposed one solver fallout through TypedDict protocol inference: reduced TDDs no longer retained a redundant path that had let constrained-TypeVar solving pick a compatible concrete constraint for `get_value(ValueA | ValueB)`.
- Fixed this in `PathBounds::default_solve` instead of sandboxing callers. When a concrete path satisfies multiple declared constraints for a constrained TypeVar, the solver now picks the first compatible declared constraint in the TypeVar's constraint order rather than depending on semantically redundant BDD paths to provide one exact constraint.
- Preserved TypeVar-to-TypeVar relationships: if the bounds for such an ambiguous constrained-TypeVar path still mention another TypeVar, the solver leaves the specialization unresolved instead of replacing the relationship with an arbitrary concrete constraint.
- Added focused unit tests for ambiguous constrained-TypeVar solving, including stability across TypeVar constraint order.
- Removed the temporary `GenericContextSpecializationBuilder::common_typed_dict_protocol_constraints` throwaway-builder workaround; `generics.rs` has no remaining change for this feature.
- Updated the `typed_dict.md` expectation for `get_value(ValueA | ValueB)` to reveal `int` while still rejecting `str`, matching the first-compatible constrained-TypeVar choice.

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
