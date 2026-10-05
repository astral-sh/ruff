# Unfinished experiment: bounded, interruptible type checking

This branch preserves a stopped research experiment in ty and Salsa, its incremental computation engine. It is an unfinished research record, not a proposed production implementation. Development ended on 2026-10-05 with known correctness failures, resource-limit failures, incomplete accounting, and unresolved performance and lifecycle questions. The final source was preserved without trying to make those failures pass.

The experiment began with constructor-to-callable conversion and grew into an attempt to make real type checking interruptible under explicit resource limits. A small cold file check completed through an experimental library entry point, including cancellation, cleanup, and retry in the same database revision. That is a useful demonstration of the architecture, but it does not establish that the architecture is practical, complete, or safe for arbitrary Python programs.

## What is preserved

The first checkpoint commit on this branch preserves the constructor and recursion work that was still in the original worktree. The following commit brings in the newer standalone experimental source, including the modified Salsa implementation under [`vendor/salsa`](../vendor/salsa). [`source-manifest.json`](bounded-typechecking/source-manifest.json) records the 1,601 files copied from that standalone source, their hashes, and their sizes. They include vendored dependency files as well as original experimental changes.

The final installed change is in [`field_run.rs`](../vendor/salsa/src/function/execute/execution_run/field_run.rs). It separates field requests that cannot record dependencies from requests that can. Both Salsa and ty's library-entry test target compiled with that change. Its new accounting constants were still provisional, and its planned runtime tests and new layout measurements had not run. The [installed patch](bounded-typechecking/installed-field-dependency.patch) identifies that exact change. An [unfinished accounting patch](bounded-typechecking/unapplied-field-accounting.patch), prepared outside the source tree, is preserved separately and **is not applied**. It is neither a validated fix nor a recommended next step.

Selected results and failure records are retained in [`bounded-typechecking/evidence`](bounded-typechecking/evidence). Some raw records contain absolute paths to the original local experiment and references to larger archives that are not included here. The linked source, copied result records, and explanations below are available in the branch; compiler caches, binaries, and the full collection of temporary measurements are not. Missing historical evidence is identified below rather than reconstructed.

## The original problem

Converting a class to a `Callable` requires finding the parameters accepted by construction, applying descriptor binding, and determining the constructed result. Direct construction and callback conversion had partly separate implementations and assumptions about which `Type` variants could represent constructor methods.

For example, the initializer below is a callable object. Its `__call__` accepts the integer directly; looking up `Product.__init__` does not turn that object into an ordinary function that receives an additional `Product` instance.

```python
from typing import Callable


class Initialize:
    def __call__(self, value: int) -> None: ...


class Product:
    __init__ = Initialize()


factory: Callable[[int], Product] = Product
```

Generalizing this conversion exposed recursive interactions among constructor lookup, descriptors, callable conversion, generic specialization, and type relations. A lookup can require another callable conversion, which can require another lookup. Generic arguments can change on every visit, so a cache keyed by the full type need never see the same key twice. Separately, a very long but finite chain can overflow the Rust stack, and a finite branching computation can take excessive time without containing a cycle.

The shared constructor lookup lives in [`types/constructor.rs`](../crates/ty_python_semantic/src/types/constructor.rs) and [`constructor/member_resolution.rs`](../crates/ty_python_semantic/src/types/constructor/member_resolution.rs). Callable conversion is implemented through [`callable/evaluation.rs`](../crates/ty_python_semantic/src/types/callable/evaluation.rs) and [`constructor/callable.rs`](../crates/ty_python_semantic/src/types/constructor/callable.rs). The [legacy](../crates/ty_python_semantic/resources/mdtest/generics/legacy/callables.md) and [PEP 695](../crates/ty_python_semantic/resources/mdtest/generics/pep695/callables.md) callable specifications preserve many of the motivating examples.

## Approaches tried and what they exposed

### Centralized lookup and structural recursion guards

