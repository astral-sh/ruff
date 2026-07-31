# Definite invalid descriptor access diagnostics

## Summary

Add `invalid-attribute-access` diagnostics when an attribute read will definitely invoke an
invalid descriptor `__get__` method.

The implementation should reuse ty's existing member lookup and syntactic-place dataflow. It
should not introduce object-identity, alias, or class-namespace mutation analysis.

## In scope

- Diagnose invalid implicit `__get__` calls on class and instance attribute reads.
- Preserve the inferred return type from the attempted `__get__` call so one diagnostic does not
    cause cascading errors.
- Carry descriptor-call failures through normal member lookup and descriptor precedence.
- Expand union-like wrappers before invoking `__get__`:
    - PEP 695 type aliases
    - constrained `TypeVar`s
    - ordinary unions
- Invoke each union branch with its concrete descriptor type as `self`.
- Preserve the correlation between constrained receiver types and the descriptor selected from that
    receiver branch when validating the synthetic `instance` or `owner` argument, including for
    `type[T]` class-object receivers.
- Report the diagnostic only when the malformed descriptor is definitely selected and every
    applicable lookup branch fails.
- Suppress the diagnostic when a normal value, valid descriptor, class mutation, or other lookup
    alternative makes the failure merely possible.
- Preserve callable union and intersection structure when deciding whether an implicit `__get__`
    call definitely fails.
- Treat a definitely or possibly bound custom `__getattribute__` as an interceptor that makes
    normal descriptor invocation non-definite, even when calling the override itself fails.
- Treat a successful `__getattr__` fallback as a successful lookup alternative when the normal
    member is possibly undefined.
- Suppress descriptor-read diagnostics after any possibly or definitely reaching same-place
    assignment, without using descriptor kind to prove that `__get__` remains selected.
- Do not treat the inferred class-wide instance-member summary from arbitrary method assignments as
    proof that an assignment reaches a particular receiver. Only a live same-place assignment can
    establish that shadow, and deletion removes it.
- Treat the target of an augmented assignment as an attribute read before its write, while avoiding
    `__get__` diagnostics for deletion targets.
- Ignore intersection elements that do not contribute a member when combining descriptor failures.
- Ignore non-descriptor positive elements of an attribute intersection when combining descriptor
    failures; they refine one runtime value rather than supplying an alternative value.
- Update the `invalid-attribute-access` documentation with a descriptor-read example and
    regenerate references.
- Add focused tests for descriptor precedence, union-like wrappers, and conservative handling of
    class replacement.

## Diagnostic certainty

Descriptor lookup should distinguish:

- No failure: the access is valid.
- Possible failure: some lookup branches fail, but another branch can succeed.
- Definite failure: every applicable branch invokes an invalid `__get__`.

Only definite failures produce `invalid-attribute-access`.

For unions, failure certainty is combined across branches rather than selecting any available
error. A non-descriptor value or valid descriptor makes the overall descriptor failure
non-definite.

Callable types retain a union-of-intersections structure. Every union element must fail for the
descriptor call to be definitely invalid, while a callable intersection element succeeds if any of
its callable members accepts the arguments.

Only lookup paths that can supply the requested member participate in certainty aggregation. An
undefined intersection element is a refinement, not a successful alternative member lookup.
Conversely, a possibly defined member introduces an absent path; a successful dynamic fallback on
that path makes descriptor failure non-definite.

Positive elements of an intersection-valued attribute describe one runtime value. Elements without
`__get__` therefore do not create a successful alternative to a descriptor supplied by another
element.

Constrained receiver types are expanded only for descriptor-call validation, pairing each concrete
receiver constraint with the member selected from that constraint. For `type[T]`, ty first
transposes the constraints to their class-object types so each descriptor receives the corresponding
concrete class as its `owner`. The ordinary member result keeps the original receiver so existing
`Self` and TypeVar binding behavior is unchanged.

Same-place dataflow participates only as a conservative suppression boundary. Any possibly or
definitely reaching assignment suppresses the descriptor diagnostic, for both instance and class
objects. Ty does not distinguish a shadowing write from a data-descriptor `__set__` interception for
this diagnostic. This avoids requiring certainty about mixed descriptor kinds, conditionally
defined setters, or whether a metaclass descriptor intercepted a class-object assignment.

Class-wide summaries of assignments in arbitrary methods do not establish a live
instance-dictionary entry for a particular receiver. Explicit instance-attribute declarations
remain static contracts and continue to supply an alternative lookup path.

A custom `__getattribute__` runs before the normal descriptor algorithm. Whenever an override is
present, the descriptor call is not definite: a successful override can return first, while an
invalid override fails before Python reaches the descriptor. This certainty check does not replace
the ordinary member type with the override's return type when the normal member is already defined,
and it does not retarget a descriptor diagnostic to the invalid override.

Augmented assignment has a read phase and a write phase. Diagnosing a malformed descriptor during
the read phase is in scope. General augmented-assignment operator inference, write validation,
read/write correlation, and bidirectional type-context improvements are separate concerns.

## Out of scope

- Propagating `C.x = value` into later `C().x` lookup.
- Propagating class mutations into subclasses.
- Connecting mutations performed through aliases, such as `Alias.x`, back to `C.x`.
- General object-identity or heap dataflow.
- Tracking mutations across calls or modules.
- Proving that an instance assignment performed in another method or call reaches a later read.
- Diagnosing malformed descriptors after any reaching same-place assignment, including definite
    data descriptors and metaclass data descriptors that intercept class-object assignments.
- Distinguishing definite data descriptors from descriptors with conditionally defined `__set__` or
    `__delete__` methods for diagnostic certainty.
- Precisely diagnosing a malformed descriptor assigned dynamically through a class object.
- Improving the inferred value type after class-namespace mutation when the existing static class
    model cannot represent it.
- General augmented-assignment operator inference, store validation, and bidirectional type-context
    improvements.
- Diagnosing or retargeting call failures from an invalid custom `__getattribute__`.
- Fully modeling the return type and exceptions of custom `__getattribute__` implementations.
- General preservation of TypeVar correlation outside descriptor-call validation.

The class-mutation items require flow facts keyed by semantic object or class identity rather than
ty's existing syntactic places. That should be designed as a separate dataflow feature. The broader
augmented-assignment items belong to their existing operator and store-validation work instead.

## Accepted tradeoff

The implementation may miss invalid descriptor accesses after any reaching assignment. This
includes definite instance data descriptors and metaclass data descriptors whose `__set__` method
intercepts a class-object assignment. These conservative false negatives are preferable to an
error-level false positive on valid code.

The original issue remains covered because its malformed descriptor is statically known and
definitely selected.
