# Return-only attempt experiment

This implements the focused experiment in `/tmp/ty-constructor-attempt-feasibility-plan.md`, in a fresh copy of `/tmp/ty-prepared-source-salsa`. The original dependency and Ruff checkout remain unchanged. The baseline selected runtime tests pass: 48 tests covering source receipts, ordinary and nested cycles, tracked outputs, and borrowed query returns.

## Admission before execution

Add `QueryPolicy::{Unclassified, ReturnOnly, CompleteOnly}` to function configuration, defaulting to Unclassified. A small local copy of the Salsa macro packages accepts `attempt = ReturnOnly` or `attempt = CompleteOnly` and emits this constant; no registry source is edited.

An outer `try_with_attempt(db, allowance, body)` rejects a pre-existing tracked query or query-operation frame. Reentrant semantic work calls the ordinary query APIs under that attempt and shares its allowance. A future production wrapper must cover all external inference entry points, not only file checking.

Check policy before fetch, refresh/validation, and callbacks. Under an attempt, Unclassified operations are rejected before their body or cached result can be used. ReturnOnly may call ReturnOnly and CompleteOnly. CompleteOnly may call only CompleteOnly. The CompleteOnly barrier spans the whole lookup/validation operation, including a cold dependency reexecution and cycle-initial callbacks; checking only pushed query bodies would miss those paths.

ReturnOnly code is forbidden from directly creating tracked structs, specifying outputs, or accumulating values. Guard these APIs before their first mutation, regardless of whether exhaustion has occurred. Completed child CompleteOnly producers keep their normal output ownership. Interning immutable values remains allowed.

Violating these declared Rust query contracts is a checked programming error, consistent with existing Salsa invalid-query-contract errors. Such a panic is not resource recovery and has no retry guarantee. A legitimate exhausted allowance must return cooperatively without panic. Negative tests distinguish an unclassified producer rejected before its body, a mislabeled return-only body rejected before output creation, and a complete-only callback rejected before consuming the constructor allowance.

## Completion and retry

The attempt owns an identity token and one allowance. An admitted work checkpoint checks the complete-only barrier, then deducts its cost; refusal marks active return-only execution support as incomplete before returning a normal error to the constructor-shaped body. The outer API returns `AttemptOutcome::Incomplete` rather than the body's temporary recovery value.

Queries depending on an incomplete read retain attempt-specific support on their memos. Fresh attempts at the same revision cannot validate these memos merely because input revisions or values compare equal. Successful independent complete leaves remain reusable. Hot reads, cold/deep validation, backdating, and initial/provisional seed reuse all need this rule.

Provisional cycle results also record their attempt owner before any later refusal. Abandoning an attempt invalidates remaining provisional participants from that attempt. Initially it is acceptable to recompute a participant whose head settled but whose own finality was never verified; this is an explicit conservative reuse policy, not a reason to add another general cycle ownership framework. Verified-final unaffected memos remain ordinary.

An incomplete body exits the fixed-point loop before normalization or semantic convergence. The runtime releases its ordinary claims on normal return. A fresh attempt restarts affected cycles from their actual seeds. Replaced memo allocations retain their existing database-borrow lifetime; repeated retry storage is measured and not claimed to be reclaimed within the same revision.

## Executable gates

First implement and test policy admission through actual generated queries. Then connect incomplete memo support and cycles in the same experiment. Admission alone does not establish source integration.

Test cold/warm/partly warm execution, shared allowance through source callbacks, same-attempt incomplete-cache reads, two fresh successful retries in the unchanged database, productive cycles interrupted after provisional publication, relevant/unrelated edits, and a completed syntax-output child. Check execution counters and exact outcomes, including absence of false `Complete(0)` recovery.

The initial implementation is a single-threaded lifecycle proof. A production design still needs shared worker ownership and cross-attempt-cycle policy; neither is inferred from a passing toy. Once the connected lifecycle passes, wire the real cold constructor/getter inference path in the ty scratch checkout. The original eight constructor cases, infinite alternatives, performance, and production entry points remain the final acceptance criteria.

## Remove redundant admission work after measuring the first prototype

A paired microbenchmark of one million ordinary cached reads (20 retained alternating pairs, developer profile at optimization level 1) measured a 1.985 median ratio: 13.72 ms before versus 27.15 ms after. This is a narrow runtime benchmark, not a checker-level performance claim, and the overhead is too large to ignore.

Retain the same producer and single-worker contracts while eliminating duplicate registration. A public fetch owns one policy guard through refresh and read propagation; a private refresh implementation shares that guard. Other direct refresh callers still enter through the guarded wrapper. The database operation counter counts outermost operations for that database on each thread, while every nested operation still pushes/checks its policy. An already-counted outer operation prevents a concurrent attempt from starting; nested operations need no repeated atomic registration. Operations on a different database still register independently. Keep the reservation/counter handshake for new outer operations and rerun the deterministic worker and cycle/cache tests before measuring again.