The early work shared constructor member resolution between direct calls and callable conversion, then tried to distinguish recursive expansion from finite changes in generic arguments. Exact-key detection handles a repeated state. It does not handle a sequence such as `Grow[int]`, `Grow[list[int]]`, and `Grow[list[list[int]]]`, where the key changes forever.

The structural guard uses *homeomorphic embedding*: roughly, an earlier stored type structure can be found inside a later structure by removing wrappers or ordered children. This recognizes increasing nesting and increasing argument counts. The implementation and examples are in `TypeEmbedding` and `CallableGrowthDetector` in [`cyclic.rs`](../crates/ty_python_semantic/src/types/cyclic.rs).

Embedding is evidence of structural growth, not proof that evaluating a particular program will never terminate. It also depends on how identities, leaves, and children are represented. Treating an argument permutation or a changing but ultimately finite specialization as infinite growth can reject valid work. Even a suitable growth test has its own traversal, comparison, and storage costs. A recursion guard alone therefore did not supply a general bound on checking time, stack use, or memory.

### Shared algorithms with explicit effects

The experiment then separated semantic decisions from operations such as reading a query, looking up a member, allocating a collection, or constructing a diagnostic. Here an *effect* is one of those operations supplied by an adapter. Ordinary adapters perform normal ty operations; controlled adapters request the same operation through the experimental runtime.

This let ordinary and controlled execution share substantial typing logic instead of maintaining a simplified second checker. Representative boundaries are [`constructor/effects.rs`](../crates/ty_python_semantic/src/types/constructor/effects.rs), [`signatures/effects.rs`](../crates/ty_python_semantic/src/types/signatures/effects.rs), and [`builder/source_definition/controlled`](../crates/ty_python_semantic/src/types/infer/builder/source_definition/controlled).

The difficulty was the size of the dependency closure. A seemingly small constructor example can require type-variable defaults, class bases, method resolution order, descriptor calls, annotations, relation checking, and diagnostic construction. Implementing one adapter repeatedly exposed another ordinary operation underneath it. Leaving such an operation unrestricted would undermine the bound; reporting it as unavailable would prevent the example from finishing. The resulting migration is broad and remains incomplete.

### An explicit runtime around canonical Salsa queries

A *canonical query* is the ordinary Salsa query identified by its existing query kind and key. Its *memo* is Salsa's cached result and dependency information. The experiment aimed to retain those identities and results while changing how an evaluation is driven and interrupted.

A *continuation* is the saved state needed to resume a computation; Rust futures provide many of these states here. The runtime owns a depth-first collection of tasks instead of giving each provider uncontrolled recursive ownership of its children. An *attempt* is one budgeted root evaluation. A failed attempt must not turn an unfinished parent computation into a reusable complete memo, although independently completed children may remain usable.

That required changes inside Salsa, not just a wrapper around `check_file`. The branch modifies query entry, dependency delivery, memo reuse, fixed-point/cycle execution, generated field access, and cancellation ownership. See [`attempt_probe.rs`](../vendor/salsa/src/attempt_probe.rs), [`function/execute.rs`](../vendor/salsa/src/function/execute.rs), and [`execution_run.rs`](../vendor/salsa/src/function/execute/execution_run.rs).

### Resource accounting and representation costs

An *admission* checks and spends an allowance before an operation proceeds. A *quote* estimates the work or requested bytes to admit. The final selected policy keeps two independent cumulative limits: **1,000,000 logical work units and 16,777,216 requested bytes**. Accepted charges are not refunded when storage is released.

Logical work counts semantic and control operations. Requested bytes conservatively account for storage and fixed representation construction/copying, including inline values and saved futures. These bytes are not live heap usage, resident memory, or measured allocator traffic. Work units are not elapsed time or CPU instructions. The two counters must be interpreted together. Structural source preparation and allocator internals remain outside the semantic execution limits; this is not a bound on the entire process.

