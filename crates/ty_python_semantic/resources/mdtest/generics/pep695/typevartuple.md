# PEP 695 `TypeVarTuple`

```toml
[environment]
python-version = "3.12"
```

## Definition

A PEP 695 type variable tuple is introduced with a single starred type parameter.

```py
def foo[*Ts](*args: *Ts) -> None:
    reveal_type(Ts)  # revealed: TypeVarTuple
    reveal_type(args)  # revealed: tuple[*Ts@foo]
```

## Variance inference

PEP 695 type variable tuples infer variance from how the class uses them.

```py
class CovariantArray[*Ts]:
    def get(self) -> tuple[*Ts]:
        raise NotImplementedError

covariant_ok: CovariantArray[object] = CovariantArray[int]()
covariant_error: CovariantArray[int] = CovariantArray[object]()  # error: [invalid-assignment]

class ContravariantArray[*Ts]:
    def set(self, value: tuple[*Ts]) -> None:
        raise NotImplementedError

contravariant_ok: ContravariantArray[int] = ContravariantArray[object]()
contravariant_error: ContravariantArray[object] = ContravariantArray[int]()  # error: [invalid-assignment]

class InvariantArray[*Ts]:
    values: tuple[*Ts]

invariant_out: InvariantArray[object] = InvariantArray[int]()  # error: [invalid-assignment]
invariant_in: InvariantArray[int] = InvariantArray[object]()  # error: [invalid-assignment]
```

## Generic Classes

### Explicit specialization

```py
class Simple[*Ts]:
    attr: tuple[*Ts]

reveal_type(Simple[()]().attr)  # revealed: tuple[()]
reveal_type(Simple[int, str]().attr)  # revealed: tuple[int, str]
reveal_type(Simple[*tuple[int, str]]().attr)  # revealed: tuple[int, str]

# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
reveal_type(Simple[[int, str]]().attr)  # revealed: tuple[Unknown]
# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
reveal_type(Simple[*[int, str]]().attr)  # revealed: tuple[Unknown, ...]
```

```py
class Prefix[T, *Ts]:
    attr: tuple[T, *Ts]

reveal_type(Prefix[int]().attr)  # revealed: tuple[int]
reveal_type(Prefix[int, bool]().attr)  # revealed: tuple[int, bool]
reveal_type(Prefix[int, bool, str]().attr)  # revealed: tuple[int, bool, str]
reveal_type(Prefix[int, *tuple[bool, str]]().attr)  # revealed: tuple[int, bool, str]

# TODO: Should this raise an error?
reveal_type(Prefix().attr)  # revealed: tuple[Unknown, *tuple[Unknown, ...]]
```

```py
class Suffix[*Ts, T]:
    attr: tuple[*Ts, T]

reveal_type(Suffix[int]().attr)  # revealed: tuple[int]
reveal_type(Suffix[int, str]().attr)  # revealed: tuple[int, str]
reveal_type(Suffix[int, str, bool]().attr)  # revealed: tuple[int, str, bool]
reveal_type(Suffix[*tuple[int, str], bool]().attr)  # revealed: tuple[int, str, bool]

# TODO: Should this raise an error?
reveal_type(Suffix().attr)  # revealed: tuple[*tuple[Unknown, ...], Unknown]
```

```py
class Between[T, *Ts, U]:
    attr: tuple[T, *Ts, U]

reveal_type(Between[int, str]().attr)  # revealed: tuple[int, str]
reveal_type(Between[int, bool, str]().attr)  # revealed: tuple[int, bool, str]
reveal_type(Between[int, bool, bytes, str]().attr)  # revealed: tuple[int, bool, bytes, str]
reveal_type(Between[int, *tuple[bool], str]().attr)  # revealed: tuple[int, bool, str]

reveal_type(Between().attr)  # revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
# error: [invalid-type-arguments] "No type argument provided for required type variable `U` of class `Between`"
reveal_type(Between[int]().attr)  # revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
```

### Inherited specializations containing `Never`

A `Never` argument in a variadic generic must retain its position when a subclass forwards its type
arguments to a generic base.

```py
from typing import Any, Never

class Kind[*Ts]: ...
class SupportsKind[*Ts](Kind[*Ts]): ...
class Container(SupportsKind[int, Never]): ...

def _(value: Container) -> None:
    expected: Kind[int, Any] = value
```

### `TypeVarTuple` with `ParamSpec`

```py
from typing import Callable

class TypeVarTupleWithParamSpec[*Ts, **P]:
    fn: Callable[P, tuple[*Ts]]

reveal_type(TypeVarTupleWithParamSpec[[str, int]]().fn)  # revealed: (str, int, /) -> tuple[()]
reveal_type(TypeVarTupleWithParamSpec[int, [str, int]]().fn)  # revealed: (str, int, /) -> tuple[int]
reveal_type(TypeVarTupleWithParamSpec[int, str, [str, int]]().fn)  # revealed: (str, int, /) -> tuple[int, str]

# error: [invalid-type-arguments]
reveal_type(TypeVarTupleWithParamSpec[str, int]().fn)  # revealed: (...) -> tuple[str]

reveal_type(TypeVarTupleWithParamSpec[str, int, []]().fn)  # revealed: () -> tuple[str, int]
reveal_type(TypeVarTupleWithParamSpec[str, int, ...]().fn)  # revealed: (...) -> tuple[str, int]
```

### Inferred specialization from construction

Calling a generic class without explicit type arguments infers its specialization from the
constructor arguments.

```py
class Positional[*Ts]:
    def __init__(self, shape: tuple[*Ts]) -> None:
        self.shape = shape

class Variadic[*Ts]:
    def __init__(self, *shape: *Ts) -> None:
        self.shape = shape

reveal_type(Positional(()))  # revealed: Positional[()]
reveal_type(Positional((1, "a")))  # revealed: Positional[int, str]

reveal_type(Variadic())  # revealed: Variadic[()]
reveal_type(Variadic(1, "a"))  # revealed: Variadic[int, str]

def _(i: int, s: str) -> None:
    reveal_type(Positional((i, s)))  # revealed: Positional[int, str]
    reveal_type(Variadic(i, s))  # revealed: Variadic[int, str]
```

Constructor arguments determine the class specialization even when the assignment expects a
different specialization.

```py
valid: Variadic[int] = Variadic(1)

inferred = Variadic(1)
reveal_type(inferred)  # revealed: Variadic[int]
# error: [invalid-assignment]
indirect: Variadic[str] = inferred
# error: [invalid-assignment]
direct: Variadic[str] = Variadic(1)
```

Concrete contexts do not supply missing arguments or discard extra ones.

```py
# error: [invalid-assignment]
missing_argument: Variadic[int] = reveal_type(Variadic())  # revealed: Variadic[()]

# error: [invalid-assignment]
extra_argument: Variadic[int] = reveal_type(Variadic(1, "a"))  # revealed: Variadic[int, str]
```

A contextual specialization cannot supply missing constructor arguments. Empty calls infer an empty
pack, and one argument cannot satisfy an arbitrary outer pack, even when it matches a required
suffix. This applies with or without a fixed suffix.

```py
def empty_with_context[*Us](shape: tuple[*Us]) -> Variadic[*Us, int]:
    # error: [invalid-return-type]
    return reveal_type(Variadic())  # revealed: Variadic[()]

def nonempty_with_context[*Us](shape: tuple[*Us]) -> Variadic[*Us, int]:
    # error: [invalid-return-type]
    return reveal_type(Variadic(1))  # revealed: Variadic[int]

def empty_without_suffix[*Us](shape: tuple[*Us]) -> Variadic[*Us]:
    # error: [invalid-return-type]
    return reveal_type(Variadic())  # revealed: Variadic[()]
```

Forwarding the outer pack supplies the required arguments. A compatible context can still widen
their element types without changing the pack's shape.

```py
widened: Variadic[object] = Variadic(1)

def forward_without_suffix[*Us](shape: tuple[*Us]) -> Variadic[*Us]:
    return Variadic(*shape)

def forward_with_suffix[*Us](shape: tuple[*Us]) -> Variadic[*Us, int]:
    return Variadic(*shape, 1)

def widen_suffix[*Us](shape: tuple[*Us]) -> Variadic[*Us, object]:
    return Variadic(*shape, 1)
```

An unpacked `tuple[Any, ...]` can match any length, so a compatible context can specialize it. The
same gradual behavior applies when a tuple is passed as one element of the pack.

```py
from typing import Any

def gradual_arguments(values: tuple[Any, ...]) -> None:
    concrete: Variadic[int, str] = reveal_type(Variadic(*values))  # revealed: Variadic[int, str]
    nested: Variadic[object, tuple[int]] = reveal_type(Variadic(1, values))  # revealed: Variadic[object, tuple[int]]

def gradual_with_context[*Us](shape: tuple[*Us], values: tuple[Any, ...]) -> Variadic[*Us, int]:
    return Variadic(*values)

def gradual_boundaries[*Us](
    shape: tuple[*Us],
    prefix: tuple[int, *tuple[Any, ...]],
    suffix: tuple[*tuple[Any, ...], str],
) -> None:
    first: Variadic[int, *Us, str] = Variadic(*prefix)
    last: Variadic[int, *Us, str] = Variadic(*suffix)
```

Aliases of `Any` preserve gradual length when the context supplies a concrete specialization.

```py
type Dynamic = Any

def gradual_alias_arguments(values: tuple[Dynamic, ...]) -> None:
    concrete: Variadic[int, str] = reveal_type(Variadic(*values))  # revealed: Variadic[int, str]
```

Fixed elements still constrain the pack's length and types, even when other elements are gradual.

```py
def fixed_any_with_context[*Us](shape: tuple[*Us], values: tuple[Any]) -> Variadic[*Us, int]:
    fixed_length: Variadic[int] = reveal_type(Variadic(*values))  # revealed: Variadic[int]
    # error: [invalid-return-type]
    return reveal_type(Variadic(*values))  # revealed: Variadic[Any]

def incompatible_gradual_prefix[*Us](shape: tuple[*Us], values: tuple[int, *tuple[Any, ...]]) -> Variadic[*Us, int]:
    return Variadic(*values)  # error: [invalid-return-type]

def incompatible_gradual_element(values: tuple[bytes, *tuple[Any, ...]]) -> Variadic[int, str]:
    return Variadic(*values)  # error: [invalid-return-type]

def too_many_gradual_boundaries(values: tuple[int, *tuple[Any, ...], str]) -> Variadic[int]:
    return Variadic(*values)  # error: [invalid-return-type]
```

