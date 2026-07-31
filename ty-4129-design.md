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
    - `TypeVar`s with union-like upper bounds
    - ordinary unions
- Expand recursive type aliases behind a cycle-aware query so recursion terminates conservatively.
- Invoke each union branch with its concrete descriptor type as `self`.
- Preserve the correlation between TypeVar receiver alternatives and the descriptor selected from
    that receiver branch when validating the synthetic `instance` or `owner` argument, including for
    `type[T]` class-object receivers and constrained or union-bounded `super()` owners.
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
- Preserve a possible valid metaclass data-descriptor branch as a higher-precedence lookup
    alternative when a mixed descriptor-kind union otherwise falls through to a class attribute.
- Treat a conditionally defined `__set__` or `__delete__` method as a possible, rather than definite,
    data-descriptor path when determining diagnostic certainty.
- Treat the absent branch of a possibly defined `super` member as an alternative that does not
    invoke the descriptor, while retaining the ordinary possibly-missing diagnostic.
- Preserve descriptor-call failures while expanding narrowed enum complements into their remaining
    literal alternatives.
- Suppress descriptor-read diagnostics after any possibly or definitely reaching same-place
    assignment, without using descriptor kind to prove that `__get__` remains selected.
- Treat an inferred class-wide instance-member summary as a possible instance-dictionary shadow for
    diagnostic certainty, without treating it as proof that an assignment reaches a particular
    receiver.
- Treat the target of an augmented assignment as an attribute read before its write, while avoiding
    `__get__` diagnostics for deletion targets.
- Ignore intersection elements that do not contribute a member when combining descriptor failures.
- Ignore non-descriptor positive elements of an attribute intersection when combining descriptor
    failures; they refine one runtime value rather than supplying an alternative value.
- Treat an attribute intersection as a data descriptor when any positive element is definitely a
    data descriptor, because every inhabitant retains that element's descriptor behavior.
- Treat a TypeVar-valued attribute as a definite data descriptor when its upper bound or every
    constraint is definitely a data descriptor.
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
Conversely, a possibly defined member introduces an absent path that does not invoke the descriptor,
even if that path ultimately raises an attribute error instead of finding a dynamic fallback. A
possibly-missing diagnostic remains separate from the descriptor-call diagnostic.

Positive elements of an intersection-valued attribute describe one runtime value. Elements without
`__get__` therefore do not create a successful alternative to a descriptor supplied by another
element. The same refinement rule determines descriptor precedence: if any positive element is
definitely a data descriptor, the intersection is a data descriptor and takes precedence over a
lower-priority class or instance attribute.

Descriptor precedence can differ across the elements of a union-valued metaclass member. A possible
data-descriptor element remains a higher-precedence alternative even when the aggregate attribute
kind says that the union is not uniformly data-descriptor-like. The diagnostic preserves that
successful alternative conservatively without changing the inferred member type.

The same rule applies when descriptor kind depends on a conditionally defined `__set__` or
`__delete__` method. Ty keeps its existing inferred member type, but combines the invalid `__get__`
call with the lower-precedence fallback before deciding that the failure is definite.

Constrained receiver types and receivers with union-like upper bounds are expanded only for
descriptor-call validation, pairing each concrete receiver alternative with the member selected from
that alternative. For `type[T]`, ty first transposes the alternatives to their class-object types so
each descriptor receives the corresponding concrete class as its `owner`. The ordinary member result
keeps the original receiver so existing `Self` and TypeVar binding behavior is unchanged.

Constrained or union-bounded `super()` owners retain the original type variable in the inferred
`super` type, but descriptor-call validation uses the concrete alternative associated with each
already-expanded `super` branch. This includes alternatives, such as enum literals, whose ordinary
`super` construction delegates to a wider fallback type. The validation owner participates in
`super` equivalence so union simplification cannot discard distinct validation branches or make a
diagnostic depend on alternative order.

Recursive alias expansion runs inside the cycle-aware descriptor-call query. A recursive branch that
re-enters the same query contributes no definite descriptor failure, while the remaining concrete
branches continue to determine the inferred return type and diagnostic certainty.

Same-place dataflow participates only as a conservative suppression boundary. Any possibly or
definitely reaching assignment suppresses the descriptor diagnostic, for both instance and class
objects. Ty does not distinguish a shadowing write from a data-descriptor `__set__` interception for
this diagnostic. This avoids requiring certainty about mixed descriptor kinds, conditionally
defined setters, or whether a metaclass descriptor intercepted a class-object assignment.

Class-wide summaries of inferred instance attributes do not establish a live instance-dictionary
entry for a particular receiver, but they do represent a possible successful fallback. That
possibility suppresses an error-level descriptor diagnostic. Ty does not distinguish an assignment
performed by `__init__` from one in an unrelated method, or remove the class-wide summary after a
same-place deletion.

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
- Introducing a general tri-state descriptor-kind representation or changing inferred member types
    for descriptors with conditionally defined `__set__` or `__delete__` methods.
- Precisely diagnosing a malformed descriptor assigned dynamically through a class object.
- Improving the inferred value type after class-namespace mutation when the existing static class
    model cannot represent it.
- Improving the inferred member type or general precedence representation for metaclass attributes
    with mixed data-descriptor and non-data-descriptor alternatives.
- General resolution of incompatible descriptor method definitions contributed by multiple
    positive intersection elements.
- General changes to `super` MRO lookup or possibly-missing attribute inference.
- General changes to enum-complement narrowing or inferred member types.
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

The implementation may also suppress a diagnostic when every branch of a mixed metaclass
descriptor-kind union fails through a different precedence path. Correlating each metaclass branch
with the corresponding class-attribute fallback, and refining the inferred member type accordingly,
is outside this change.

The implementation may suppress a diagnostic because of an inferred instance attribute whose
assignment does not actually reach the receiver, including an assignment in an unrelated method or
one removed by a later deletion. Distinguishing those cases from constructor-established attributes
would require the out-of-scope object-identity and interprocedural dataflow described above.

The original issue remains covered because its malformed descriptor is statically known and
definitely selected.