Earlier accounting coupled work to representation width. Separating the counters made their meanings clearer, but did not remove the underlying costs. Audits then found missing costs in quotation preparation, argument/result transfers, collection growth, hashing, and cleanup. Correcting an underestimate could make an example refuse earlier without changing its typing algorithm. Conversely, a smaller quoted total would not itself demonstrate a physical performance improvement.

The shared transfer helpers in [`local_transfer.rs`](../crates/ty_python_semantic/src/types/local_transfer.rs), its [collection accounting](../crates/ty_python_semantic/src/types/local_transfer/collections.rs), and [default-guard accounting](../crates/ty_python_semantic/src/types/infer/builder/source_definition/controlled/typevar_default/cost.rs) illustrate both the mechanism and the substantial complexity it introduced.

## Final experimental architecture

The public API is behind `experimental-analysis`. Enabling that feature does not automatically route ordinary callers through it. The existing `check_file` entry retains its ordinary execution mode; shared semantic refactors and the vendored Salsa changes still affect the codebase more broadly.

The intended execution sequence is:

1. **Prepare structural inputs.** [`analysis::prepare_file`](../crates/ty_python_semantic/src/analysis.rs) obtains the applied environment, parses and indexes the file, and prepares suppression and host data. It does not execute semantic inference. A `PreparedAnalysisFile` retains that state and the database's identity, revision, and cancellation stamp.
1. **Start one bounded root.** `check_file_with_policy` or `expression_type_with_policy` creates an analysis session with the two cumulative limits. Results distinguish completion, resource exhaustion, unavailable controlled operations, and contract/preparation errors. Native Salsa cancellation retains its cancellation/unwinding behavior.
1. **Dispatch registered semantic operations.** [`source_runtime.rs`](../crates/ty_python_semantic/src/types/infer/source_runtime.rs) connects real source queries and their controlled providers to [`RegistryBuilder` and `TaskEndpoint`](../vendor/salsa/src/function/execute/execution_run/registration.rs). Unsupported dependencies produce an explicit incomplete outcome; they are not replaced by approximate successful results.
1. **Read and publish canonical state.** Query fetch and dependency delivery use the existing memo identities. [`PreparedSourceMemo`](../vendor/salsa/src/function/memo/prepared_source.rs) certifies completed structural state. Later reads still check its stamp and memo allocation identity; a same-revision replacement is not automatically equivalent. [`read_run.rs`](../vendor/salsa/src/function/execute/execution_run/read_run.rs) records dependencies before results reach consumers. Structural preparation needed later in a run has a separate [controlled preparation boundary](../vendor/salsa/src/function/execute/execution_run/structural_preparation.rs).
1. **Retain owners until their children are finished.** The driver owns queued requests. Callback captures, query claims, active guards, and borrowed source owners must survive until dependent children are drained. [`callback.rs`](../vendor/salsa/src/function/execute/execution_run/callback.rs) and the driver's completion/drainage paths enforce parts of that ordering. A callback's failure cannot safely destroy storage still borrowed by a queued child.
1. **Return completion or stop without publishing a partial answer.** File-level incompletion contains no partial diagnostic list. Expression-level incompletion contains no candidate type. Completed child memos may survive, allowing a fresh attempt with the same prepared file and database revision. This requires more than clearing an error flag: claims, operation stacks, guards, and retained storage must be in a valid state for the retry.

The final field-access change is a narrower representation experiment within this design. Input and tracked fields can carry dependency metadata, while interned fields do not create dependency edges. Previously their common optional representation made interned reads retain a state for recording a dependency they could never have. The installed candidate carries that distinction through a sealed associated type (`NoFieldDependency` or `OptionalFieldDependency`) and separate recording futures. Before the change, three measured interned getter futures were each 616 bytes. The [layout record](bounded-typechecking/evidence/generic-field-layout1-verified.json) establishes the old sizes, not any reduction in the final candidate.

## What was demonstrated

The most useful complete example uses the experimental **library** API with a real `ProjectDatabase` and the ordinary typing and diagnostic algorithms:

