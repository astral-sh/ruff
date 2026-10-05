# Return-only analysis attempt: runtime experiment

The isolated runtime's latest combined run passes 101 checks. The connected constructor harness passes 11 source-level tests, including the eight original forwarding cases, finite chains through depth 32, infinite alternatives, refusal during a productive cycle and its normalizer, and unchanged-database retries. These are scratch experiments, not evidence that the live Ruff branch is fixed.

## Executed evidence

The admission-only implementation passed seven boundary tests but failed the new same-revision retry check: a fresh sufficient attempt returned `Complete(0)` from the previous refusal instead of `Complete(7)`. `/tmp/ty-attempt-retry-negative.log` records that negative control.

With incomplete support connected to query reads, cache validation and cycles, `/tmp/ty-attempt-runtime-optimized.log` records the earlier passing combined run using the `persistence` feature, after the policy-scan and support-lookup refinements:

- 25 attempt tests cover cold and warm admission, complete syntax-output children, same-attempt incomplete reads, two independent refusal/retry rounds in the unchanged database, source callbacks sharing one allowance, deep validation of equal fallback values, interrupted productive and nested cycles, refusal during initial/recovery callbacks, reuse of independent complete results, explicit interruption reasons, tracked-method macro forwarding, and single-worker admission.
- 48 existing controls cover ordinary cycles, nested convergence, dependency-order changes, prepared-source observations, tracked output ownership and borrowed query values.
- 19 library tests cover the available memory-usage and persistence configurations alongside existing storage behavior.

The exact combined command was `cargo test --offline --locked --features persistence --test attempt_probe --test prepared_source_probe --test cycle --test cycle_nested_converge_early --test cycle_dependency_order_different_entry_queries --test tracked_struct --test tracked_fn_return_ref --lib`.

A separate all-features run was not completed: optional dependencies were not cached, and downloading them hit the Cargo-cache filesystem restriction. The persistence and unstable-memory features used by the focused validation compile and pass. No all-features claim is made.

## Ordinary-operation scope validation

The additive `try_with_operation` API passes 101 runtime checks in `/tmp/ty-attempt-runtime-scoped.log`: 33 attempt tests, 48 existing controls, and 20 library tests. The new checks cover nested same/different database scopes, concurrent attempt rejection between queries, rejection of an unjoined worker scope, unwind cleanup, query-policy enforcement through scopes, shared allowance and same-revision retry, and borrowed values. One internal check observes a single shared registration across repeated and nested query operations. The full command and feature selection are unchanged from the combined run above.

Scopes are separate from query-policy frames and retain per-query fallback admission for unscoped callers. They do not implement concurrent incomplete attempts. Matched checker measurements found no convincing benefit from scopes; `/tmp/ty-attempt-checker-performance.md` records the comparison. The source harness has been rerun against this runtime, without installing ordinary scopes.

## What the storage tests establish

An outstanding borrow from an incomplete memo remains valid after replacement. Twenty unsuccessful attempts at the same revision retain twenty superseded values, and the next normal revision releases all twenty. The final complete value remains until database destruction. Retained storage grows with retries; automatic unbounded retry would therefore be inappropriate.

The shared attempt-token heap allocation is not yet included in Salsa's aggregate memory accounting. Existing extra-metadata size is counted, but this is not a complete byte accounting of retry storage. Add token accounting or a separate retained-token measurement before making a production memory claim.

## Remaining acceptance gates

The admission contract is single-worker only. Concurrent attempts and workers accessing a database during an attempt are rejected before query execution; this is not a production parallel-analysis design. Operation guards use shared atomic counters even during ordinary execution. A paired cached-query microbenchmark initially measured a 1.985 median ratio. Removing duplicate admission and nested atomic registration reduced that to 1.526, approximately 7.4 ns extra per trivial hot query. This remains a concern; it is not a whole-checker performance measurement. Evidence and proposed conservative follow-ups are in `/tmp/ty-attempt-runtime-cost-audit.md`.

The real cold constructor/getter path and original finite scenarios pass in the experimental driver. The most recent full semantic suite, before the later observation tests and seven additional query classifications, passes 1,250 tests with 34 skipped; the two generic-callable fixture files continue to fail on the original forwarding cases outside the installed experiment. `/tmp/ty-attempt-full-semantic-optimized.log` records this run. The settings callback in the Markdown-test database was audited and classified before that run; the first run's 481 admission failures were caused by its missing classification. No new or pending snapshots were generated in that run.

The productive-source-cycle test observes the actual loop-carried definition changing, then interrupts a constructor debit after the cycle iterates. It rejects subsequent affected normalization/finalization, repeats the demand within that incomplete attempt, and verifies fresh canonical seeding on the first unchanged-database retry. The second retry uses the completed answer without constructor expansions.

The separate normalizer test now reaches an absent comparison override inside `infer_expression_types_impl` at iteration 5. With allowance 402 it refuses the exact `FalseFactory` constructor's driver-entry charge inside that callback, performs no later affected widening/finalization or accepted work, and drains all scopes. The unchanged-database retry reexecutes the affected query and completes the normalizer; a second warm retry performs no constructor expansions. Control/retry/warm expansion counts are 175/166/0. This establishes normalizer-origin refusal for this source path, not every possible normalization operation.

The infinite matrix covers growing `__new__` and `__init__`, separate direct and callback demands, both generic syntaxes, and finite/growing union alternatives in both orders. At allowances 128/256/512, every attempt and unchanged-revision retry reports `Incomplete::Allowance`, including after a finite alternative has actually completed. Direct bindings and callable signatures have separate completion observations. Finite counterpart controls preserve the exact diagnostic IDs, locations, and union multiplicities. `/tmp/ty-attempt-source-expanded-controls.log` records all 11 passing source tests, including the previous seven controls and the expanded normalizer test. Observed stack spans remain constant across these allowances for each fixture, but do not establish a complete native-stack bound.

Ordinary checker performance remains unresolved. The scoped/optimized-unscoped paired measurements show no convincing scope benefit, and baseline comparisons retain a consistent unchanged-check regression; cold/edit measurements are noisier. `/tmp/ty-attempt-checker-performance.md` records the exact data. `/tmp/ty-attempt-fetch-cost-design.md` proposes an unimplemented attribution experiment. Full generated-work/stack bounds, retained-token accounting, production entry points, parallel workers, and overlapping requests remain outstanding. The governing architecture is `/tmp/ty-constructor-attempt-feasibility-plan.md` and `IMPLEMENTATION_PLAN.md`.

## Ordinary frame reuse experiment

A same-database/same-policy ordinary frame reuse fast path passed106 focused tests but did not improve the matched checker workloads and regressed warm checks by ~6.8%. It was removed; the runtime operation code and manifest were restored from the pre-experiment archive. The two new independent policy/reentrant-callback integration controls remain. All103 tests pass in `/tmp/ty-attempt-runtime-restored-with-callback-controls.log` (20library,35attempt,48existing controls). `/tmp/ty-attempt-checker-performance.md` and `/tmp/ty-attempt-checker-frame-reuse.json` retain the measurements. This does not resolve the underlying ordinary-query overhead.

The real-source scratch now passes12 source controls in `/tmp/ty-attempt-generated-search-final-controls.log`. Passive measurements confirm uncharged recursive Self searches during growing initializer forwarding; `/tmp/ty-attempt-interruptible-search-plan.md` specifies the next repair before implementation. No broader work or native-stack guarantee is established by these passing tests.