### Unspecified type arguments

An unsubscripted variadic generic behaves as if it used an unknown-length tuple of `Any` arguments.
ty represents the missing type information as `Unknown`, distinguishing it from explicitly provided
`Any`.

```py
class Unspecified[*Ts]:
    attr: tuple[*Ts]

unspecified = Unspecified()
reveal_type(unspecified)  # revealed: Unspecified[*tuple[Unknown, ...]]
reveal_type(unspecified.attr)  # revealed: tuple[Unknown, ...]
```

### Default type arguments

A defaulted type variable tuple supplies its unpacked tuple when the generic class is not explicitly
specialized. Explicit type arguments override the default.

```toml
[environment]
python-version = "3.13"
```

```py
class WithDefault[*Ts = *tuple[int, str]]:
    attr: tuple[*Ts]

reveal_type(WithDefault().attr)  # revealed: tuple[int, str]
reveal_type(WithDefault[bool, bytes]().attr)  # revealed: tuple[bool, bytes]
```

### Gradual specializations

A type variable tuple remains assignable to an explicitly gradual specialization of its generic
class.

```py
from typing import Any

class Array[*Ts]:
    def erase_shape(self) -> "Array[*tuple[Any, ...]]":
        return self
```

### Constrained inference from synthetic `Self`

A fixed synthetic `Self` domain provides evidence for inferring a fresh constrained type variable,
without making the owner's type variables inference targets. A constraint with gradual tuple
arguments can accept a `TypeVarTuple` specialization.

```py
from typing import Any, Generic, TypeVar

class Other: ...

class Container[T, *Ts]:
    values: tuple[T, *Ts]

    def interface(self) -> "Interface[Container[Any, *tuple[Any, ...]]]":
        return Interface(self)

C = TypeVar(
    "C",
    Container[Any, *tuple[Any, ...]],
    Other,
    covariant=True,
)

class Interface(Generic[C]):
    def __init__(self, value: C) -> None: ...
```

## Functions

### Multiple type variable tuples

Generic functions can declare multiple type variable tuples because their type parameters are
inferred from arguments; functions cannot be explicitly specialized. Separate tuple arguments infer
their type variable tuples independently.

```py
def pair[*Ts, *Us](
    first: tuple[*Ts],
    second: tuple[*Us],
) -> tuple[tuple[*Ts], tuple[*Us]]:
    return first, second

def check_pair(first: int, second: str, third: bool, fourth: bytes) -> None:
    reveal_type(pair((first, second), (third, fourth)))  # revealed: tuple[tuple[int, str], tuple[bool, bytes]]
```

A variadic parameter can also infer one type variable tuple from a fixed nested tuple and another
from its remaining arguments.

```py
def nested[*Ts, *Us](
    *args: *tuple[tuple[*Us], *Ts],
) -> tuple[tuple[*Us], tuple[*Ts]]:
    raise NotImplementedError

def check_nested(first: int, second: str, third: bool, fourth: bytes) -> None:
    reveal_type(nested((first, second), third, fourth))  # revealed: tuple[tuple[int, str], tuple[bool, bytes]]
```

### Tuple arguments and returns

```py
def simple[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def with_prefix[T, *Ts](x: T, y: tuple[*Ts]) -> tuple[T, *Ts]:
    raise NotImplementedError

def with_suffix[*Ts, U](x: tuple[*Ts], y: U) -> tuple[*Ts, U]:
    raise NotImplementedError

def both[T, *Ts, U](x: T, y: tuple[*Ts], z: U) -> tuple[T, *Ts, U]:
    raise NotImplementedError

def f(i: int, s: str, b: bool, t: tuple[int, str], vt: tuple[int, ...]) -> None:
    reveal_type(simple(()))  # revealed: tuple[()]
    reveal_type(simple((i, s)))  # revealed: tuple[int, str]
    reveal_type(simple(t))  # revealed: tuple[int, str]
    reveal_type(simple(vt))  # revealed: tuple[int, ...]

    reveal_type(with_prefix(i, (s, b)))  # revealed: tuple[int, str, bool]
    reveal_type(with_prefix(i, t))  # revealed: tuple[int, int, str]
    reveal_type(with_prefix(i, vt))  # revealed: tuple[int, *tuple[int, ...]]
    reveal_type(with_prefix(t, vt))  # revealed: tuple[tuple[int, str], *tuple[int, ...]]

    reveal_type(with_suffix((i, s), b))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix(t, b))  # revealed: tuple[int, str, bool]
    reveal_type(with_suffix(vt, b))  # revealed: tuple[*tuple[int, ...], bool]
    reveal_type(with_suffix(vt, t))  # revealed: tuple[*tuple[int, ...], tuple[int, str]]

    reveal_type(both(i, (i, s), b))  # revealed: tuple[int, int, str, bool]
    reveal_type(both(i, t, b))  # revealed: tuple[int, int, str, bool]
    reveal_type(both(i, vt, b))  # revealed: tuple[int, *tuple[int, ...], bool]

    # TODO: Avoid also reporting an invalid argument type for the first unpacked element.
    # error: [invalid-argument-type] "Argument to function `simple` is incorrect: Expected `tuple[Unknown, ...]`, found `int`"
    # error: [too-many-positional-arguments] "Too many positional arguments to function `simple`: expected 1, got 2"
    reveal_type(simple(*t))  # revealed: tuple[Unknown, ...]
```

A gradual tuple also infers a gradual pack when the parameter allows `None`.

```py
from typing import Any

def optional[*Ts](value: tuple[*Ts] | None) -> tuple[*Ts] | None:
    return value

def check_optional(value: tuple[Any, ...]) -> None:
    reveal_type(optional(value))  # revealed: tuple[Any, ...] | None
```

### Assignability to fixed-length tuples

An unspecialized type variable tuple can contain any number of elements, so a tuple containing one
cannot be assigned to a fixed-length tuple, even when its fixed prefix and suffix match.

```py
def arbitrary_pack[*Ts](value: tuple[*Ts]) -> tuple[int, str]:
    return value  # error: [invalid-return-type]

def middle_pack[*Ts](value: tuple[int, *Ts, str]) -> tuple[int, str]:
    return value  # error: [invalid-return-type]
```

### Assignability involving type variable tuples

A symbolic type variable tuple can be erased to a homogeneous `object` tuple, but a fully static
homogeneous tuple cannot be used to construct an arbitrary symbolic pack. Two independently bound
packs are also not interchangeable.

```py
def erase_pack[*Ts](values: tuple[*Ts]) -> tuple[object, ...]:
    return values

def preserve_pack[*Ts](values: tuple[*Ts]) -> tuple[*Ts]:
    return values

def preserve_pack_with_boundaries[*Ts](values: tuple[int, *Ts, str]) -> tuple[object, *Ts, object]:
    return values

def reject_object_pack[*Ts](values: tuple[object, ...], witness: tuple[*Ts]) -> tuple[*Ts]:
    return values  # error: [invalid-return-type]

def reject_int_pack[*Ts](values: tuple[int, ...], witness: tuple[*Ts]) -> tuple[*Ts]:
    return values  # error: [invalid-return-type]

class Outer[*Ts]:
    def reject_unrelated_pack[*Us](self, values: tuple[*Ts]) -> tuple[*Us]:
        return values  # error: [invalid-return-type]
```

A fixed-length tuple cannot replace an arbitrary type variable tuple either. The caller determines
the pack's length and element types, so even an empty tuple is not a valid return for every pack.
The same restriction applies to annotated assignments inside the function.

```py
def reject_empty[*Ts](values: tuple[*Ts]) -> tuple[*Ts]:
    return ()  # error: [invalid-return-type]

def reject_fixed[*Ts](values: tuple[*Ts]) -> tuple[*Ts]:
    return (1, "a")  # error: [invalid-return-type]

def reject_fixed_assignment[*Ts]() -> None:
    fixed: tuple[*Ts] = (1,)  # error: [invalid-assignment]
```

Matching fixed elements before or after a pack do not establish what the pack contains. The
remaining elements still cannot replace an arbitrary type variable tuple.

```py
def reject_empty_middle[*Ts](values: tuple[*Ts]) -> tuple[int, *Ts, str]:
    return (1, "a")  # error: [invalid-return-type]

def reject_fixed_middle[*Ts](values: tuple[*Ts]) -> tuple[int, *Ts, str]:
    return (1, True, "a")  # error: [invalid-return-type]
```

Materializing a type variable tuple can change its default without changing the identity of the
bound type variable occurrence.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Any
from ty_extensions import Top, static_assert
from ty_extensions._internal import is_assignable_to

def materialized_default[*Ts = *tuple[Any, ...]]() -> None:
    static_assert(is_assignable_to(tuple[*Ts], Top[tuple[*Ts]]))
```

Fixed-length tuples are not subtypes of an arbitrary pack either. An `Any` or `Never` element does
not change a fixed tuple's length.

```py
from typing import Never
from ty_extensions._internal import is_subtype_of

def fixed_tuple_relations[*Ts]() -> None:
    static_assert(not is_subtype_of(tuple[()], tuple[*Ts]))
    static_assert(not is_subtype_of(tuple[int], tuple[*Ts]))
    static_assert(not is_assignable_to(tuple[Any], tuple[*Ts]))
    static_assert(not is_assignable_to(tuple[Never], tuple[*Ts]))
```

### Gradual tuple assignability to symbolic packs

A fully gradual tuple can materialize to any specialization of a type variable tuple, including
fixed elements around the pack. This permits assignment, but does not make it a subtype of the
symbolic tuple.

```py
from typing import Any
from ty_extensions import static_assert
from ty_extensions._internal import Unknown, is_subtype_of

def gradual_packs[*Ts](dynamic: tuple[Any, ...], unknown: tuple[Unknown, ...]) -> None:
    plain: tuple[*Ts] = dynamic
    plain = unknown
    bounded: tuple[int, *Ts, str] = unknown
    static_assert(not is_subtype_of(tuple[Unknown, ...], tuple[*Ts]))