```python
def choose(value):
    return value


choose(choose(True))
```

The [library-entry tests](../crates/ty_python_semantic/tests/experimental_analysis_entry.rs) also use positional and keyword calls. They begin without warming the semantic root, prepare structural inputs, complete the file check with no diagnostics, compare against ordinary checking, and repeat within the same revision. Separate cases interrupt after a completed scope merge and refuse work or bytes at both immediate and later limits, then verify cleanup and a successful same-revision retry. This demonstrates a complete path through the architecture, not general Python coverage or default CLI adoption.

The following evidence predates the final field-access candidate unless explicitly stated otherwise:

| Evidence                                                                                                                                                                                                                  | Recorded result                                                                                                           | What it does not establish                                                           |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| [Three library lifecycle tests](bounded-typechecking/evidence/dynamic-guard-and-field-baseline-results1.json)                                                                                                             | Cold completion, cancellation/retry, and work/byte refusal/retry passed.                                                  | Arbitrary files, physical memory bounds, or runtime behavior of the final candidate. |
| [Eight Salsa field-access controls](bounded-typechecking/evidence/dynamic-guard-and-field-baseline-results1.json)                                                                                                         | Ordinary metadata, canonical dependency edges, interned borrows, refusal, cancellation, child drainage, and retry passed. | That the subsequently changed field-access implementation retains those results.     |
| [Focused semantic comparison](bounded-typechecking/evidence/dynamic-guard-preparation-focused1-comparison.json)                                                                                                           | 169 selected cases: 131 passed and 38 failed.                                                                             | A green suite; failures are retained in full.                                        |
| Two ownership controls, with [selection](bounded-typechecking/evidence/dynamic-guard-preparation-controls1-command.json) and [exit status](bounded-typechecking/evidence/dynamic-guard-preparation-controls1-status.json) | Both passed: refusal retains factory captures; pending constructor fallback drains children before retry.                 | Physical retirement of every allocation or arbitrary-depth destruction.              |
| Final candidate [Salsa compiler result](bounded-typechecking/evidence/typed-field-dependency-compile1-status.json) and [ty compiler result](bounded-typechecking/evidence/typed-field-dependency-ty-compile1-status.json) | Both no-run builds exited successfully.                                                                                   | Runtime tests, complete accounting, new layout sizes, or CI success.                 |

The recorded compiler invocations used the local `ohm-1.98.1-5` toolchain and existing offline dependency caches. Their logs preserve warnings as well as results. They should not be read as a portability claim for another toolchain or host.

## Unresolved failures and missing assurances

### Useful finite programs still exceed the limits

The most recent focused inventory contains **38 selected failures plus seven retained failures outside that selection**. The [failure inventory](bounded-typechecking/evidence/typed-field-dependency-known-failures.json) preserves their names, payloads, and panic locations. A newly recorded partial-default-specialization failure exhausts requested bytes before reaching its cleanup and canonical-result assertions; those later assertions were not demonstrated by that run.

Both legacy and PEP 695 public constructor-forwarding examples still refused with `RequestedAllocationLimit` on cold execution and retry. Their [comparison record](bounded-typechecking/evidence/forwarding-public-file-validation17-comparison.json) retains cleanup/revision observations, but does not establish completed diagnostics or that execution reached the intended callable consumer.

An earlier broader selection recorded **1,108 passes and nine failures out of 1,117 cases**. The failing cases included property-related resource failures, star-import byte exhaustion, and an unavailable `CheckerConstructor` operation. The failing legacy/PEP 695 callable cases collectively missed 32 expected diagnostics; that diagnostic count is distinct from the number of failing cases. The [failure payloads](bounded-typechecking/evidence/constructor-self-receiver-semantic16-failures.json) remain evidence of these problems. These selections overlap and were taken at different source states; their counts must not be added to manufacture a single current-suite total.

### Accounting is not a complete safety proof

