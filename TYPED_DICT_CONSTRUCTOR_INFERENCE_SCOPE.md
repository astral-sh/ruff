# Generic `TypedDict` Constructor Inference

This document defines the initial scope for
[ty issue #4134](https://github.com/astral-sh/ty/issues/4134). The goal is to
infer useful specializations for bare generic `TypedDict` constructors without
introducing unsound results or expanding the change into general mapping-flow
analysis.

## Supported inference

### Direct constructor arguments

Inference should support both PEP 695 and legacy generic declarations:

```py
from typing import Generic, TypeVar, TypedDict

class Box[T](TypedDict):
    value: T

reveal_type(Box(value=1))  # Box[int]

T = TypeVar("T")

class LegacyBox(TypedDict, Generic[T]):
    value: T

reveal_type(LegacyBox(value=1))  # LegacyBox[int]

class ListBox[T](TypedDict):
    value: list[T]

reveal_type(ListBox(value=[1]))  # ListBox[int]
```

Direct inference also applies to fields inherited through generic `TypedDict`
bases. The inferred arguments must be applied to the generic context of the
class being constructed before resolving its full field schema, so inherited
fields and the returned specialization agree on the meaning of each type
variable.

An expected specialization can provide the constructor specialization when
every exact field value is valid under that context:

```py
class Animal: ...

class Dog(Animal): ...

target: Box[Animal] = Box(value=Dog())
```

`Box` is invariant because its fields are mutable, so the argument-only
specialization `Box[Dog]` is not assignable to `Box[Animal]`. In this case,
constructor inference should use the valid contextual specialization. If
neither an inferred specialization nor the expected specialization can be
validated against the exact field values, the constructor retains its
preexisting unknown specialization and diagnostic behavior.

An alias or union may wrap the expected specialization. Contextual inference
may select it only when structural alias expansion and union inspection find
exactly one distinct specialization of the class being constructed. `None` and
unrelated union arms do not make that specialization ambiguous. If multiple
matching specializations remain, contextual inference retains the preexisting
unknown specialization instead of choosing one arbitrarily.

Type parameters with defaults retain their preexisting default specialization
and validation behavior. Inferring a different specialization from constructor
arguments would require validating the call against that inferred
specialization, which is outside the initial scope.

### Safe flat literal forms

If ty retains its existing support for mapping-style `TypedDict`
construction, inference may also support flat literals whose relevant keys and
values are statically known:

```py
reveal_type(Box({"value": 1}))  # Box[int]
reveal_type(Box(**{"value": 1}))  # Box[int]
reveal_type(Box({"value": 1}, value="x"))  # Box[str]
reveal_type(ListBox({"value": [1]}))  # ListBox[int]
reveal_type(ListBox(**{"value": [1]}))  # ListBox[int]
```

These forms are eligible only when ty can prove the exact source for each
type-variable-bearing field. Nested dictionary unpacking, opaque mappings,
optional keys, extra items, and uncertain duplicate keys are not safe flat
literal forms. A direct keyword deterministically replaces the corresponding
entry from a positional flat literal, so it remains exact evidence rather than
an uncertain duplicate. Exact field evidence uses the value expression's type
without the unresolved `TypedDict` field as context, so equivalent direct and
flat-literal forms infer the same candidate specialization. After selecting a
candidate, ty must infer every field expression again under that
specialization's declared field type and run ordinary constructor validation
before returning the specialized `TypedDict`. Context-sensitive expressions
such as lambdas cannot be validated using only their context-free inferred
types.

## Conservative fallbacks

Inference is best-effort. If an argument can conditionally or opaquely
overwrite a type-variable-bearing field, the constructor should retain its
unknown specialization instead of using incomplete field evidence.

```py
from typing import NotRequired

class MaybeString(TypedDict):
    value: NotRequired[str]

def optional_overwrite(maybe: MaybeString) -> None:
    reveal_type(Box(**{"value": 1, **maybe}))  # Box[Unknown]

def opaque_overwrite(values: dict[str, str]) -> None:
    reveal_type(Box(**{"value": 1, **values}))  # Box[Unknown]
```

In particular, earlier evidence such as `value: int` must not survive after a
later source may replace that value. This initial scope does not attempt to
infer `int | str` or otherwise model every possible mapping state.

Fallback is transitive through nested generic `TypedDict` values. If a field
value has gradual content anywhere in the specialization of a generic
`TypedDict` because its own constructor inference was skipped, an enclosing
generic `TypedDict` constructor must also retain its unknown specialization.
This check applies recursively to specialization arguments, unions, and
containers, but type aliases are an opaque boundary in the initial scope. Any
alias encountered in constructor evidence makes the enclosing constructor
gradual, even when expanding that alias could prove a precise specialization.
This conservative rule prevents aliases from hiding gradual evidence and makes
recursive aliases terminate without inspecting their values. The traversal
does not inspect class or protocol members, type-variable bounds, or other
attributes that are not part of the field type itself.

When this fallback is driven by the call expression's expected type, only an
unknown specialization of the class currently being constructed is relevant.
An unrelated bare generic `TypedDict` in another union arm must not suppress
exact constructor inference. This differs from inspecting field values: a
gradual nested generic `TypedDict` of any class in an actual field value still
propagates gradual evidence to the enclosing constructor.

## Nested and recursive construction

Nested and recursive generic `TypedDict` construction must remain
diagnostic-free:

```py
class Node[T](TypedDict):
    value: NotRequired[T]
    child: NotRequired["Node[T]"]

reveal_type(Node(child=Node(value=1)))  # Node[Unknown]
```

Inferring `Node[int]` would require propagating constraints through the
invariant nested `Node[T]` field and validating against the resulting
specialization. That is outside the initial scope. Falling back to
`Node[Unknown]` is acceptable; reporting that `Node[int]` is not assignable to
`Node[Unknown]` is not.

The same fallback applies when another field could otherwise constrain the
outer specialization:

```py
class Inner[T](TypedDict):
    value: T

class Outer[T](TypedDict):
    inner: Inner[T]
    marker: T

reveal_type(Outer(inner=Inner(value=1), marker="x"))  # Outer[Unknown]
```

## Bounds and constraints

Literal promotion must preserve every declared type-variable bound and
constraint. An inferred literal should be promoted when the promoted type is
still a valid specialization, including for broad bounds:

```py
class IntBound[T: int](TypedDict):
    value: T

reveal_type(IntBound(value=1))  # IntBound[int]
```

When promotion would violate a narrow bound, inference retains the bound-aware
literal solution:

```py
from typing import Callable, Literal

class Bound[T: Literal[1]](TypedDict):
    value: T

reveal_type(Bound(value=1))  # Bound[Literal[1]]

class Both[T](TypedDict):
    value: T
    callback: Callable[[T], None]

def accepts_one(value: Literal[1]) -> None: ...

reveal_type(Both(value=1, callback=accepts_one))  # Both[Literal[1]]

target: Box[Literal[1]] = Box(value=1)
```

The validity check must use the bound or constraints specialized with the other
inferred type arguments. If promotion cannot be proven valid, inference must
retain the bound-aware solution or fall back to the unknown specialization.
Satisfying the declared bounds is necessary but not sufficient: every
constructor argument must remain assignable to its field under the promoted
specialization. If revalidation fails, inference retains the valid unpromoted
specialization. The promoted return type must also remain assignable to the
call expression's expected type; contextual inference must not be widened away.
If the argument-inferred specialization is also incompatible with the expected
type, inference uses the expected specialization when the constructor arguments
remain valid under it, and otherwise falls back to the unknown specialization.

## Implementation constraints

- Constructor inference must not emit diagnostics of its own. Existing
    `TypedDict` constructor validation remains responsible for diagnostics.
- An inferred specialization should replace the ordinary result only when the
    relevant type variables are solved from exact field evidence.
- Gradual evidence anywhere in a nested generic `TypedDict` specialization,
    including through structural union and container wrappers, makes enclosing
    constructor inference gradual. Type aliases are treated as opaque gradual
    evidence. This traversal must not inspect unrelated class or protocol
    members or type-variable metadata.
- Constructor validation and the returned type must use the same
    specialization. Context-free field types are only candidate-inference
    evidence; field expressions must be re-inferred and validated with
    diagnostics enabled under the selected specialization before it is exposed.
- Inferred arguments must be remapped from the synthetic callable's fresh type
    variables to the source class's generic context before resolving the
    `TypedDict` schema, including fields inherited from generic bases.
- A compatible contextual specialization takes precedence when an invariant
    argument-inferred specialization would not satisfy the call expression's
    expected type. It also takes precedence when a context-free candidate is
    compatible only through gradual content, so context-sensitive expressions
    are validated under the precise expected field type.
- Contextual selection expands aliases and inspects unions, but succeeds only
    when exactly one distinct arm specializes the class being constructed.
    Unrelated arms are ignored and multiple matching specializations are
    ambiguous.
- Post-inference literal promotion should occur exactly when the promoted type
    satisfies the specialized type-variable bounds and constraints and all
    constructor arguments and the call expression's expected type remain valid
    under the promoted specialization.
- Exact field expressions must be inferred without the unresolved `TypedDict`
    field context when gathering candidate evidence so supported direct and
    flat-literal forms behave equivalently.
- A direct keyword replaces evidence from the same key in a positional flat
    literal; uncertain duplicate sources still make inference gradual.
- If exact inference is not possible, the call should preserve the preexisting
    unknown specialization and diagnostic behavior.
- Support for PEP 695 and legacy generic syntax must remain equivalent.

## Non-goals

The initial implementation does not attempt to:

- model ordered overwrites across nested dictionary displays;
- merge optional or opaque overwrite types into unions;
- infer through nested or recursive generic `TypedDict` fields;
- infer constructor specializations that override type-parameter defaults;
- expand ty's accepted `TypedDict` constructor forms; or
- define general inference behavior for arbitrary mappings.