```

A gradual tuple with a fixed prefix or suffix cannot be assigned to a bare symbolic pack, which may
be empty. This remains true when the required element is `Any`.

```py
def fixed_boundaries[*Ts](
    prefix: tuple[Any, *tuple[Any, ...]],
    suffix: tuple[*tuple[Any, ...], Any],
) -> None:
    plain: tuple[*Ts] = prefix  # error: [invalid-assignment]
    plain = suffix  # error: [invalid-assignment]
```

### Mixed gradual tuple assignability to symbolic packs

Fixed prefixes and suffixes are compared covariantly. The gradual segment can supply additional
required target elements as well as the symbolic pack, but fixed source elements cannot disappear.

```py
from typing import Any
from ty_extensions import static_assert
from ty_extensions._internal import Unknown, is_subtype_of

def mixed_sources[*Ts](
    prefix: tuple[bool, *tuple[Any, ...]],
    suffix: tuple[*tuple[Unknown, ...], str],
    both: tuple[bool, *tuple[Any, ...], str],
) -> None:
    prefixed: tuple[int, *Ts] = prefix
    suffixed: tuple[*Ts, object] = suffix
    bounded: tuple[int, *Ts, object] = both
    longer: tuple[int, bytes, *Ts, float, object] = both

    too_short: tuple[int, *Ts] = both  # error: [invalid-assignment]
    wrong_prefix: tuple[str, *Ts, object] = both  # error: [invalid-assignment]
    wrong_suffix: tuple[int, *Ts, int] = both  # error: [invalid-assignment]

    static_assert(not is_subtype_of(tuple[bool, *tuple[Any, ...], str], tuple[int, *Ts, object]))
```

### Fixed source elements crossing symbolic packs

A fixed source element that falls inside the symbolic pack must be assignable to every possible
element type. `Any`, `Unknown`, and `Never` allow this; `int` and `Any | int` do not. Even a `Never`
element remains a required tuple position when the pack is empty.

```py
from typing import Any, Never
from ty_extensions._internal import Unknown

def crossing_sources[*Ts](
    any_prefix: tuple[Any, *tuple[Any, ...]],
    unknown_suffix: tuple[*tuple[Any, ...], Unknown],
    never_prefix: tuple[Never, *tuple[Any, ...]],
    int_prefix: tuple[int, *tuple[Any, ...]],
    union_suffix: tuple[*tuple[Any, ...], Any | int],
) -> None:
    moved_prefix: tuple[*Ts, int] = any_prefix
    moved_suffix: tuple[int, *Ts] = unknown_suffix
    bottom_prefix: tuple[*Ts, int] = never_prefix

    too_short: tuple[*Ts] = never_prefix  # error: [invalid-assignment]
    restricted_prefix: tuple[*Ts, int] = int_prefix  # error: [invalid-assignment]
    restricted_suffix: tuple[int, *Ts] = union_suffix  # error: [invalid-assignment]
```

A scalar type variable can match itself at an aligned position, but it does not constrain the
unrelated elements of the symbolic pack. An aligned `T` also cannot satisfy a fixed `int` target,
since `T` is not necessarily a subtype of `int`.

```py
def scalar_source[T, *Ts](source: tuple[T, *tuple[Any, ...]]) -> None:
    aligned: tuple[T, *Ts] = source
    no_specialization: tuple[int, *Ts] = source  # error: [invalid-assignment]
    crossing: tuple[*Ts, int] = source  # error: [invalid-assignment]
```

### Symbolic pack assignability to mixed gradual tuples

Fixed source endpoints remain usable when the symbolic pack is erased to a gradual segment. The
target boundaries must also fit when the symbolic pack is empty.

```py
from typing import Any
from ty_extensions import static_assert
from ty_extensions._internal import Unknown, is_subtype_of

def mixed_targets[*Ts](source: tuple[bool, *Ts, str]) -> None:
    bounded: tuple[int, *tuple[Any, ...], object] = source
    prefixed: tuple[int, *tuple[Unknown, ...]] = source
    suffixed: tuple[*tuple[Any, ...], object] = source

    too_long: tuple[int, bytes, *tuple[Any, ...], str] = source  # error: [invalid-assignment]
    wrong_prefix: tuple[str, *tuple[Any, ...], object] = source  # error: [invalid-assignment]
    wrong_suffix: tuple[int, *tuple[Any, ...], int] = source  # error: [invalid-assignment]

    static_assert(not is_subtype_of(tuple[bool, *Ts, str], tuple[int, *tuple[Any, ...], object]))
```

### Fixed target elements crossing symbolic packs

A fixed target element that falls inside the symbolic pack must accept every possible element type.
`object` and `Any` allow this, but `int` and a separate scalar type variable do not. Unlike a source
element of type `Any | int`, a target element of that type can accept any pack element by
materializing its `Any` appropriately.

```py
from typing import Any
from ty_extensions._internal import Unknown

def crossing_targets[*Ts](
    prefix: tuple[int, *Ts],
    suffix: tuple[*Ts, int],
    long_suffix: tuple[*Ts, int, str],
    plain: tuple[*Ts],
) -> None:
    moved_prefix: tuple[object, *tuple[Any, ...]] = suffix
    moved_long_suffix: tuple[object, *tuple[Any, ...], str] = long_suffix
    moved_suffix: tuple[*tuple[Any, ...], Any] = prefix
    unknown_prefix: tuple[Unknown, *tuple[Any, ...]] = suffix
    union_suffix: tuple[*tuple[Any, ...], Any | int] = prefix

    restricted_prefix: tuple[int, *tuple[Any, ...]] = suffix  # error: [invalid-assignment]
    restricted_suffix: tuple[*tuple[Any, ...], int] = prefix  # error: [invalid-assignment]
    too_short: tuple[object, *tuple[Any, ...]] = plain  # error: [invalid-assignment]

def scalar_target[T, *Ts](source: tuple[*Ts, int]) -> None:
    target: tuple[T, *tuple[Any, ...]] = source  # error: [invalid-assignment]
```

### Protocol target elements crossing symbolic packs

A protocol used as a fixed target element must accept every possible pack element. All objects
support `__str__`, but a pack can contain an unhashable value such as a list. A protocol that
requires `__hash__` therefore cannot accept an arbitrary pack element at a fixed endpoint.

```py
from typing import Any, Protocol

class SupportsStr(Protocol):
    def __str__(self) -> str: ...

class SupportsHash(Protocol):
    def __hash__(self) -> int: ...

def protocol_targets[*Ts](prefix: tuple[int, *Ts], suffix: tuple[*Ts, int]) -> None:
    universal_prefix: tuple[SupportsStr, *tuple[Any, ...]] = suffix
    universal_suffix: tuple[*tuple[Any, ...], SupportsStr] = prefix

    hash_prefix: tuple[SupportsHash, *tuple[Any, ...]] = suffix  # error: [invalid-assignment]
    hash_suffix: tuple[*tuple[Any, ...], SupportsHash] = prefix  # error: [invalid-assignment]
```

The standard-library `Hashable` protocol follows the same rule, including through an alias:

```py
from collections.abc import Hashable

type HashableAlias = Hashable

def hashable_targets[*Ts](prefix: tuple[int, *Ts], suffix: tuple[*Ts, int]) -> None:
    hash_prefix: tuple[Hashable, *tuple[Any, ...]] = suffix  # snapshot: invalid-assignment
    hash_suffix: tuple[*tuple[Any, ...], HashableAlias] = prefix  # error: [invalid-assignment]
```

```snapshot
error[invalid-assignment]: Object of type `tuple[*Ts@hashable_targets, int]` is not assignable to `tuple[Hashable, *tuple[Any, ...]]`
  --> src/mdtest_snippet.py:20:54
   |
20 |     hash_prefix: tuple[Hashable, *tuple[Any, ...]] = suffix  # snapshot: invalid-assignment
   |                  ---------------------------------   ^^^^^^ Incompatible value of type `tuple[*Ts@hashable_targets, int]`
   |                  |
   |                  Declared type
```

Fixed `object` endpoints retain their ordinary assignability to `Hashable`, which permits uses such
as `object()` sentinels. This does not imply that arbitrary pack elements are hashable:

```py
def fixed_objects[*Ts](source: tuple[object, *Ts, object]) -> None:
    prefix: tuple[Hashable, *tuple[Any, ...]] = source
    suffix: tuple[*tuple[Any, ...], Hashable] = source
```

When a fixed source endpoint is unhashable, the diagnostic identifies that endpoint's type:

```py
def fixed_unhashable[*Ts](source: tuple[list[int], *Ts]) -> None:
    target: tuple[Hashable, *tuple[Any, ...]] = source  # snapshot: invalid-assignment
```

```snapshot
error[invalid-assignment]: Object of type `tuple[list[int], *Ts@fixed_unhashable]` is not assignable to `tuple[Hashable, *tuple[Any, ...]]`
  --> src/mdtest_snippet.py:26:49
   |
26 |     target: tuple[Hashable, *tuple[Any, ...]] = source  # snapshot: invalid-assignment
   |             ---------------------------------   ^^^^^^ Incompatible value of type `tuple[list[int], *Ts@fixed_unhashable]`
   |             |
   |             Declared type
info: type `list[int]` is not assignable to protocol `Hashable`
info: └── protocol member `__hash__` is incompatible
```

### Inferring scalar target elements beside symbolic packs

When assigning a generic function to a callable type, its scalar type parameter can be inferred from
a tuple containing a symbolic pack. The inferred element type must accept every possible pack
element. A type variable without an explicit bound can match, but one bounded by `Hashable` cannot.

```py
from collections.abc import Hashable
from typing import Any, Callable

def accept[T](value: tuple[T, *tuple[Any, ...]]) -> None: ...
def accept_hashable[T: Hashable](value: tuple[T, *tuple[Any, ...]]) -> None: ...
def callbacks[*Ts]() -> None:
    unbounded: Callable[[tuple[*Ts, int]], None] = accept
    hashable: Callable[[tuple[*Ts, int]], None] = accept_hashable  # error: [invalid-assignment]
```

### Aliases of gradual tuple elements

An alias of `Any` also makes a variadic segment gradual in length. Aliases of `list[Any]` or
`Any | int` do not have that effect, nor does a recursive container alias.

```py
from typing import Any