The final field-access candidate has an incomplete accounting audit. Before shutdown, that audit identified missing provisional terms for endpoint argument transfers and resource-value construction. The audit also had not established complete prior charges for constructing, returning, and awaiting the nested dependency and transaction futures. These operations transfer argument/result representations even without a heap allocation. Another unresolved boundary is how the admission machinery's own preparation and request-value construction are covered before its first successful charge. If such operations remain outside the accounting, the reported totals omit work or bytes already consumed before a refusal, so the limits do not establish the intended bound. The final candidate's successful compilation discharges none of these issues.

More broadly, complete bounds for native hashing/equality, interner operations, all allocation paths, quotation preparation, and cleanup remain unfinished. Refusal can leave valid logical state without establishing when all underlying storage is physically released. Stack safety for arbitrarily deep polling, cloning, dropping, formatting, and equality was not established. Concurrency, fixed-point completion, and warm-memo equivalence still need stronger evidence.

### Historical performance and lifecycle problems remain open

The [acceptance-obligation index](bounded-typechecking/evidence/acceptance-obligation-evidence-index.json) retains an 8-by-8 disjunctive-normal-form case: a branching logical representation that can expand during type-constraint processing. Its passing control expects `WorkLimit`; that is successful detection of exhaustion, not successful completion. A recorded 1,496,000-unit figure covers merging alone, not an entire root evaluation.

The same index records eight historical `LEAK` test-runner events across six fixtures. That label describes the runner's output-handle/process classification; it is not proof of a Rust heap leak. Their original causes and physical-retirement consequences were not resolved. Later runs without that label do not explain them.

For an earlier finite 4,098-entry conversion workload, the exact original fixtures, command line, byte policy, and complete raw receipts were not recovered. Its historical two-million-work threshold also differs from the final one-million limit. The available secondary account is insufficient for a reliable before/after performance claim.

Other outstanding cases include recursive base normalization for `tuple(().missing)`, invalid `ParamSpec` defaults, and coverage for omitted or unreachable code, `no_type_check`, and enclosing diagnostic spans. The [acceptance-obligation index](bounded-typechecking/evidence/acceptance-obligation-evidence-index.json) also retains the remaining cost-comparison and interruption/retirement scenarios. Full tests, Clippy, cross-platform checks, and complete repository hooks were not established for the final source. Known failures are intentionally preserved in this record.

## Lessons from the experiment

1. **Termination, stack safety, and acceptable cost are separate problems.** Recognizing repeated or growing types does not bound a long finite computation, a branching relation, or the work required by the guard itself. A general design must state which of these it promises.
1. **Interruption is a query-engine concern as well as a typing concern.** A budget around a recursive function is insufficient when cached values, tracked outputs, dependency edges, and child requests outlive that function. Completion and ownership rules must be designed together.
1. **One complete cold example is more informative than many isolated adapters.** The library milestone forced real source preparation, canonical queries, diagnostics, cancellation, cleanup, and retry to meet at one boundary. It also exposed how far that success remained from routine constructor examples.
1. **Share semantic algorithms, but assess the cost of the abstraction.** Shared effects reduced opportunities for semantic drift, while futures, wrappers, result carriers, and repeated admissions introduced substantial complexity and quoted byte costs. The large unfinished migration is evidence that this balance was not resolved.
1. **A budget model must be explicit about its units and exclusions.** Cumulative requested bytes are not live memory; logical work is not time. Correcting an estimate, reducing executed operations, and speeding up the checker are three different claims requiring different evidence.
1. **Preserve uncertainty at the point of measurement.** A test that expects refusal, an assertion never reached after an earlier panic, a compile-only result, or a restored logical guard cannot be promoted into evidence of completion, behavior preservation, or physical cleanup.

There are reusable ideas here—shared constructor resolution, explicit incomplete outcomes, canonical memo ownership, prepared structural reads, and lifecycle tests—but this experiment did not establish that its complete architecture is the right production solution. The branch is preserved so those ideas and failures can be examined without treating unfinished work as accepted design.