type Dynamic = Any
type IndirectDynamic = Dynamic
type AnyList = list[Any]
type PartlyDynamic = Any | int
type Recursive = list[Recursive]

def gradual_aliases[*Ts](
    source: tuple[int, *tuple[IndirectDynamic, ...], str],
    symbolic: tuple[int, *Ts, str],
) -> None:
    packed: tuple[int, *Ts, str] = source
    erased: tuple[int, *tuple[IndirectDynamic, ...], str] = symbolic

def non_gradual_aliases[*Ts](
    containers: tuple[int, *tuple[AnyList, ...]],
    union: tuple[int, *tuple[PartlyDynamic, ...]],
    recursive: tuple[int, *tuple[Recursive, ...]],
) -> None:
    pack: tuple[int, *Ts] = containers  # error: [invalid-assignment]
    pack = union  # error: [invalid-assignment]
    pack = recursive  # error: [invalid-assignment]
```

### Starred variadic parameters

An unpacked `TypeVarTuple` can annotate `*args`. Call binding infers the pack from direct arguments
and from the residual tuple shape of splatted arguments, while generic function bodies retain the
symbolic pack declared by the function.

```py
def simple[*Ts](*args: *Ts) -> tuple[*Ts]:
    reveal_type(args)  # revealed: tuple[*Ts@simple]
    raise NotImplementedError

def with_prefix[T, *Ts](prefix: T, *args: *Ts) -> tuple[T, *Ts]:
    raise NotImplementedError

def bounded[*Ts](head: int, *rest: *tuple[*Ts, str]) -> tuple[*Ts]:
    raise NotImplementedError

def with_kw_only[T, *Ts](*args: *Ts, kw: T) -> tuple[*Ts, T]:
    raise NotImplementedError

def forward[*Us](*args: *Us) -> tuple[*Us]:
    reveal_type(simple(*args))  # revealed: tuple[*Us@forward]
    return simple(*args)

def f(
    i: int,
    s: str,
    b: bool,
    empty: tuple[()],
    one: tuple[int],
    fixed: tuple[int, str],
    suffix: tuple[bool, str],
    unbounded: tuple[int, ...],
    mixed: tuple[int, *tuple[str, ...], bytes],
    xs: list[int],
) -> None:
    reveal_type(simple())  # revealed: tuple[()]
    reveal_type(simple(i))  # revealed: tuple[int]
    reveal_type(simple(i, s))  # revealed: tuple[int, str]
    reveal_type(simple(*(i, s)))  # revealed: tuple[int, str]
    reveal_type(simple(i, s, b))  # revealed: tuple[int, str, bool]
    reveal_type(simple(fixed))  # revealed: tuple[tuple[int, str]]
    reveal_type(simple(*empty))  # revealed: tuple[()]
    reveal_type(simple(*one))  # revealed: tuple[int]
    reveal_type(simple(*fixed))  # revealed: tuple[int, str]
    reveal_type(simple(*unbounded))  # revealed: tuple[int, ...]
    reveal_type(simple(*mixed))  # revealed: tuple[int, *tuple[str, ...], bytes]
    reveal_type(simple(*xs))  # revealed: tuple[int, ...]

    reveal_type(with_prefix(i))  # revealed: tuple[int]
    reveal_type(with_prefix(i, s, b))  # revealed: tuple[int, str, bool]
    reveal_type(with_prefix(*fixed))  # revealed: tuple[int, str]
    reveal_type(with_prefix(i, *fixed))  # revealed: tuple[int, int, str]
    reveal_type(with_prefix(*unbounded))  # revealed: tuple[int, *tuple[int, ...]]
    reveal_type(with_prefix(i, *unbounded))  # revealed: tuple[int, *tuple[int, ...]]
    reveal_type(with_prefix(*xs))  # revealed: tuple[int, *tuple[int, ...]]

    reveal_type(bounded(i, *suffix))  # revealed: tuple[bool]

    reveal_type(with_kw_only(kw=b))  # revealed: tuple[bool]
    reveal_type(with_kw_only(i, s, kw=b))  # revealed: tuple[int, str, bool]
    reveal_type(with_kw_only(fixed, kw=b))  # revealed: tuple[tuple[int, str], bool]
    reveal_type(with_kw_only(*fixed, kw=b))  # revealed: tuple[int, str, bool]
    reveal_type(with_kw_only(unbounded, kw=b))  # revealed: tuple[tuple[int, ...], bool]
    reveal_type(with_kw_only(*unbounded, kw=b))  # revealed: tuple[*tuple[int, ...], bool]
    reveal_type(with_kw_only(*xs, kw=b))  # revealed: tuple[*tuple[int, ...], bool]

    # error: [missing-argument] "No argument provided for required parameter `kw` of function `with_kw_only`"
    reveal_type(with_kw_only(i, s, b))  # revealed: tuple[int, str, bool, Unknown]
```

Variadic inference preserves contextual argument types, including an outer type variable.

```py
from typing import TypedDict

class Payload(TypedDict):
    value: int

def contextual[T](value: T) -> None:
    concrete: tuple[Payload, list[int]] = simple({"value": 1}, [])
    generic: tuple[Payload, T] = simple({"value": 1}, value)
    # error: [invalid-assignment]
    # error: [invalid-argument-type]
    invalid: tuple[Payload] = simple({"value": "wrong"})
```

Fixed values next to a type variable tuple keep their normal bound diagnostics.

```py
def bounded_arguments[U: bytes, T: str, *Ts](first: U, *args: *tuple[*Ts, T]) -> tuple[*Ts, T]:
    raise NotImplementedError

bounded_arguments(
    1,  # error: [invalid-argument-type] "upper bound `bytes`"
    "ok",
    2,  # error: [invalid-argument-type] "upper bound `str`"
)

def check_splat_error(values: list[int]) -> None:
    bounded_arguments(
        b"valid",
        *values,  # snapshot: invalid-argument-type
    )
```

```snapshot
error[invalid-argument-type]: Argument to function `bounded_arguments` is incorrect
  --> src/mdtest_snippet.py:86:9
   |
86 |         *values,  # snapshot: invalid-argument-type
   |         ^^^^^^^ Argument type `int` does not satisfy upper bound `str` of type variable `T`
info: Type variable defined here
  --> src/mdtest_snippet.py:74:33
   |
74 | def bounded_arguments[U: bytes, T: str, *Ts](first: U, *args: *tuple[*Ts, T]) -> tuple[*Ts, T]:
   |                                 ^^^^^^
```

### Union splatted arguments

Equal-length tuple unions preserve their length and combine the types at each position. Different
lengths produce an open tuple, while direct arguments around the splat keep their known positions.

```py
def collect[*Ts](*args: *Ts) -> tuple[*Ts]:
    return args

def check(
    same_length: tuple[int] | tuple[str],
    paired: tuple[int, str] | tuple[bytes, bool],
    different_lengths: tuple[int] | tuple[str, bytes],
    prefix: bool,
    suffix: bytes,
) -> None:
    reveal_type(collect(*same_length))  # revealed: tuple[int | str]
    reveal_type(collect(*paired))  # revealed: tuple[int | bytes, str | bool]
    reveal_type(collect(*different_lengths))  # revealed: tuple[int | str | bytes, ...]
    reveal_type(collect(prefix, *same_length, suffix))  # revealed: tuple[bool, int | str, bytes]

    # error: [invalid-assignment]
    wrong: tuple[bytes] = collect(*same_length)
```

### Starred variadic arguments without a variadic return

A bounded or constrained element is checked even when the return type does not contain its pack.

```py
def bounded_prefix[T: str, *Ts](*args: *tuple[T, *Ts]) -> None: ...
def constrained_suffix[T: (str, bytes), *Ts](*args: *tuple[*Ts, T]) -> None: ...
def check(values: list[int], valid: list[str]) -> None:
    bounded_prefix(*valid)
    constrained_suffix(*valid)

    # error: [invalid-argument-type]
    bounded_prefix(*values)
    # error: [invalid-argument-type]
    constrained_suffix(*values)
```

### Argument types override incompatible contextual return types

A contextual return type can guide compatible arguments, but it must not override the argument types
or the number of arguments in a call.

```py
def collect[*Ts](*args: *Ts) -> tuple[*Ts]:
    return args

valid: tuple[int] = collect(1)

inferred = collect(1)
reveal_type(inferred)  # revealed: tuple[Literal[1]]
# error: [invalid-assignment]
indirect: tuple[str] = inferred
# error: [invalid-assignment]
direct: tuple[str] = collect(1)

valid_empty: tuple[()] = collect()
# error: [invalid-assignment]
invalid_empty: tuple[str] = collect()
```

Return statements and arguments to other functions also provide contextual return types.

```py
def invalid_return() -> tuple[str]:
    # error: [invalid-return-type]
    return collect(1)

def accept_strings(values: tuple[str]) -> None: ...

accept_strings(collect("valid"))
# error: [invalid-argument-type]
accept_strings(collect(1))
```

### Fixed boundaries around variadic type variable tuples

Fixed values before or after a type variable tuple do not become part of its inferred shape. Open
splats can provide those boundaries while preserving fixed values already present on the other side.

```py
def prefixed[*Ts](*args: *tuple[int, *Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def suffixed[*Ts](*args: *tuple[*Ts, str]) -> tuple[*Ts]:
    raise NotImplementedError

def bounded[*Ts](*args: *tuple[int, *Ts, int]) -> tuple[*Ts]:
    raise NotImplementedError

def check(
    ints: list[int],
    strings: list[str],
    extra_prefix: tuple[int, bool, *tuple[str, ...], bytes],
    extra_suffix: tuple[bool, *tuple[int, ...], bytes, str],
    extra_boundaries: tuple[int, bool, *tuple[str, ...], bytes, int],
    missing_prefix: tuple[*tuple[int, ...], bytes],
    missing_suffix: tuple[bool, *tuple[str, ...]],
) -> None:
    reveal_type(prefixed(1))  # revealed: tuple[()]
    reveal_type(prefixed(1, True))  # revealed: tuple[Literal[True]]
    reveal_type(prefixed(*ints))  # revealed: tuple[int, ...]
    reveal_type(prefixed(*extra_prefix))  # revealed: tuple[bool, *tuple[str, ...], bytes]
    reveal_type(prefixed(*missing_prefix))  # revealed: tuple[*tuple[int, ...], bytes]

    reveal_type(suffixed("last"))  # revealed: tuple[()]
    reveal_type(suffixed(True, "last"))  # revealed: tuple[Literal[True]]
    reveal_type(suffixed(*strings))  # revealed: tuple[str, ...]
    reveal_type(suffixed(*extra_suffix))  # revealed: tuple[bool, *tuple[int, ...], bytes]
    reveal_type(suffixed(*missing_suffix))  # revealed: tuple[bool, *tuple[str, ...]]

    reveal_type(bounded(1, 1))  # revealed: tuple[()]
    reveal_type(bounded(1, True, 1))  # revealed: tuple[Literal[True]]
    reveal_type(bounded(*ints))  # revealed: tuple[int, ...]
    reveal_type(bounded(*extra_boundaries))  # revealed: tuple[bool, *tuple[str, ...], bytes]
```

### Callable inference

`Callable` accepts unpacked `TypeVarTuple`s in its positional parameter list.

```py
from typing import Callable

def simple[*Ts](callback: Callable[[*Ts], tuple[*Ts]]) -> tuple[*Ts]:
    reveal_type(callback)  # revealed: (*Ts@simple) -> tuple[*Ts@simple]
    raise NotImplementedError

def positional_only(x: int, y: str, /) -> tuple[int, str]:
    raise NotImplementedError

def no_parameters() -> tuple[()]:
    raise NotImplementedError

def standard(x: int, y: str) -> tuple[int, str]:
    raise NotImplementedError

def positional_variadic(x: int, *args: str) -> tuple[int, *tuple[str, ...]]:
    raise NotImplementedError

def variadic1(*args: int) -> tuple[int, ...]:
    raise NotImplementedError

def variadic2(*args: int) -> tuple[str, ...]:
    raise NotImplementedError

def accepts_object(value: object, /) -> tuple[int]:
    raise NotImplementedError

def keyword_only(*, x: int) -> tuple[int]:
    raise NotImplementedError

def gradual(callback: Callable[..., tuple[int, ...]]) -> None:
    reveal_type(simple(callback))  # revealed: tuple[int, ...]

reveal_type(simple(no_parameters))  # revealed: tuple[()]
reveal_type(simple(positional_only))  # revealed: tuple[int, str]
reveal_type(simple(standard))  # revealed: tuple[int, str]
reveal_type(simple(positional_variadic))  # revealed: tuple[int, *tuple[str, ...]]
reveal_type(simple(variadic1))  # revealed: tuple[int, ...]
reveal_type(simple(accepts_object))  # revealed: tuple[int]

# TODO: Report the incompatible return type after callable specialization fails.
reveal_type(simple(variadic2))  # revealed: tuple[Unknown, ...]
# error: [invalid-argument-type] "Argument to function `simple` is incorrect: Expected `(*args: Unknown) -> tuple[Unknown, ...]`, found `def keyword_only(*, x: int) -> tuple[int]`"
reveal_type(simple(keyword_only))  # revealed: tuple[Unknown, ...]
```

### Callable inference through invariant and contravariant wrappers

An unpacked `TypeVarTuple` keeps its precise inferred parameter types when a callable or callable
protocol is nested inside an invariant or contravariant wrapper.

```py
from typing import Callable, Protocol

class Invariant[T]:
    def __init__(self, callback: T) -> None: ...
    callback: T

class Contravariant[T]:
    def __init__(self, callback: T) -> None: ...
    def put(self, callback: T) -> None: ...

def invariant[*Ts](wrapper: Invariant[Callable[[*Ts], None]]) -> tuple[*Ts]:
    raise NotImplementedError

def contravariant[*Ts](wrapper: Contravariant[Callable[[*Ts], None]]) -> tuple[*Ts]:
    raise NotImplementedError

def callback(first: object, value: str) -> None: ...

reveal_type(invariant(Invariant(callback)))  # revealed: tuple[object, str]
reveal_type(contravariant(Contravariant(callback)))  # revealed: tuple[object, str]
```

A callable protocol preserves the same inferred parameters through both wrapper variances.

```py
class Callback[*Ts](Protocol):
    def __call__(self, *args: *Ts) -> None: ...

def invariant_protocol[*Ts](wrapper: Invariant[Callback[*Ts]]) -> tuple[*Ts]:
    raise NotImplementedError

def contravariant_protocol[*Ts](wrapper: Contravariant[Callback[*Ts]]) -> tuple[*Ts]:
    raise NotImplementedError

reveal_type(invariant_protocol(Invariant(callback)))  # revealed: tuple[object, str]
reveal_type(contravariant_protocol(Contravariant(callback)))  # revealed: tuple[object, str]
```

Separately declared protocols with equivalent variadic methods also preserve the exact inferred
tuple under both wrapper variances.

```py
class Target[*Ts](Protocol):
    def call(self, *args: *Ts) -> None: ...

class Actual[*Ts](Protocol):
    def call(self, *args: *Ts) -> None: ...

def invariant_structural[*Ts](wrapper: Invariant[Target[*Ts]]) -> tuple[*Ts]:
    raise NotImplementedError

def contravariant_structural[*Ts](wrapper: Contravariant[Target[*Ts]]) -> tuple[*Ts]:
    raise NotImplementedError

def check_structural(
    invariant_wrapper: Invariant[Actual[str]],
    contravariant_wrapper: Contravariant[Actual[str]],
) -> None:
    reveal_type(invariant_structural(invariant_wrapper))  # revealed: tuple[str]
    reveal_type(contravariant_structural(contravariant_wrapper))  # revealed: tuple[str]
```

Unions of structurally compatible protocols retain the same tuple. Incompatible alternatives are
rejected without widening their inferred tuple to an unknown-length tuple.

```py
class Other[*Ts](Protocol):
    def call(self, *args: *Ts) -> None: ...

def check_unions(
    invariant_match: Invariant[Actual[str] | Other[str]],
    contravariant_match: Contravariant[Actual[str] | Other[str]],
    invariant_mismatch: Invariant[Actual[str] | Other[bytes]],
    contravariant_mismatch: Contravariant[Actual[str] | Other[bytes]],
) -> None:
    reveal_type(invariant_structural(invariant_match))  # revealed: tuple[str]
    reveal_type(contravariant_structural(contravariant_match))  # revealed: tuple[str]
    # error: [invalid-argument-type]
    reveal_type(invariant_structural(invariant_mismatch))  # revealed: tuple[()]
    # error: [invalid-argument-type]
    reveal_type(contravariant_structural(contravariant_mismatch))  # revealed: tuple[()]
```

A nominal class implementing the same variadic protocol retains its precise method parameter.

```py
class StringRunner:
    def call(self, value: str) -> None: ...

reveal_type(invariant_structural(Invariant(StringRunner())))  # revealed: tuple[str]
reveal_type(contravariant_structural(Contravariant(StringRunner())))  # revealed: tuple[str]
```

### Callable return inference

An unpacked `TypeVarTuple` in a callable return type is inferred as one packed tuple, including
fixed elements surrounding it.

```py
from typing import Callable

def infer_return[*Ts](callback: Callable[[], tuple[*Ts]]) -> tuple[*Ts]:
    raise NotImplementedError

def empty_return() -> tuple[()]:
    raise NotImplementedError

def fixed_return() -> tuple[int, str]:
    raise NotImplementedError

def mixed_return() -> tuple[int, *tuple[str, ...]]:
    raise NotImplementedError

reveal_type(infer_return(empty_return))  # revealed: tuple[()]
reveal_type(infer_return(fixed_return))  # revealed: tuple[int, str]
reveal_type(infer_return(mixed_return))  # revealed: tuple[int, *tuple[str, ...]]

def infer_return_middle[*Ts](
    callback: Callable[[], tuple[int, *Ts, bytes]],
) -> tuple[*Ts]:
    raise NotImplementedError

def fixed_middle() -> tuple[int, str, bytes]:
    raise NotImplementedError

def mixed_middle() -> tuple[int, *tuple[str, ...], bytes]:
    raise NotImplementedError

reveal_type(infer_return_middle(fixed_middle))  # revealed: tuple[str]
reveal_type(infer_return_middle(mixed_middle))  # revealed: tuple[str, ...]
```

### Callable inference with sub-call checking

This usage pattern is similar to how `ParamSpec` can be used to accept a callable and its arguments
except that in the case of `TypeVarTuple` all parameters are positional-only.

```py
from typing import Callable

def invoke[*Ts, R](callback: Callable[[*Ts], R], *args: *Ts) -> R:
    raise NotImplementedError

def positional_only(x: int, y: str, /) -> tuple[int, str]:
    raise NotImplementedError

def standard(x: int, y: str) -> tuple[int, str]:
    raise NotImplementedError

def positional_variadic(x: int, *args: str) -> tuple[int, *tuple[str, ...]]:
    raise NotImplementedError

reveal_type(invoke(positional_only, 1, "a"))  # revealed: tuple[int, str]
# TODO: Validate arguments matched to the variadic parameter against the `TypeVarTuple` inferred
# from the callback.
reveal_type(invoke(positional_only))  # revealed: tuple[int, str]
reveal_type(invoke(positional_only, 1))  # revealed: tuple[int, str]
reveal_type(invoke(positional_only, 1, 2))  # revealed: tuple[int, str]

reveal_type(invoke(standard, 1, "a"))  # revealed: tuple[int, str]
# error: [unknown-argument] "Argument `x` does not match any known parameter of function `invoke`"
# error: [unknown-argument] "Argument `y` does not match any known parameter of function `invoke`"
reveal_type(invoke(standard, x=1, y="a"))  # revealed: tuple[int, str]

reveal_type(invoke(positional_variadic, 1, "a", "b"))  # revealed: tuple[int, *tuple[str, ...]]
reveal_type(invoke(positional_variadic, 1))  # revealed: tuple[int, *tuple[str, ...]]
reveal_type(invoke(positional_variadic))  # revealed: tuple[int, *tuple[str, ...]]

def accept_forwarded[*Ts](callback: Callable[[*Ts], object], args: tuple[*Ts]) -> None: ...
def forward[*Ts](callback: Callable[[*Ts], object], *args: *Ts) -> None:
    accept_forwarded(callback, args)

def accept_mixed_forwarded[*Ts](
    callback: Callable[[int, *Ts, str], object],
    args: tuple[int, *Ts, str],
) -> None: ...
def forward_mixed[*Ts](
    callback: Callable[[int, *Ts, str], object],
    *args: *tuple[int, *Ts, str],
) -> None:
    accept_mixed_forwarded(callback, args)
```

### Callable inference through nested callable parameters

Nested callable parameters make the pack covariant, but inference currently loses its fixed length.

```py
from typing import Callable

def nested[*Ts](
    callback: Callable[[Callable[[*Ts], None]], None],
    *args: *Ts,
) -> tuple[*Ts]:
    return args

def accepts_int_callback(callback: Callable[[int], None]) -> None: ...
def check(value: int, other: str) -> None:
    # TODO: Should reveal `tuple[int]`.
    reveal_type(nested(accepts_int_callback, value))  # revealed: tuple[int, ...]
    # TODO: Should reveal `tuple[int | str]`.
    reveal_type(nested(accepts_int_callback, other))  # revealed: tuple[int, ...]

    # TODO: Should report an error because the callback accepts only one argument.
    nested(accepts_int_callback, value, other)
```

### Starred variadic tuple normalization

A fixed provided tuple containing `Never` keeps its shape during tuple-level constraint inference.
Its `Never` element must not be discarded or replaced by an unknown-length tuple.

```py
from typing import Never

def collect[*Ts](*args: *Ts) -> tuple[*Ts]:
    raise NotImplementedError

def collect_prefixed[*Ts](*args: *tuple[int, *Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def check_never(value: Never) -> None:
    reveal_type(collect(value))  # revealed: tuple[Never]
    reveal_type(collect_prefixed(1, value))  # revealed: tuple[Never]
```

### Unsupported callable checks are deferred

A generic callback can leave the expected callable with a gradual positional parameter list until
callback constraints are combined with the inferred arguments. Similarly, inferring each position
from an overload independently loses the correlation between overload branches. Avoid reporting
these cases until callback forwarding is supported.

```py
from collections.abc import Awaitable, Callable
from typing import overload

def start[*Ts](callback: Callable[[*Ts], Awaitable[object]], *args: *Ts) -> None: ...
async def waiter[T](value: T, mapping: dict[T, int]) -> None: ...

values: dict[int, int] = {}
start(waiter, 1, values)

def invoke[*Ts, R](callback: Callable[[*Ts], R], *args: *Ts) -> R:
    raise NotImplementedError

@overload
def correlated(left: str, right: str) -> str: ...
@overload
def correlated(left: bytes, right: bytes) -> bytes: ...
def correlated(left: str | bytes, right: str | bytes) -> str | bytes:
    return left

def wrapper[AnyStr: (str, bytes)](left: AnyStr, right: AnyStr) -> str | bytes:
    return invoke(correlated, left, right)
```

### Callable inference with fixed positional parameters

Fixed positional parameters surrounding an unpacked `TypeVarTuple` are excluded from the inferred
tuple.

```py
from typing import Callable

def infer_with_suffix[*Ts](callback: Callable[[int, *Ts, bytes], None]) -> tuple[*Ts]:
    raise NotImplementedError

def fixed_suffix(prefix: int, middle: str, suffix: bytes, /) -> None: ...
def empty_middle(prefix: int, suffix: bytes, /) -> None: ...
def unpacked_suffix(*args: *tuple[int, *tuple[str, ...], bytes]) -> None: ...

reveal_type(infer_with_suffix(fixed_suffix))  # revealed: tuple[str]
reveal_type(infer_with_suffix(empty_middle))  # revealed: tuple[()]
reveal_type(infer_with_suffix(unpacked_suffix))  # revealed: tuple[str, ...]
```

### Nested unpacked callable parameters

Nested unpacked tuple parameters are equivalent to their flattened form.

```py
from typing import Callable

def expect_nested(
    callback: Callable[[int, *tuple[*tuple[str, ...], bytes], str], None],
) -> None: ...
def pass_flattened(
    callback: Callable[[int, *tuple[str, ...], bytes, str], None],
) -> None:
    expect_nested(callback)
```

### Nested unpacked `TypeVarTuple` callable parameters

A `TypeVarTuple` nested inside an unpacked tuple remains inferable after the surrounding tuple is
expanded into its fixed prefix and suffix.

```py
from typing import Callable

def infer_nested[*Ts](callback: Callable[[int, *tuple[*Ts, bytes]], None]) -> tuple[*Ts]:
    raise NotImplementedError

def fixed_middle(prefix: int, middle: str, suffix: bytes, /) -> None: ...
def empty_middle(prefix: int, suffix: bytes, /) -> None: ...

reveal_type(infer_nested(fixed_middle))  # revealed: tuple[str]
reveal_type(infer_nested(empty_middle))  # revealed: tuple[()]
```

### Callable inference with additional keyword parameters

Additional keyword-only or variadic keyword parameters do not contribute to a `TypeVarTuple`
inferred from a `Callable`'s positional parameter list.

```py
from typing import Callable

def infer_positional[*Ts](callback: Callable[[*Ts], None]) -> tuple[*Ts]:
    raise NotImplementedError

def optional_keyword_only(x: int, y: str, *, debug: bool = False) -> None: ...
def extra_keywords(x: int, y: str, **kwargs: bool) -> None: ...

reveal_type(infer_positional(optional_keyword_only))  # revealed: tuple[int, str]
reveal_type(infer_positional(extra_keywords))  # revealed: tuple[int, str]
```

### Callable protocol inference

`Callable[[*Ts], R]` can only describe positional-only parameters. Callable protocols are used below
to test `TypeVarTuple` inference for signatures that combine variadic positional parameters with
keyword-only or variadic keyword parameters.

#### Keyword-only parameters

A callable protocol can combine a `TypeVarTuple` with required or optional keyword-only parameters
and a fixed positional prefix.

```py
from typing import Protocol

class KeywordOnlyCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, flag: bool) -> None: ...

def infer_keyword_only[*Ts](callback: KeywordOnlyCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def explicit_keyword_only(x: int, y: str, *, flag: bool) -> None: ...
def positional_only_with_keyword(x: int, y: str, /, *, flag: bool) -> None: ...
def positional_or_keyword(x: int, y: str, flag: bool) -> None: ...
def keyword_catch_all(x: int, y: str, **kwargs: object) -> None: ...

reveal_type(infer_keyword_only(explicit_keyword_only))  # revealed: tuple[int, str]
reveal_type(infer_keyword_only(positional_only_with_keyword))  # revealed: tuple[int, str]
# TODO: Should reveal `tuple[int, str]`.
# error: [invalid-argument-type] "Argument to function `infer_keyword_only` is incorrect: Expected `KeywordOnlyCallback[*tuple[Unknown, ...]]`, found `def positional_or_keyword(x: int, y: str, flag: bool) -> None`"
reveal_type(infer_keyword_only(positional_or_keyword))  # revealed: tuple[Unknown, ...]
reveal_type(infer_keyword_only(keyword_catch_all))  # revealed: tuple[int, str]

class OptionalKeywordCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, flag: bool = False) -> None: ...

def infer_optional_keyword[*Ts](callback: OptionalKeywordCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def optional_keyword_callback(x: int, y: str, *, flag: bool = False) -> None: ...

reveal_type(infer_optional_keyword(optional_keyword_callback))  # revealed: tuple[int, str]

class PrefixedKeywordCallback[*Ts](Protocol):
    def __call__(self, prefix: bytes, *args: *Ts, flag: bool) -> None: ...

def infer_prefixed[*Ts](callback: PrefixedKeywordCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def prefixed(prefix: bytes, x: int, y: str, *, flag: bool) -> None: ...
def prefixed_variadic(prefix: bytes, *args: str, flag: bool) -> None: ...

reveal_type(infer_prefixed(prefixed))  # revealed: tuple[int, str]

# An open-ended positional parameter can be inferred in an otherwise mixed signature.
reveal_type(infer_prefixed(prefixed_variadic))  # revealed: tuple[str, ...]
```

#### Variadic keyword parameters

Variadic keyword parameters are matched separately from the positional parameters captured by a
`TypeVarTuple`.

```py
from typing import Protocol

class KeywordVariadicCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, **kwargs: int) -> None: ...

def infer_keyword_variadic[*Ts](callback: KeywordVariadicCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def keyword_variadic(x: int, y: str, **kwargs: int) -> None: ...

reveal_type(infer_keyword_variadic(keyword_variadic))  # revealed: tuple[int, str]

class KeywordOnlyAndVariadicCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, flag: bool, **kwargs: int) -> None: ...

def infer_keyword_only_and_variadic[*Ts](
    callback: KeywordOnlyAndVariadicCallback[*Ts],
) -> tuple[*Ts]:
    raise NotImplementedError

def keyword_only_and_variadic(x: int, y: str, *, flag: bool, **kwargs: int) -> None: ...

reveal_type(infer_keyword_only_and_variadic(keyword_only_and_variadic))  # revealed: tuple[int, str]

class MultipleKeywordCallback[*Ts](Protocol):
    def __call__(self, *args: *Ts, first: int, second: str) -> None: ...

def infer_multiple_keywords[*Ts](callback: MultipleKeywordCallback[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def multiple_keyword_catch_all(x: int, y: str, **kwargs: object) -> None: ...

reveal_type(infer_multiple_keywords(multiple_keyword_catch_all))  # revealed: tuple[int, str]
```

### Length-sensitive inference

If the same `TypeVarTuple` instance is used in multiple places in a signature or class, the exact
inference behavior is not specified in the typing spec. However, all usages must match in length.

```py
def foo[*Ts](arg1: tuple[*Ts], arg2: tuple[*Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def f(i: int, s: str, b: bool) -> None:
    reveal_type(foo((i, s), (b, i)))  # revealed: tuple[int, str | int]
    # error: [invalid-argument-type] "Argument to function `foo` is incorrect: Expected `tuple[int]`, found `tuple[str, bool]`"
    reveal_type(foo((i,), (s, b)))  # revealed: tuple[int]
```

A positional tuple and `*args` using the same type variable tuple must have the same length. When
their lengths match, their element types are combined.

```py
def repeat[*Ts](expected: tuple[*Ts], *args: *Ts) -> tuple[*Ts]:
    return expected

def check_repeated(i: int, s: str) -> None:
    reveal_type(repeat(()))  # revealed: tuple[()]
    reveal_type(repeat((i, s), i, s))  # revealed: tuple[int, str]
    reveal_type(repeat((i, s), i, i))  # revealed: tuple[int, str | int]

    # error: 5 [invalid-argument-type] "Argument to function `repeat` is incorrect: Expected `tuple[int]`, found `tuple[()]`"
    repeat((i,))
    # error: 20 [invalid-argument-type] "Argument to function `repeat` is incorrect: Expected `tuple[int, str]`, found `tuple[int]`"
    repeat((i, s), i)
    # snapshot: invalid-argument-type
    repeat((i,), i, s)
```

```snapshot
error[invalid-argument-type]: Argument to function `repeat` is incorrect
  --> src/mdtest_snippet.py:21:18
   |
21 |     repeat((i,), i, s)
   |                  ^^^^ Expected `tuple[int]`, found `tuple[int, str]`
info: a tuple of length 2 is not assignable to a tuple of length 1
info: Function defined here
 --> src/mdtest_snippet.py:8:5
  |
8 | def repeat[*Ts](expected: tuple[*Ts], *args: *Ts) -> tuple[*Ts]:
  |     ^^^^^^                            ---------- Parameter declared here
```

The same length and element-type rules apply when the tuple is passed as a keyword-only argument.

```py
def repeat_keyword[*Ts](*args: *Ts, expected: tuple[*Ts]) -> tuple[*Ts]:
    return expected

def check_repeated_keyword(i: int, s: str) -> None:
    reveal_type(repeat_keyword(expected=()))  # revealed: tuple[()]
    reveal_type(repeat_keyword(i, s, expected=(i, s)))  # revealed: tuple[int, str]
    reveal_type(repeat_keyword(i, i, expected=(i, s)))  # revealed: tuple[int, str | int]

    # error: 20 [invalid-argument-type] "Argument to function `repeat_keyword` is incorrect: Expected `tuple[int, str]`, found `tuple[int]`"
    repeat_keyword(i, expected=(i, s))
    # error: 20 [invalid-argument-type] "Argument to function `repeat_keyword` is incorrect: Expected `tuple[int]`, found `tuple[int, str]`"
    repeat_keyword(i, s, expected=(i,))
```

Matching lengths are also required when the return type does not contain the type variable tuple.

```py
def repeat_without_return[*Ts](expected: tuple[*Ts], *args: *Ts) -> None: ...

repeat_without_return((1, "value"), 1, "value")
# error: [invalid-argument-type]
repeat_without_return((1, "value"), 1)
```

## Type concatenation

A type variable tuple can be combined with fixed leading or trailing types.

```py
class Array[*Ts]: ...
class A: ...
class B: ...
class C: ...
class D: ...

def add_letter_a[*Ts](x: Array[*Ts]) -> Array[A, *Ts]:
    raise NotImplementedError

def del_letter_a[*Ts](x: Array[A, *Ts]) -> Array[*Ts]:
    raise NotImplementedError

def add_letters[*Ts](x: Array[*Ts]) -> Array[A, *Ts, C]:
    raise NotImplementedError

def del_letter_c[*Ts](x: Array[*Ts, C]) -> Array[*Ts]:
    raise NotImplementedError

def generic[T, *Ts](x: T, y: Array[*Ts]) -> Array[T, *Ts]:
    raise NotImplementedError

reveal_type(add_letters(Array[B, D]()))  # revealed: Array[A, B, D, C]
reveal_type(add_letter_a(Array[B, C]()))  # revealed: Array[A, B, C]

reveal_type(del_letter_a(Array[A, B]()))  # revealed: Array[B]
# error: [invalid-argument-type] "Argument to function `del_letter_a` is incorrect: Expected `Array[A, C]`, found `Array[B, C]`"
reveal_type(del_letter_a(Array[B, C]()))  # revealed: Array[C]

reveal_type(del_letter_c(Array[A, B, C]()))  # revealed: Array[A, B]
# error: [invalid-argument-type] "Argument to function `del_letter_c` is incorrect: Expected `Array[A, C]`, found `Array[A, B]`"
reveal_type(del_letter_c(Array[A, B]()))  # revealed: Array[A]

reveal_type(generic(A(), Array[B, D]()))  # revealed: Array[A, B, D]
reveal_type(generic(A(), Array[()]()))  # revealed: Array[A]
```

## Unpacking Unbounded Tuple Types

An unpacked unbounded tuple can describe an unknown middle section while retaining fixed endpoints,
and it can be passed into a function that solves a type variable tuple.

```py
from typing import Any

def accept_any_in_between(x: tuple[bytes, *tuple[Any, ...], int]) -> None: ...
def carry_items[*Items](x: tuple[bytes, *Items, int]) -> tuple[*Items]:
    raise NotImplementedError

def f(
    empty: tuple[bytes, int],
    multi: tuple[bytes, str, bool, int],
    truncated: tuple[bytes],
    dynamic: tuple[bytes, *tuple[Any, ...], int],
) -> None:
    accept_any_in_between(empty)
    accept_any_in_between(multi)
    # error: [invalid-argument-type] "Argument to function `accept_any_in_between` is incorrect: Expected `tuple[bytes, *tuple[Any, ...], int]`, found `tuple[bytes]`"
    accept_any_in_between(truncated)
    reveal_type(carry_items(dynamic))  # revealed: tuple[Any, ...]
```

When a mixed unbounded tuple is used to solve a `TypeVarTuple`, its fixed prefix and suffix remain
part of the solution.

```py
def preserve[*Ts](value: tuple[*Ts]) -> tuple[*Ts]:
    return value

def f(
    prefix: tuple[int, *tuple[str, ...]],
    suffix: tuple[*tuple[str, ...], bytes],
    mixed: tuple[int, *tuple[str, ...], bytes],
) -> None:
    reveal_type(preserve(prefix))  # revealed: tuple[int, *tuple[str, ...]]
    reveal_type(preserve(suffix))  # revealed: tuple[*tuple[str, ...], bytes]
    reveal_type(preserve(mixed))  # revealed: tuple[int, *tuple[str, ...], bytes]
```

A tuple containing an unpacked tuple can precisely describe heterogeneous positional arguments,
including a variable-length middle portion or a type-variable prefix.

```py
def accept_str_in_between(*args: *tuple[bool, *tuple[str, ...], bytes]) -> None: ...
def remove_bytes[*Prefix](*args: *tuple[*Prefix, bytes]) -> tuple[*Prefix]:
    raise NotImplementedError

accept_str_in_between(True, "phase", "status", b"ok")
accept_str_in_between(True, b"ok")
accept_str_in_between(True, 1, b"bad")  # error: [invalid-argument-type]

reveal_type(remove_bytes(1, "record", b"sum"))  # revealed: tuple[Literal[1], Literal["record"]]
```

## `@staticmethod` and `@classmethod`

```py
from typing import Self

class Foo[*Ts]:
    @staticmethod
    def static_method(*args: *Ts) -> None: ...
    @classmethod
    def class_method(cls, *args: *Ts) -> Self:
        raise NotImplementedError

reveal_type(Foo[int, str].class_method(1, ""))  # revealed: Foo[int, str]

foo = Foo[int, str]()
foo.static_method(1, "")
foo.class_method(1, "")

# error: [invalid-argument-type]
foo.static_method(1, 2)
# error: [invalid-argument-type]
foo.class_method(1, 2)
```

## Type Aliases

### Variadic aliases

```py
type Simple[*Ts] = tuple[*Ts]
type Prefix[T, *Ts] = tuple[T, *Ts]
type Suffix[*Ts, T] = tuple[*Ts, T]
type Between[T, *Ts, U] = tuple[T, *Ts, U]

def _(
    a1: Simple[()],
    a2: Simple[int, str],
    a3: Between[int, str],
    a4: Between[int, bool, str],
    a5: Between[int, bool, bytes, str],
    a6: Prefix[bool],
    a7: Prefix[bool, int, str],
    a8: Suffix[bool],
    a9: Suffix[int, str, bool],
    # error: [invalid-type-arguments] "No type argument provided for required type variable `U`"
    a10: Between[int],
):
    reveal_type(a1)  # revealed: tuple[()]
    reveal_type(a2)  # revealed: tuple[int, str]
    reveal_type(a3)  # revealed: tuple[int, str]
    reveal_type(a4)  # revealed: tuple[int, bool, str]
    reveal_type(a5)  # revealed: tuple[int, bool, bytes, str]
    reveal_type(a6)  # revealed: tuple[bool]
    reveal_type(a7)  # revealed: tuple[bool, int, str]
    reveal_type(a8)  # revealed: tuple[bool]
    reveal_type(a9)  # revealed: tuple[int, str, bool]
    reveal_type(a10)  # revealed: tuple[Unknown, *tuple[Unknown, ...], Unknown]
```

### Aliases containing `Never`

A variadic alias retains each specialized argument even when a later argument is `Never`.

```py
from typing import Never

class Container[*Ts]: ...

type Padded[T] = Container[T, Never]

def _(value: Padded[int]) -> None:
    reveal_type(value)  # revealed: Container[int, Never]
```

### Unpacked tuple type arguments

```py
type Alias[*Ts] = tuple[int, *Ts]

def _(a1: Alias[*tuple[str, bool]], a2: Alias[*tuple[str, ...]]) -> None:
    reveal_type(a1)  # revealed: tuple[int, str, bool]
    reveal_type(a2)  # revealed: tuple[int, *tuple[str, ...]]
```

### Unspecified alias type arguments

A bare variadic alias substitutes an unknown-length tuple of `Any`, just like a bare variadic
generic class.

```py
from typing import Any

type Alias[*Fields] = tuple[bytes, *Fields]

def _(a1: Alias, a2: Alias[*tuple[Any, ...]]) -> None:
    reveal_type(a1)  # revealed: tuple[bytes, *tuple[Unknown, ...]]
    reveal_type(a2)  # revealed: tuple[bytes, *tuple[Any, ...]]
```

### Splitting arbitrary-length tuples

```py
type First[*Ts, T] = tuple[*Ts, T]
type Second[T, *Ts] = tuple[T, *Ts]

reveal_type(First[*tuple[int, ...]])  # revealed: <type alias 'First[*tuple[int, ...], int]'>
reveal_type(First[*tuple[int, ...], str])  # revealed: <type alias 'First[*tuple[int, ...], str]'>
reveal_type(Second[*tuple[int, ...]])  # revealed: <type alias 'Second[int, *tuple[int, ...]]'>
reveal_type(Second[str, *tuple[int, ...]])  # revealed: <type alias 'Second[str, *tuple[int, ...]]'>
```

### Variadic substitutions

A variadic alias can forward its remaining arguments to another variadic alias.

```py
type First[*Ts] = tuple[bytes, *Ts]
type Second[*Ts] = First[int, *Ts]

reveal_type(First[str, bool])  # revealed: <type alias 'First[str, bool]'>
reveal_type(Second[str, bool])  # revealed: <type alias 'Second[str, bool]'>
```

### Unsupported union unpacking

Unpacking a type variable tuple into `Union` is currently not supported. We recover to `object`
rather than interpreting the pack as a single union member.

```py
from typing import Union

# TODO: shouldn't error
# error: [invalid-type-form]
type VariadicUnion[*Ts] = Union[*Ts]

def _(value: VariadicUnion[int, str]) -> None:
    reveal_type(value)  # revealed: object
```

### Using Callable

```py
from typing import Callable

type Alias[*Ts] = Callable[[*Ts], None]

def test[*Ts](fn: Alias[int, *Ts]) -> tuple[*Ts]:
    raise NotImplementedError

def fn0(a: int) -> None: ...
def fn1(a: int, b: str) -> None: ...
def fn2(a: int, b: str, c: bytes) -> None: ...

reveal_type(test(fn0))  # revealed: tuple[()]
reveal_type(test(fn1))  # revealed: tuple[str]
reveal_type(test(fn2))  # revealed: tuple[str, bytes]
```

### Indexing and iteration

An unpacked type variable tuple represents the variable-length segment collectively. It is not the
type of each individual element in that segment.

```py
def element_types[*Ts](values: tuple[*Ts]) -> None:
    # TODO: should reveal `Union[*Ts]` representation
    reveal_type(values[0])  # revealed: object

    for value in values:
        # TODO: should reveal `Union[*Ts]` representation
        reveal_type(value)  # revealed: object

    reveal_type(values.__iter__())  # revealed: Iterator[object]
    reveal_type(values * 2)  # revealed: tuple[object, ...]

def boundaries[*Ts](values: tuple[int, *Ts, str]) -> None:
    reveal_type(values[0])  # revealed: int
    reveal_type(values[-1])  # revealed: str
    reveal_type(values[:])  # revealed: tuple[int, *Ts@boundaries, str]

def materialize[*Ts](values: tuple[*Ts]) -> None:
    reveal_type(list(values))  # revealed: list[object]

    runtime_elements: list[object] = list(values)

    # error: [invalid-assignment] "Object of type `list[object]` is not assignable to `list[tuple[object, ...]]`"
    tuple_elements: list[tuple[object, ...]] = list(values)
```

### Slicing

A slice preserves a symbolic pack only when it retains the complete pack in its original order.

```py
def slices[*Ts](values: tuple[*Ts]) -> None:
    reveal_type(values[:])  # revealed: tuple[*Ts@slices]

    reveal_type(values[1:])  # revealed: tuple[object, ...]
    reveal_type(values[::-1])  # revealed: tuple[object, ...]
    reveal_type(values[::2])  # revealed: tuple[object, ...]

def reverse_boundaries[*Ts](values: tuple[int, *Ts, str]) -> None:
    reveal_type(values[::-1])  # revealed: tuple[str, *tuple[object, ...], int]
    reveal_type(values[:0:-1])  # revealed: tuple[str, *tuple[object, ...]]

def trim_boundaries[*Ts](values: tuple[int, *Ts, str]) -> tuple[*Ts]:
    reveal_type(values[1:-1])  # revealed: tuple[*Ts@trim_boundaries]
    return values[1:-1]

def reverse[*Ts](values: tuple[*Ts]) -> tuple[*Ts]:
    # error: [invalid-return-type] "Return type does not match returned value: expected `tuple[*Ts@reverse]`, found `tuple[object, ...]`"
    return values[::-1]

def stride[*Ts](values: tuple[*Ts]) -> tuple[*Ts]:
    # error: [invalid-return-type] "Return type does not match returned value: expected `tuple[*Ts@stride]`, found `tuple[object, ...]`"
    return values[::2]
```

## Accessing Individual Types

Operations that need to rearrange individual members of a type variable tuple can expose overloads
for each supported tuple length.

```py
from typing import Any, overload

class Row[*Cells]:
    @overload
    def get[A, B](self: "Row[A, B]") -> "Row[B, A]": ...
    @overload
    def get[A, B, C](self: "Row[A, B, C]") -> "Row[B, C, A]": ...
    def get(self) -> "Row[*tuple[Any, ...]]":
        raise NotImplementedError

def f(pair: Row[int, str], triple: Row[int, str, bytes]) -> None:
    reveal_type(pair.get())  # revealed: Row[str, int]
    reveal_type(triple.get())  # revealed: Row[str, bytes, int]
```

## Invalid Forms

### Multiple Type Variable Tuples not allowed

Only one type variable tuple can appear in a generic class or type alias type parameter list. Both
can be explicitly specialized, so multiple type variable tuples would make it ambiguous which pack
consumes each type argument.

```py
# error: [invalid-type-form] "Generic class `Array` cannot have multiple `TypeVarTuple` type parameters"
class Array[*Ts1, *Ts2]: ...

# error: [invalid-type-form] "Type alias `Alias` cannot have multiple `TypeVarTuple` type parameters"
type Alias[*Ts1, *Ts2] = tuple[*Ts1] | tuple[*Ts2]
```

### Must always be unpacked

A type variable tuple represents zero or more types, so it cannot be used as a single type.

```py
def invalid[*Ts](x: Ts) -> None: ...  # error: [invalid-type-form]
def invalid_args[*Ts](*args: Ts) -> None: ...  # error: [invalid-type-form]

class InvalidTupleElement[*Ts]:
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts` is not valid in this context in a type expression"
    values: tuple[Ts]

reveal_type(InvalidTupleElement[int, str]().values)  # revealed: tuple[Unknown, ...]

def valid[*Ts](x: tuple[*Ts]) -> tuple[*Ts]:
    return x
```

A bare type variable tuple in a tuple annotation recovers as `*tuple[Unknown, ...]`, preserving any
fixed elements before and after it. Treating the bare pack as one `Unknown` element would
incorrectly impose a fixed length.

```py
# error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
def mixed[*Ts](values: tuple[int, Ts, str]) -> None:
    reveal_type(values)  # revealed: tuple[int, *tuple[Unknown, ...], str]
```

### Missing unpack in a homogeneous tuple

Adding an ellipsis does not make a bare type variable tuple a valid element type. The invalid
specialization recovers to `tuple[Unknown, ...]`.

```py
# error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
def homogeneous[*Ts](values: tuple[Ts, ...]) -> None:
    reveal_type(values)  # revealed: tuple[Unknown, ...]
```

### Missing unpack inside another type

Recovery only affects the bare pack's position in its tuple. An enclosing tuple or `type[]`
annotation keeps its structure. An ordinary tuple with an `Unknown` element keeps its fixed length.

```py
from ty_extensions._internal import Unknown

def nested[*Ts](
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
    values: tuple[tuple[Ts]],
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
    cls: type[tuple[Ts]],
    fixed: tuple[Unknown],
) -> None:
    reveal_type(values)  # revealed: tuple[tuple[Unknown, ...]]
    reveal_type(cls)  # revealed: type[tuple[Unknown, ...]]
    reveal_type(fixed)  # revealed: tuple[Unknown]
```

### Missing unpack in quoted annotations

Quoting the whole tuple annotation or just the bare type variable tuple does not change the
diagnostic or the fallback type.

```py
def quoted[*Ts](
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
    whole: "tuple[Ts]",
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
    element: tuple["Ts"],
) -> None:
    reveal_type(whole)  # revealed: tuple[Unknown, ...]
    reveal_type(element)  # revealed: tuple[Unknown, ...]
```

### Other errors alongside a missing unpack

Recovering from a missing unpack does not prevent us from reporting independent errors in the
remaining tuple elements.

```py
# error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
# error: [unresolved-reference] "Name `Missing` used when not defined"
def invalid_sibling[*Ts](values: tuple[Ts, Missing]) -> None:
    reveal_type(values)  # revealed: tuple[*tuple[Unknown, ...], Unknown]
```

### Missing unpack alongside other variadic elements

A bare type variable tuple alongside a valid variadic unpack or another bare pack reports only the
missing-unpack errors, without a cascading multiple-unpack error.

```py
def other_variadic[*Ts](
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
    before: tuple[Ts, *tuple[int, ...]],
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
    after: tuple[*tuple[int, ...], Ts],
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
    # error: [invalid-type-form] "Bare TypeVarTuple `Ts`"
    repeated: tuple[Ts, Ts],
) -> None: ...
```

### Invalid unpack operand

Only tuple types and type variable tuples can be unpacked in a type expression.

```py
# error: [invalid-type-form] "`*` can only unpack a tuple type or `TypeVarTuple`"
def invalid(*args: *int) -> None:
    reveal_type(args)  # revealed: tuple[Unknown, ...]

class Pair[*Ts, U]: ...

def invalid_generic(
    # error: [invalid-type-form] "`*` can only unpack a tuple type or `TypeVarTuple`"
    value: Pair[*int, str],
) -> None:
    reveal_type(value)  # revealed: Pair[*tuple[Unknown, ...], str]
```

### Only one variadic unpack

```py
def f[*Ts](
    ok1: tuple[int, *Ts],
    ok2: tuple[int, *Ts, str],
    bad1: tuple[*Ts, *tuple[str, ...]],  # error: [invalid-type-form]
    bad2: tuple[*tuple[str, ...], *Ts],  # error: [invalid-type-form]
) -> None: ...
```
