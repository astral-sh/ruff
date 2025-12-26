# Assignable-to relation

```toml
[environment]
python-version = "3.12"
```

The `is_assignable_to(S, T)` relation below checks if type `S` is assignable to type `T` (target).
This allows us to check if a type `S` can be used in a context where a type `T` is expected
(function arguments, variable assignments). See the [typing documentation] for a precise definition
of this concept.

## Basic types

### Fully static

Fully static types participate in subtyping. If a type `S` is a subtype of `T`, `S` will also be
assignable to `T`. Two equivalent types are subtypes of each other:

```py
from ty_extensions import static_assert, is_assignable_to

class Parent: ...
class Child1(Parent): ...
class Child2(Parent): ...
class Grandchild(Child1, Child2): ...
class Unrelated: ...

static_assert(is_assignable_to(int, int))
static_assert(is_assignable_to(Parent, Parent))
static_assert(is_assignable_to(Child1, Parent))
static_assert(is_assignable_to(Grandchild, Parent))
static_assert(is_assignable_to(Unrelated, Unrelated))

static_assert(not is_assignable_to(str, int))
static_assert(not is_assignable_to(object, int))
static_assert(not is_assignable_to(Parent, Child1))
static_assert(not is_assignable_to(Unrelated, Parent))
static_assert(not is_assignable_to(Child1, Child2))
```

### Gradual types

The dynamic type is assignable to or from any type.

```py
from ty_extensions import static_assert, is_assignable_to, Unknown
from typing import Any, Literal

static_assert(is_assignable_to(Unknown, Literal[1]))
static_assert(is_assignable_to(Any, Literal[1]))
static_assert(is_assignable_to(Literal[1], Unknown))
static_assert(is_assignable_to(Literal[1], Any))
```

## Literal types

### Boolean literals

`Literal[True]` and `Literal[False]` are both subtypes of (and therefore assignable to) `bool`,
which is in turn a subtype of `int`:

```py
from ty_extensions import static_assert, is_assignable_to
from typing import Literal

static_assert(is_assignable_to(Literal[True], Literal[True]))
static_assert(is_assignable_to(Literal[True], bool))
static_assert(is_assignable_to(Literal[True], int))

static_assert(not is_assignable_to(Literal[True], Literal[False]))
static_assert(not is_assignable_to(bool, Literal[True]))
```

### Integer literals

```py
from ty_extensions import static_assert, is_assignable_to
from typing import Literal

static_assert(is_assignable_to(Literal[1], Literal[1]))
static_assert(is_assignable_to(Literal[1], int))

static_assert(not is_assignable_to(Literal[1], Literal[2]))
static_assert(not is_assignable_to(int, Literal[1]))
static_assert(not is_assignable_to(Literal[1], str))
```

### String literals and `LiteralString`

All string-literal types are subtypes of (and therefore assignable to) `LiteralString`, which is in
turn a subtype of `str`:

```py
from ty_extensions import static_assert, is_assignable_to
from typing_extensions import Literal, LiteralString
from typing import Sequence, Any

static_assert(is_assignable_to(Literal["foo"], Literal["foo"]))
static_assert(is_assignable_to(Literal["foo"], LiteralString))
static_assert(is_assignable_to(Literal["foo"], str))
static_assert(is_assignable_to(Literal["foo"], Sequence))
static_assert(is_assignable_to(Literal["foo"], Sequence[str]))
static_assert(is_assignable_to(Literal["foo"], Sequence[Any]))

static_assert(is_assignable_to(LiteralString, str))
static_assert(is_assignable_to(LiteralString, Sequence))
static_assert(is_assignable_to(LiteralString, Sequence[str]))
static_assert(is_assignable_to(LiteralString, Sequence[Any]))

static_assert(not is_assignable_to(Literal["foo"], Literal["bar"]))
static_assert(not is_assignable_to(str, Literal["foo"]))
static_assert(not is_assignable_to(str, LiteralString))
```

### Byte literals

```py
from ty_extensions import static_assert, is_assignable_to
from typing_extensions import Literal, LiteralString

static_assert(is_assignable_to(Literal[b"foo"], bytes))
static_assert(is_assignable_to(Literal[b"foo"], Literal[b"foo"]))

static_assert(not is_assignable_to(Literal[b"foo"], str))
static_assert(not is_assignable_to(Literal[b"foo"], LiteralString))
static_assert(not is_assignable_to(Literal[b"foo"], Literal[b"bar"]))
static_assert(not is_assignable_to(Literal[b"foo"], Literal["foo"]))
static_assert(not is_assignable_to(Literal["foo"], Literal[b"foo"]))
```

### Enum literals

```py
from ty_extensions import static_assert, is_assignable_to
from typing_extensions import Literal
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

static_assert(is_assignable_to(Literal[Answer.YES], Literal[Answer.YES]))
static_assert(is_assignable_to(Literal[Answer.YES], Answer))
static_assert(is_assignable_to(Literal[Answer.YES, Answer.NO], Answer))
static_assert(is_assignable_to(Answer, Literal[Answer.YES, Answer.NO]))

static_assert(not is_assignable_to(Literal[Answer.YES], Literal[Answer.NO]))

class Single(Enum):
    VALUE = 1

static_assert(is_assignable_to(Literal[Single.VALUE], Single))
static_assert(is_assignable_to(Single, Literal[Single.VALUE]))
```

### Slice literals

The type of a slice literal is currently inferred as a specialization of `slice`.

```py
from ty_extensions import TypeOf, is_assignable_to, static_assert

static_assert(is_assignable_to(TypeOf[1:2:3], slice))
static_assert(is_assignable_to(TypeOf[1:2:3], slice[int]))
```

## `type[…]` and class literals

In the following tests, `TypeOf[str]` is a singleton type with a single inhabitant, the class `str`.
This contrasts with `type[str]`, which represents "all possible subclasses of `str`".

Both `TypeOf[str]` and `type[str]` are subtypes of `type` and `type[object]`, which both represent
"all possible instances of `type`"; therefore both `type[str]` and `TypeOf[str]` are assignable to
`type`. `type[Any]`, on the other hand, represents a type of unknown size or inhabitants, but which
is known to be no larger than the set of possible objects represented by `type`.

```py
from ty_extensions import static_assert, is_assignable_to, Unknown, TypeOf
from typing import Any

static_assert(is_assignable_to(type, type))
static_assert(is_assignable_to(type[object], type[object]))

static_assert(is_assignable_to(type, type[object]))
static_assert(is_assignable_to(type[object], type))

static_assert(is_assignable_to(type[str], type[object]))
static_assert(is_assignable_to(TypeOf[str], type[object]))
static_assert(is_assignable_to(type[str], type))
static_assert(is_assignable_to(TypeOf[str], type))

static_assert(is_assignable_to(type[str], type[str]))
static_assert(is_assignable_to(TypeOf[str], type[str]))

static_assert(not is_assignable_to(TypeOf[int], type[str]))
static_assert(not is_assignable_to(type, type[str]))
static_assert(not is_assignable_to(type[object], type[str]))

static_assert(is_assignable_to(type[Any], type[Any]))
static_assert(is_assignable_to(type[Any], type[object]))
static_assert(is_assignable_to(type[object], type[Any]))
static_assert(is_assignable_to(type, type[Any]))
static_assert(is_assignable_to(type[Any], type[str]))
static_assert(is_assignable_to(type[str], type[Any]))
static_assert(is_assignable_to(TypeOf[str], type[Any]))

static_assert(is_assignable_to(type[Unknown], type[Unknown]))
static_assert(is_assignable_to(type[Unknown], type[object]))
static_assert(is_assignable_to(type[object], type[Unknown]))
static_assert(is_assignable_to(type, type[Unknown]))
static_assert(is_assignable_to(type[Unknown], type[str]))
static_assert(is_assignable_to(type[str], type[Unknown]))
static_assert(is_assignable_to(TypeOf[str], type[Unknown]))

static_assert(is_assignable_to(type[Unknown], type[Any]))
static_assert(is_assignable_to(type[Any], type[Unknown]))

static_assert(not is_assignable_to(object, type[Any]))
static_assert(not is_assignable_to(str, type[Any]))

class Meta(type): ...

static_assert(is_assignable_to(type[Any], Meta))
static_assert(is_assignable_to(type[Unknown], Meta))
static_assert(is_assignable_to(Meta, type[Any]))
static_assert(is_assignable_to(Meta, type[Unknown]))

def _(x: Any):
    class AnyMeta(metaclass=x): ...
    static_assert(is_assignable_to(type[AnyMeta], type))
    static_assert(is_assignable_to(type[AnyMeta], type[object]))
    static_assert(is_assignable_to(type[AnyMeta], type[Any]))

from typing import TypeVar, Generic, Any

T_co = TypeVar("T_co", covariant=True)

class Foo(Generic[T_co]): ...
class Bar(Foo[T_co], Generic[T_co]): ...

static_assert(is_assignable_to(TypeOf[Bar[int]], type[Foo[int]]))
static_assert(is_assignable_to(TypeOf[Bar[bool]], type[Foo[int]]))
static_assert(is_assignable_to(TypeOf[Bar], type[Foo[int]]))
static_assert(is_assignable_to(TypeOf[Bar[Any]], type[Foo[int]]))
static_assert(is_assignable_to(TypeOf[Bar[Unknown]], type[Foo[int]]))
static_assert(is_assignable_to(TypeOf[Bar], type[Foo]))
static_assert(is_assignable_to(TypeOf[Bar[Any]], type[Foo[Any]]))
static_assert(is_assignable_to(TypeOf[Bar[Any]], type[Foo[int]]))

static_assert(not is_assignable_to(TypeOf[Bar[int]], type[Foo[bool]]))
static_assert(not is_assignable_to(TypeOf[Foo[bool]], type[Bar[int]]))
```

## `type[]` is not assignable to types disjoint from `builtins.type`

```py
from typing import Any
from ty_extensions import is_assignable_to, static_assert

static_assert(not is_assignable_to(type[Any], None))
```

## Inheriting `Any`

### Class-literal types

Class-literal types that inherit from `Any` are assignable to any type `T` where `T` is assignable
to `type`:

```py
from typing import Any
from ty_extensions import is_assignable_to, static_assert, TypeOf

def test(x: Any):
    class Foo(x): ...
    class Bar(Any): ...
    static_assert(is_assignable_to(TypeOf[Foo], Any))
    static_assert(is_assignable_to(TypeOf[Foo], type))
    static_assert(is_assignable_to(TypeOf[Foo], type[int]))
    static_assert(is_assignable_to(TypeOf[Foo], type[Any]))

    static_assert(is_assignable_to(TypeOf[Bar], Any))
    static_assert(is_assignable_to(TypeOf[Bar], type))
    static_assert(is_assignable_to(TypeOf[Bar], type[int]))
    static_assert(is_assignable_to(TypeOf[Bar], type[Any]))

    static_assert(not is_assignable_to(TypeOf[Foo], int))
    static_assert(not is_assignable_to(TypeOf[Bar], int))
```

This is because the `Any` element in the MRO could materialize to any subtype of `type`.

### Nominal instance and subclass-of types

Instances of classes that inherit `Any` are assignable to any non-final type.

```py
from ty_extensions import is_assignable_to, static_assert
from typing_extensions import Any, final

class InheritsAny(Any):
    pass

class Arbitrary:
    pass

@final
class FinalClass:
    pass

static_assert(is_assignable_to(InheritsAny, Arbitrary))
static_assert(is_assignable_to(InheritsAny, Any))
static_assert(is_assignable_to(InheritsAny, object))
static_assert(not is_assignable_to(InheritsAny, FinalClass))
```

Similar for subclass-of types:

```py
static_assert(is_assignable_to(type[Any], type[Any]))
static_assert(is_assignable_to(type[object], type[Any]))
static_assert(is_assignable_to(type[Any], type[Arbitrary]))
static_assert(is_assignable_to(type[Any], type[object]))
```

## Heterogeneous tuple types

```py
from ty_extensions import static_assert, is_assignable_to, AlwaysTruthy, AlwaysFalsy
from typing import Literal, Any

static_assert(is_assignable_to(tuple[()], tuple[()]))
static_assert(is_assignable_to(tuple[int], tuple[int]))
static_assert(is_assignable_to(tuple[int], tuple[Any]))
static_assert(is_assignable_to(tuple[Any], tuple[int]))
static_assert(is_assignable_to(tuple[int, str], tuple[int, str]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[int, int]))
static_assert(is_assignable_to(tuple[Any, Literal[2]], tuple[int, int]))
static_assert(is_assignable_to(tuple[Literal[1], Any], tuple[int, int]))
static_assert(is_assignable_to(tuple[()], tuple))
static_assert(is_assignable_to(tuple[int, str], tuple))
static_assert(is_assignable_to(tuple[Any], tuple))

# TODO: It is not yet clear if we want the following two assertions to hold.
# See https://github.com/astral-sh/ruff/issues/15528 for more details. The
# short version is: We either need to special-case enforcement of the Liskov
# substitution principle on `__bool__` and `__len__` for tuple subclasses,
# or we need to negate these assertions.
static_assert(is_assignable_to(tuple[()], AlwaysFalsy))
static_assert(is_assignable_to(tuple[int], AlwaysTruthy))

static_assert(not is_assignable_to(tuple[()], tuple[int]))
static_assert(not is_assignable_to(tuple[int], tuple[str]))
static_assert(not is_assignable_to(tuple[int], tuple[int, str]))
static_assert(not is_assignable_to(tuple[int, str], tuple[int]))
static_assert(not is_assignable_to(tuple[int, int], tuple[Literal[1], int]))
static_assert(not is_assignable_to(tuple[Any, Literal[2]], tuple[int, str]))
```

## Assignability of heterogeneous tuple types to homogeneous tuple types

```toml
[environment]
python-version = "3.12"
```

While a homogeneous tuple type is not assignable to any heterogeneous tuple types, a heterogeneous
tuple type can be assignable to a homogeneous tuple type, and homogeneous tuple types can be
assignable to `Sequence`:

```py
from typing import Literal, Any, Sequence
from ty_extensions import static_assert, is_assignable_to, Not, AlwaysFalsy

static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[Literal[1, 2], ...]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[Literal[1], *tuple[Literal[2], ...]]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[*tuple[Literal[1], ...], Literal[2]]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[Literal[1], *tuple[str, ...], Literal[2]]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[Literal[1], Literal[2], *tuple[str, ...]]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[*tuple[str, ...], Literal[1], Literal[2]]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[int, ...]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[int | str, ...]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[Any, ...]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[Not[AlwaysFalsy], ...]))
static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], Sequence[int]))
static_assert(is_assignable_to(tuple[int, ...], Sequence[int]))
static_assert(is_assignable_to(tuple[int, ...], Sequence[Any]))
static_assert(is_assignable_to(tuple[Any, ...], Sequence[int]))

static_assert(is_assignable_to(tuple[()], tuple[Literal[1, 2], ...]))
static_assert(is_assignable_to(tuple[()], tuple[int, ...]))
static_assert(is_assignable_to(tuple[()], tuple[int | str, ...]))
static_assert(is_assignable_to(tuple[()], tuple[Not[AlwaysFalsy], ...]))
static_assert(is_assignable_to(tuple[()], Sequence[int]))

static_assert(not is_assignable_to(tuple[int, int], tuple[str, ...]))
```

## Assignability of two mixed tuple types

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Literal, Any, Sequence
from ty_extensions import static_assert, is_assignable_to, Not, AlwaysFalsy

static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[10]],
    )
)
static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...]],
    )
)

static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], *tuple[int, ...], Literal[10]],
    )
)
static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], *tuple[int, ...]],
    )
)

static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[*tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[*tuple[int, ...], Literal[10]],
    )
)
static_assert(
    is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[*tuple[int, ...]],
    )
)

static_assert(
    not is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_assignable_to(
        tuple[Literal[1], Literal[2], *tuple[int, ...]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)

static_assert(
    not is_assignable_to(
        tuple[Literal[1], *tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_assignable_to(
        tuple[Literal[1], *tuple[int, ...], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_assignable_to(
        tuple[Literal[1], *tuple[int, ...]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)

static_assert(
    not is_assignable_to(
        tuple[*tuple[int, ...], Literal[9], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_assignable_to(
        tuple[*tuple[int, ...], Literal[10]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
static_assert(
    not is_assignable_to(
        tuple[*tuple[int, ...]],
        tuple[Literal[1], Literal[2], *tuple[int, ...], Literal[9], Literal[10]],
    )
)
```

## Assignability of the gradual tuple

```toml
[environment]
python-version = "3.12"
```

As a [special case][gradual tuple], `tuple[Any, ...]` is a [gradual][gradual form] tuple type, which
is assignable to every tuple of any length.

```py
from typing import Any
from ty_extensions import static_assert, is_assignable_to

static_assert(is_assignable_to(tuple[Any, ...], tuple[Any, ...]))
static_assert(is_assignable_to(tuple[Any, ...], tuple[Any]))
static_assert(is_assignable_to(tuple[Any, ...], tuple[Any, Any]))
static_assert(is_assignable_to(tuple[Any, ...], tuple[int, ...]))
static_assert(is_assignable_to(tuple[Any, ...], tuple[int]))
static_assert(is_assignable_to(tuple[Any, ...], tuple[int, int]))
static_assert(is_assignable_to(tuple[Any, ...], tuple[int, *tuple[int, ...]]))
static_assert(is_assignable_to(tuple[Any, ...], tuple[*tuple[int, ...], int]))
static_assert(is_assignable_to(tuple[Any, ...], tuple[int, *tuple[int, ...], int]))
```

This also applies when `tuple[Any, ...]` is unpacked into a mixed tuple.

```py
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], tuple[int, *tuple[Any, ...]]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], tuple[Any, ...]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], tuple[Any]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], tuple[Any, Any]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], tuple[int, *tuple[int, ...]]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], tuple[int, ...]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], tuple[int]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...]], tuple[int, int]))

static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], tuple[*tuple[Any, ...], int]))
static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], tuple[Any, ...]))
static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], tuple[Any]))
static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], tuple[Any, Any]))
static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], tuple[*tuple[int, ...], int]))
static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], tuple[int, ...]))
static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], tuple[int]))
static_assert(is_assignable_to(tuple[*tuple[Any, ...], int], tuple[int, int]))

# `*tuple[Any, ...]` can materialize to a tuple of any length as a special case,
# so this passes:
static_assert(is_assignable_to(tuple[*tuple[Any, ...], Any], tuple[*tuple[Any, ...], Any, Any]))

static_assert(is_assignable_to(tuple[int, *tuple[Any, ...], int], tuple[int, *tuple[Any, ...], int]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...], int], tuple[Any, ...]))
static_assert(not is_assignable_to(tuple[int, *tuple[Any, ...], int], tuple[Any]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...], int], tuple[Any, Any]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...], int], tuple[int, *tuple[int, ...], int]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...], int], tuple[int, ...]))
static_assert(not is_assignable_to(tuple[int, *tuple[Any, ...], int], tuple[int]))
static_assert(is_assignable_to(tuple[int, *tuple[Any, ...], int], tuple[int, int]))
```

The same is not true of fully static tuple types, since an unbounded homogeneous tuple is defined to
be the _union_ of all tuple lengths, not the _gradual choice_ of them.

```py
static_assert(is_assignable_to(tuple[int, ...], tuple[Any, ...]))
static_assert(not is_assignable_to(tuple[int, ...], tuple[Any]))
static_assert(not is_assignable_to(tuple[int, ...], tuple[Any, Any]))
static_assert(is_assignable_to(tuple[int, ...], tuple[int, ...]))
static_assert(not is_assignable_to(tuple[int, ...], tuple[int]))
static_assert(not is_assignable_to(tuple[int, ...], tuple[int, int]))
static_assert(not is_assignable_to(tuple[int, ...], tuple[int, *tuple[int, ...]]))
static_assert(not is_assignable_to(tuple[int, ...], tuple[*tuple[int, ...], int]))
static_assert(not is_assignable_to(tuple[int, ...], tuple[int, *tuple[int, ...], int]))

static_assert(is_assignable_to(tuple[int, *tuple[int, ...]], tuple[int, *tuple[Any, ...]]))
static_assert(is_assignable_to(tuple[int, *tuple[int, ...]], tuple[Any, ...]))
static_assert(not is_assignable_to(tuple[int, *tuple[int, ...]], tuple[Any]))
static_assert(not is_assignable_to(tuple[int, *tuple[int, ...]], tuple[Any, Any]))
static_assert(is_assignable_to(tuple[int, *tuple[int, ...]], tuple[int, *tuple[int, ...]]))
static_assert(is_assignable_to(tuple[int, *tuple[int, ...]], tuple[int, ...]))
static_assert(not is_assignable_to(tuple[int, *tuple[int, ...]], tuple[int]))
static_assert(not is_assignable_to(tuple[int, *tuple[int, ...]], tuple[int, int]))

static_assert(is_assignable_to(tuple[*tuple[int, ...], int], tuple[*tuple[Any, ...], int]))
static_assert(is_assignable_to(tuple[*tuple[int, ...], int], tuple[Any, ...]))
static_assert(not is_assignable_to(tuple[*tuple[int, ...], int], tuple[Any]))
static_assert(not is_assignable_to(tuple[*tuple[int, ...], int], tuple[Any, Any]))
static_assert(is_assignable_to(tuple[*tuple[int, ...], int], tuple[*tuple[int, ...], int]))
static_assert(is_assignable_to(tuple[*tuple[int, ...], int], tuple[int, ...]))
static_assert(not is_assignable_to(tuple[*tuple[int, ...], int], tuple[int]))
static_assert(not is_assignable_to(tuple[*tuple[int, ...], int], tuple[int, int]))

static_assert(is_assignable_to(tuple[int, *tuple[int, ...], int], tuple[int, *tuple[Any, ...], int]))
static_assert(is_assignable_to(tuple[int, *tuple[int, ...], int], tuple[Any, ...]))
static_assert(not is_assignable_to(tuple[int, *tuple[int, ...], int], tuple[Any]))
static_assert(not is_assignable_to(tuple[int, *tuple[int, ...], int], tuple[Any, Any]))
static_assert(is_assignable_to(tuple[int, *tuple[int, ...], int], tuple[int, *tuple[int, ...], int]))
static_assert(is_assignable_to(tuple[int, *tuple[int, ...], int], tuple[int, ...]))
static_assert(not is_assignable_to(tuple[int, *tuple[int, ...], int], tuple[int]))
static_assert(not is_assignable_to(tuple[int, *tuple[int, ...], int], tuple[int, int]))
```

## Union types

```py
from ty_extensions import AlwaysTruthy, AlwaysFalsy, static_assert, is_assignable_to, Unknown
from typing_extensions import Literal, Any, LiteralString

static_assert(is_assignable_to(int, int | str))
static_assert(is_assignable_to(str, int | str))
static_assert(is_assignable_to(int | str, int | str))
static_assert(is_assignable_to(str | int, int | str))
static_assert(is_assignable_to(Literal[1], int | str))
static_assert(is_assignable_to(Literal[1], Unknown | str))
static_assert(is_assignable_to(Literal[1] | Literal[2], Literal[1] | Literal[2]))
static_assert(is_assignable_to(Literal[1] | Literal[2], int))
static_assert(is_assignable_to(Literal[1] | None, int | None))
static_assert(is_assignable_to(Any, int | str))
static_assert(is_assignable_to(Any | int, int))
static_assert(is_assignable_to(str, int | Any))

static_assert(not is_assignable_to(int | None, int))
static_assert(not is_assignable_to(int | None, str | None))
static_assert(not is_assignable_to(Literal[1] | None, int))
static_assert(not is_assignable_to(Literal[1] | None, str | None))
static_assert(not is_assignable_to(Any | int | str, int))

# TODO: No errors
# error: [static-assert-error]
static_assert(is_assignable_to(bool, Literal[False] | AlwaysTruthy))
# error: [static-assert-error]
static_assert(is_assignable_to(bool, Literal[True] | AlwaysFalsy))
# error: [static-assert-error]
static_assert(is_assignable_to(LiteralString, Literal[""] | AlwaysTruthy))
static_assert(not is_assignable_to(Literal[True] | AlwaysFalsy, Literal[False] | AlwaysTruthy))
```

## Intersection types

```py
from ty_extensions import static_assert, is_assignable_to, Intersection, Not, AlwaysTruthy, AlwaysFalsy
from typing_extensions import Any, Literal, final, LiteralString

class Parent: ...
class Child1(Parent): ...
class Child2(Parent): ...
class Grandchild(Child1, Child2): ...
class Unrelated: ...

static_assert(is_assignable_to(Intersection[Child1, Child2], Child1))
static_assert(is_assignable_to(Intersection[Child1, Child2], Child2))
static_assert(is_assignable_to(Intersection[Child1, Child2], Parent))
static_assert(is_assignable_to(Intersection[Child1, Parent], Parent))

static_assert(is_assignable_to(Intersection[Parent, Unrelated], Parent))
static_assert(is_assignable_to(Intersection[Child1, Unrelated], Child1))
static_assert(is_assignable_to(Intersection[Child1, Unrelated, Child2], Intersection[Child1, Unrelated]))

static_assert(is_assignable_to(Intersection[Child1, Not[Child2]], Child1))
static_assert(is_assignable_to(Intersection[Child1, Not[Child2]], Parent))
static_assert(is_assignable_to(Intersection[Child1, Not[Grandchild]], Parent))

static_assert(is_assignable_to(Intersection[Child1, Child2], Intersection[Child1, Child2]))
static_assert(is_assignable_to(Intersection[Child1, Child2], Intersection[Child2, Child1]))
static_assert(is_assignable_to(Grandchild, Intersection[Child1, Child2]))
static_assert(not is_assignable_to(Intersection[Child1, Child2], Intersection[Parent, Unrelated]))

static_assert(not is_assignable_to(Parent, Intersection[Parent, Unrelated]))
static_assert(not is_assignable_to(int, Intersection[int, Not[Literal[1]]]))
# The literal `1` is not assignable to `Parent`, so the intersection of int and Parent is definitely an int that is not `1`
static_assert(is_assignable_to(Intersection[int, Parent], Intersection[int, Not[Literal[1]]]))
static_assert(not is_assignable_to(int, Not[int]))
static_assert(not is_assignable_to(int, Not[Literal[1]]))

static_assert(is_assignable_to(Not[Parent], Not[Child1]))
static_assert(not is_assignable_to(Not[Parent], Parent))
static_assert(not is_assignable_to(Intersection[Unrelated, Not[Parent]], Parent))

# Intersection with `Any` dominates the left hand side of intersections
static_assert(is_assignable_to(Intersection[Any, Parent], Parent))
static_assert(is_assignable_to(Intersection[Any, Child1], Parent))
static_assert(is_assignable_to(Intersection[Any, Child2, Not[Child1]], Parent))
static_assert(is_assignable_to(Intersection[Any, Parent], Unrelated))
static_assert(is_assignable_to(Intersection[Any, Parent], Intersection[Parent, Unrelated]))
static_assert(is_assignable_to(Intersection[Any, Parent, Unrelated], Parent))
static_assert(is_assignable_to(Intersection[Any, Parent, Unrelated], Intersection[Parent, Unrelated]))

# Even Any & Not[Parent] is assignable to Parent, since it could be Never
static_assert(is_assignable_to(Intersection[Any, Not[Parent]], Parent))
static_assert(is_assignable_to(Intersection[Any, Not[Parent]], Not[Parent]))

# Intersection with `Any` is effectively ignored on the right hand side for the sake of assignment
static_assert(is_assignable_to(Parent, Intersection[Any, Parent]))
static_assert(is_assignable_to(Parent, Parent | Intersection[Any, Unrelated]))
static_assert(is_assignable_to(Child1, Intersection[Any, Parent]))
static_assert(not is_assignable_to(Literal[1], Intersection[Any, Parent]))
static_assert(not is_assignable_to(Unrelated, Intersection[Any, Parent]))

# Intersections with Any on both sides combine the above logic - the LHS dominates and Any is ignored on the right hand side
static_assert(is_assignable_to(Intersection[Any, Parent], Intersection[Any, Parent]))
static_assert(is_assignable_to(Intersection[Any, Unrelated], Intersection[Any, Parent]))
static_assert(is_assignable_to(Intersection[Any, Parent, Unrelated], Intersection[Any, Parent, Unrelated]))
static_assert(is_assignable_to(Intersection[Unrelated, Any], Intersection[Unrelated, Not[Any]]))
static_assert(is_assignable_to(Intersection[Literal[1], Any], Intersection[Unrelated, Not[Any]]))

# TODO: No errors
# The condition `is_assignable_to(T & U, U)` should still be satisfied after the following transformations:
# `LiteralString & AlwaysTruthy` -> `LiteralString & ~Literal[""]`
# error: [static-assert-error]
static_assert(is_assignable_to(Intersection[LiteralString, Not[Literal[""]]], AlwaysTruthy))
# error: [static-assert-error]
static_assert(is_assignable_to(Intersection[LiteralString, Not[Literal["", "a"]]], AlwaysTruthy))
# `LiteralString & ~AlwaysFalsy`  -> `LiteralString & ~Literal[""]`
# error: [static-assert-error]
static_assert(is_assignable_to(Intersection[LiteralString, Not[Literal[""]]], Not[AlwaysFalsy]))
# error: [static-assert-error]
static_assert(is_assignable_to(Intersection[LiteralString, Not[Literal["", "a"]]], Not[AlwaysFalsy]))
```

## Intersections with non-fully-static negated elements

A type can be _assignable_ to an intersection containing negated elements only if the _bottom_
materialization of that type is disjoint from the _bottom_ materialization of all negated elements
in the intersection. This differs from subtyping, which should do the disjointness check against the
_top_ materialization of the negated elements.

```py
from typing_extensions import Any, Never, Sequence
from ty_extensions import Not, is_assignable_to, static_assert

# The bottom materialization of `tuple[Any]` is `tuple[Never]`,
# which simplifies to `Never`, so `tuple[int]` and `tuple[()]` are
# both assignable to `~tuple[Any]`
static_assert(is_assignable_to(tuple[int], Not[tuple[Any]]))
static_assert(is_assignable_to(tuple[()], Not[tuple[Any]]))

# But the bottom materialization of `tuple[Any, ...]` is `tuple[Never, ...]`,
# which simplifies to `tuple[()]`, so `tuple[int]` is still assignable to
# `~tuple[Any, ...]`, but `tuple[()]` is not
static_assert(is_assignable_to(tuple[int], Not[tuple[Any, ...]]))
static_assert(not is_assignable_to(tuple[()], Not[tuple[Any, ...]]))

# Similarly, the bottom materialization of `Sequence[Any]` is `Sequence[Never]`,
# so `tuple[()]` is not assignable to `~Sequence[Any]`, and nor is `list[Never]`,
# since both `tuple[()]` and `list[Never]` are subtypes of `Sequence[Never]`.
# `tuple[int, ...]` is also not assignable to `~Sequence[Any]`, as although it is
# not a subtype of `Sequence[Never]` it is also not disjoint from `Sequence[Never]`:
# `tuple[()]` is a subtype of both `Sequence[Never]` and `tuple[int, ...]`, so
# `tuple[int, ...]` and `Sequence[Never]` cannot be considered disjoint.
#
# Other `list` and `tuple` specializations *are* assignable to `~Sequence[Any]`,
# however, since there are many fully static materializations of `Sequence[Any]`
# that would be disjoint from a given `list` or `tuple` specialization.
static_assert(not is_assignable_to(tuple[()], Not[Sequence[Any]]))
static_assert(not is_assignable_to(list[Never], Not[Sequence[Any]]))
static_assert(not is_assignable_to(tuple[int, ...], Not[Sequence[Any]]))

# TODO: should pass (`tuple[int]` should be considered disjoint from `Sequence[Never]`)
static_assert(is_assignable_to(tuple[int], Not[Sequence[Any]]))  # error: [static-assert-error]

# TODO: should pass (`list[int]` should be considered disjoint from `Sequence[Never]`)
static_assert(is_assignable_to(list[int], Not[Sequence[Any]]))  # error: [static-assert-error]

# If the left-hand side is also not fully static,
# the left-hand side will be assignable to the right if the bottom materialization
# of the left-hand side is disjoint from the bottom materialization of all negated
# elements on the right-hand side

# `tuple[Any, ...]` cannot be assignable to `~tuple[Any, ...]`,
# because the bottom materialization of `tuple[Any, ...]` is
# `tuple[()]`, and `tuple[()]` is not disjoint from itself
static_assert(not is_assignable_to(tuple[Any, ...], Not[tuple[Any, ...]]))

# but `tuple[Any]` is assignable to `~tuple[Any]`,
# as the bottom materialization of `tuple[Any]` is `Never`,
# and `Never` *is* disjoint from itself
static_assert(is_assignable_to(tuple[Any], Not[tuple[Any]]))

# The same principle applies for non-fully-static `list` specializations.
# TODO: this should pass (`Bottom[list[Any]]` should simplify to `Never`)
static_assert(is_assignable_to(list[Any], Not[list[Any]]))  # error: [static-assert-error]

# `Bottom[list[Any]]` is `Never`, which is disjoint from `Bottom[Sequence[Any]]`
# (which is `Sequence[Never]`).
# TODO: this should pass (`Bottom[list[Any]]` should simplify to `Never`)
static_assert(is_assignable_to(list[Any], Not[Sequence[Any]]))  # error: [static-assert-error]
```

## General properties

See also: our property tests in `property_tests.rs`.

### Everything is assignable to `object`

`object` is Python's top type; the set of all possible objects at runtime:

```py
from ty_extensions import static_assert, is_assignable_to, Unknown
from typing import Literal, Any

static_assert(is_assignable_to(str, object))
static_assert(is_assignable_to(Literal[1], object))
static_assert(is_assignable_to(object, object))
static_assert(is_assignable_to(type, object))
static_assert(is_assignable_to(Any, object))
static_assert(is_assignable_to(Unknown, object))
static_assert(is_assignable_to(type[object], object))
static_assert(is_assignable_to(type[str], object))
static_assert(is_assignable_to(type[Any], object))
```

### Every type is assignable to `Any` / `Unknown`

`Any` and `Unknown` are gradual types. They could materialize to any given type at runtime, and so
any type is assignable to them:

```py
from ty_extensions import static_assert, is_assignable_to, Unknown
from typing import Literal, Any

static_assert(is_assignable_to(str, Any))
static_assert(is_assignable_to(Literal[1], Any))
static_assert(is_assignable_to(object, Any))
static_assert(is_assignable_to(type, Any))
static_assert(is_assignable_to(Any, Any))
static_assert(is_assignable_to(Unknown, Any))
static_assert(is_assignable_to(type[object], Any))
static_assert(is_assignable_to(type[str], Any))
static_assert(is_assignable_to(type[Any], Any))

static_assert(is_assignable_to(str, Unknown))
static_assert(is_assignable_to(Literal[1], Unknown))
static_assert(is_assignable_to(object, Unknown))
static_assert(is_assignable_to(type, Unknown))
static_assert(is_assignable_to(Any, Unknown))
static_assert(is_assignable_to(Unknown, Unknown))
static_assert(is_assignable_to(type[object], Unknown))
static_assert(is_assignable_to(type[str], Unknown))
static_assert(is_assignable_to(type[Any], Unknown))
```

### `Never` is assignable to every type

`Never` is Python's bottom type: the empty set, a type with no inhabitants. It is therefore
assignable to any arbitrary type.

```py
from ty_extensions import static_assert, is_assignable_to, Unknown
from typing_extensions import Never, Any, Literal

static_assert(is_assignable_to(Never, str))
static_assert(is_assignable_to(Never, Literal[1]))
static_assert(is_assignable_to(Never, object))
static_assert(is_assignable_to(Never, type))
static_assert(is_assignable_to(Never, Any))
static_assert(is_assignable_to(Never, Unknown))
static_assert(is_assignable_to(Never, type[object]))
static_assert(is_assignable_to(Never, type[str]))
static_assert(is_assignable_to(Never, type[Any]))
```

## Callable

The examples provided below are only a subset of the possible cases and include the ones with
gradual types. The cases with fully static types and using different combinations of parameter kinds
are covered in the [subtyping tests](./is_subtype_of.md#callable).

### Return type

```py
from ty_extensions import CallableTypeOf, Unknown, static_assert, is_assignable_to
from typing import Any, Callable

static_assert(is_assignable_to(Callable[[], Any], Callable[[], int]))
static_assert(is_assignable_to(Callable[[], int], Callable[[], Any]))

static_assert(is_assignable_to(Callable[[], int], Callable[[], float]))
static_assert(not is_assignable_to(Callable[[], float], Callable[[], int]))
```

The return types should be checked even if the parameter types uses gradual form (`...`).

```py
static_assert(is_assignable_to(Callable[..., int], Callable[..., float]))
static_assert(not is_assignable_to(Callable[..., float], Callable[..., int]))
```

And, if there is no return type, the return type is `Unknown`.

```py
static_assert(is_assignable_to(Callable[[], Unknown], Callable[[], int]))
static_assert(is_assignable_to(Callable[[], int], Callable[[], Unknown]))
```

### Parameter types

A `Callable` which uses the gradual form (`...`) for the parameter types is consistent with any
input signature.

```py
from ty_extensions import CallableTypeOf, static_assert, is_assignable_to
from typing import Any, Callable

static_assert(is_assignable_to(Callable[[], None], Callable[..., None]))
static_assert(is_assignable_to(Callable[..., None], Callable[..., None]))
static_assert(is_assignable_to(Callable[[int, float, str], None], Callable[..., None]))
```

Even if it includes any other parameter kinds.

```py
def positional_only(a: int, b: int, /) -> None: ...
def positional_or_keyword(a: int, b: int) -> None: ...
def variadic(*args: int) -> None: ...
def keyword_only(*, a: int, b: int) -> None: ...
def keyword_variadic(**kwargs: int) -> None: ...
def mixed(a: int, /, b: int, *args: int, c: int, **kwargs: int) -> None: ...

static_assert(is_assignable_to(CallableTypeOf[positional_only], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[positional_or_keyword], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[variadic], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[keyword_only], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[keyword_variadic], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[mixed], Callable[..., None]))
```

And, even if the parameters are unannotated.

```py
def positional_only(a, b, /) -> None: ...
def positional_or_keyword(a, b) -> None: ...
def variadic(*args) -> None: ...
def keyword_only(*, a, b) -> None: ...
def keyword_variadic(**kwargs) -> None: ...
def mixed(a, /, b, *args, c, **kwargs) -> None: ...

static_assert(is_assignable_to(CallableTypeOf[positional_only], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[positional_or_keyword], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[variadic], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[keyword_only], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[keyword_variadic], Callable[..., None]))
static_assert(is_assignable_to(CallableTypeOf[mixed], Callable[..., None]))
```

### Function types

```py
from typing import Any, Callable

def f(x: Any) -> str:
    return ""

def g(x: Any) -> int:
    return 1

c: Callable[[Any], str] = f

# error: [invalid-assignment] "Object of type `def g(x: Any) -> int` is not assignable to `(Any, /) -> str`"
c: Callable[[Any], str] = g
```

A function with no explicit return type should be assignable to a callable with a return type of
`Any`.

```py
def h():
    return

c: Callable[[], Any] = h
```

And, similarly for parameters with no annotations:

```py
def i(a, b, /) -> None:
    return

c: Callable[[Any, Any], None] = i
```

Additionally, a function definition that includes both `*args` and `**kwargs` parameters that are
annotated as `Any` or kept unannotated should be assignable to a callable with `...` as the
parameter type.

```py
def variadic_without_annotation(*args, **kwargs):
    return

def variadic_with_annotation(*args: Any, **kwargs: Any) -> Any:
    return

c: Callable[..., Any] = variadic_without_annotation
c: Callable[..., Any] = variadic_with_annotation
```

### Method types

```py
from typing import Any, Callable

class A:
    def f(self, x: Any) -> str:
        return ""

    def g(self, x: Any) -> int:
        return 1

c: Callable[[Any], str] = A().f

# error: [invalid-assignment] "Object of type `bound method A.g(x: Any) -> int` is not assignable to `(Any, /) -> str`"
c: Callable[[Any], str] = A().g
```

### Class literal types

```py
from typing import Any, Callable
from ty_extensions import static_assert, is_assignable_to

c: Callable[[object], type] = type
c: Callable[[str], Any] = str
c: Callable[[str], Any] = int

# error: [invalid-assignment]
c: Callable[[str], Any] = object

class A:
    def __init__(self, x: int) -> None: ...

a: Callable[[int], A] = A

class C:
    def __new__(cls, *args, **kwargs) -> "C":
        return super().__new__(cls)

    def __init__(self, x: int) -> None: ...

c: Callable[[int], C] = C

def f(a: Callable[..., Any], b: Callable[[Any], Any]): ...

f(tuple, tuple)

def g(a: Callable[[Any, Any], Any]): ...

# error: [invalid-argument-type] "Argument to function `g` is incorrect: Expected `(Any, Any, /) -> Any`, found `<class 'tuple'>`"
g(tuple)
```

### Generic class literal types

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Callable

class B[T]:
    def __init__(self, x: T) -> None: ...

b: Callable[[int], B[int]] = B[int]

class C[T]:
    def __new__(cls, *args, **kwargs) -> "C[T]":
        return super().__new__(cls)

    def __init__(self, x: T) -> None: ...

c: Callable[[int], C[int]] = C[int]
```

### Overloads

`overloaded.pyi`:

```pyi
from typing import Any, overload

@overload
def overloaded() -> None: ...
@overload
def overloaded(a: str) -> str: ...
@overload
def overloaded(a: str, b: Any) -> str: ...
```

```py
from overloaded import overloaded
from typing import Any, Callable

c: Callable[[], None] = overloaded
c: Callable[[str], str] = overloaded
c: Callable[[str, Any], Any] = overloaded
c: Callable[..., str] = overloaded

# error: [invalid-assignment]
c: Callable[..., int] = overloaded

# error: [invalid-assignment]
c: Callable[[int], str] = overloaded
```

### Classes with `__call__`

```py
from typing import Callable, Any
from ty_extensions import static_assert, is_assignable_to

class TakesAny:
    def __call__(self, a: Any) -> str:
        return ""

class ReturnsAny:
    def __call__(self, a: str) -> Any: ...

static_assert(is_assignable_to(TakesAny, Callable[[int], str]))
static_assert(not is_assignable_to(TakesAny, Callable[[int], int]))

static_assert(is_assignable_to(ReturnsAny, Callable[[str], int]))
static_assert(not is_assignable_to(ReturnsAny, Callable[[int], int]))

from functools import partial

def f(x: int, y: str) -> None: ...

c1: Callable[[int], None] = partial(f, y="a")
```

### Generic classes with `__call__`

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import Callable, Any, Generic, TypeVar, ParamSpec
from ty_extensions import static_assert, is_assignable_to

T = TypeVar("T")
P = ParamSpec("P")

class Foo[T]:
    def __call__(self): ...

class FooLegacy(Generic[T]):
    def __call__(self): ...

class Bar[T, **P]:
    def __call__(self): ...

class BarLegacy(Generic[T, P]):
    def __call__(self): ...

static_assert(is_assignable_to(Foo, Callable[..., Any]))
static_assert(is_assignable_to(FooLegacy, Callable[..., Any]))
static_assert(is_assignable_to(Bar, Callable[..., Any]))
static_assert(is_assignable_to(BarLegacy, Callable[..., Any]))

class Spam[T]: ...
class SpamLegacy(Generic[T]): ...
class Eggs[T, **P]: ...
class EggsLegacy(Generic[T, P]): ...

static_assert(not is_assignable_to(Spam, Callable[..., Any]))
static_assert(not is_assignable_to(SpamLegacy, Callable[..., Any]))
static_assert(not is_assignable_to(Eggs, Callable[..., Any]))
static_assert(not is_assignable_to(EggsLegacy, Callable[..., Any]))
```

### Classes with `__call__` as attribute

An instance type is assignable to a compatible callable type if the instance type's class has a
callable `__call__` attribute.

```py
from __future__ import annotations

from typing import Callable
from ty_extensions import static_assert, is_assignable_to

def call_impl(a: A, x: int) -> str:
    return ""

class A:
    __call__: Callable[[A, int], str] = call_impl

static_assert(is_assignable_to(A, Callable[[int], str]))
static_assert(not is_assignable_to(A, Callable[[int], int]))
reveal_type(A()(1))  # revealed: str
```

### Subclass of

#### Type of a class with constructor methods

```py
from typing import Callable
from ty_extensions import static_assert, is_assignable_to

class A:
    def __init__(self, x: int) -> None: ...

class B:
    def __new__(cls, x: str) -> "B":
        return super().__new__(cls)

static_assert(is_assignable_to(type[A], Callable[[int], A]))
static_assert(not is_assignable_to(type[A], Callable[[str], A]))

static_assert(is_assignable_to(type[B], Callable[[str], B]))
static_assert(not is_assignable_to(type[B], Callable[[int], B]))
```

#### Type with no generic parameters

```py
from typing import Callable, Any
from ty_extensions import static_assert, is_assignable_to

static_assert(is_assignable_to(type, Callable[..., Any]))
```

### Generic callables

A generic callable can be considered equivalent to an intersection of all of its possible
specializations. That means that a generic callable is assignable to any particular specialization.
(If someone expects a function that works with a particular specialization, it's fine to hand them
the generic callable.)

```py
from typing import Callable
from ty_extensions import CallableTypeOf, TypeOf, is_assignable_to, static_assert

def identity[T](t: T) -> T:
    return t

static_assert(is_assignable_to(TypeOf[identity], Callable[[int], int]))
static_assert(is_assignable_to(TypeOf[identity], Callable[[str], str]))
# TODO: no error
# error: [static-assert-error]
static_assert(not is_assignable_to(TypeOf[identity], Callable[[str], int]))

static_assert(is_assignable_to(CallableTypeOf[identity], Callable[[int], int]))
static_assert(is_assignable_to(CallableTypeOf[identity], Callable[[str], str]))
# TODO: no error
# error: [static-assert-error]
static_assert(not is_assignable_to(CallableTypeOf[identity], Callable[[str], int]))
```

The reverse is not true — if someone expects a generic function that can be called with any
specialization, we cannot hand them a function that only works with one specialization.

```py
static_assert(not is_assignable_to(Callable[[int], int], TypeOf[identity]))
static_assert(not is_assignable_to(Callable[[str], str], TypeOf[identity]))
static_assert(not is_assignable_to(Callable[[str], int], TypeOf[identity]))

static_assert(not is_assignable_to(Callable[[int], int], CallableTypeOf[identity]))
static_assert(not is_assignable_to(Callable[[str], str], CallableTypeOf[identity]))
static_assert(not is_assignable_to(Callable[[str], int], CallableTypeOf[identity]))
```

## Generics

### Assignability of generic types parameterized by gradual types

If `Foo` is a class that is generic over a single type variable `T`, `Foo[X]` will be assignable to
`Foo[Y]` iff `X` is assignable to `Y` AND `Y` is assignable to `X`.

This might appear to be the same principle as the "gradual equivalence" relation, but it is subtly
different. Two gradual types can be said to be "gradually equivalent" iff they have exactly the same
sets of possible materializations -- if they represent the same sets of possible types (the same
sets of sets of possible runtime objects). By this principle `int | Any` is gradually equivalent to
`Unknown | int`, since they have exactly the same sets of posisble materializations. But
`bool | Any` is not equivalent to `int`, since there are many possible materializations of
`bool | Any` that are not assignable to `int`. It is therefore _not_ necessary for `X` to be
gradually equivalent to `Y` in order for `Foo[X]` to be assignable to `Foo[Y]`; it is _only_
necessary for `X` and `Y` to be mutually assignable.

```py
from typing import Any, TypeVar, Generic
from ty_extensions import static_assert, is_assignable_to

InvariantTypeVar = TypeVar("InvariantTypeVar")

class Foo(Generic[InvariantTypeVar]):
    x: InvariantTypeVar

class A: ...
class B(A): ...
class C: ...

static_assert(is_assignable_to(Foo[A], Foo[B | Any]))
static_assert(is_assignable_to(Foo[B | Any], Foo[A]))
static_assert(is_assignable_to(Foo[Foo[Any]], Foo[Foo[A | C]]))
static_assert(is_assignable_to(Foo[Foo[A | C]], Foo[Foo[Any]]))
static_assert(is_assignable_to(Foo[tuple[A]], Foo[tuple[Any] | tuple[B]]))
static_assert(is_assignable_to(Foo[tuple[Any] | tuple[B]], Foo[tuple[A]]))

def f(obj: Foo[A]):
    g(obj)

def g(obj: Foo[B | Any]):
    f(obj)

def f2(obj: Foo[Foo[Any]]):
    g2(obj)

def g2(obj: Foo[Foo[A | C]]):
    f2(obj)

def f3(obj: Foo[tuple[Any] | tuple[B]]):
    g3(obj)

def g3(obj: Foo[tuple[A]]):
    f3(obj)
```

## Generic aliases

```py
from typing import final
from ty_extensions import static_assert, is_assignable_to, TypeOf

class GenericClass[T]:
    x: T  # invariant

static_assert(is_assignable_to(TypeOf[GenericClass], type[GenericClass]))
static_assert(is_assignable_to(TypeOf[GenericClass[int]], type[GenericClass]))
static_assert(is_assignable_to(TypeOf[GenericClass], type[GenericClass[int]]))
static_assert(is_assignable_to(TypeOf[GenericClass[int]], type[GenericClass[int]]))
static_assert(not is_assignable_to(TypeOf[GenericClass[str]], type[GenericClass[int]]))

class GenericClassIntBound[T: int]:
    x: T  # invariant

static_assert(is_assignable_to(TypeOf[GenericClassIntBound], type[GenericClassIntBound]))
static_assert(is_assignable_to(TypeOf[GenericClassIntBound[int]], type[GenericClassIntBound]))
static_assert(is_assignable_to(TypeOf[GenericClassIntBound], type[GenericClassIntBound[int]]))
static_assert(is_assignable_to(TypeOf[GenericClassIntBound[int]], type[GenericClassIntBound[int]]))

@final
class GenericFinalClass[T]:
    x: T  # invariant

static_assert(is_assignable_to(TypeOf[GenericFinalClass], type[GenericFinalClass]))
static_assert(is_assignable_to(TypeOf[GenericFinalClass[int]], type[GenericFinalClass]))
static_assert(is_assignable_to(TypeOf[GenericFinalClass], type[GenericFinalClass[int]]))
static_assert(is_assignable_to(TypeOf[GenericFinalClass[int]], type[GenericFinalClass[int]]))
static_assert(not is_assignable_to(TypeOf[GenericFinalClass[str]], type[GenericFinalClass[int]]))
```

## `TypeGuard` and `TypeIs`

`TypeGuard[...]` and `TypeIs[...]` are always assignable to `bool`.

```py
from ty_extensions import Unknown, is_assignable_to, static_assert
from typing_extensions import Any, TypeGuard, TypeIs

static_assert(is_assignable_to(TypeGuard[Unknown], bool))
static_assert(is_assignable_to(TypeIs[Any], bool))

# TODO no error
static_assert(not is_assignable_to(TypeGuard[Unknown], str))  # error: [static-assert-error]
static_assert(not is_assignable_to(TypeIs[Any], str))
```

## `ParamSpec`

```py
from ty_extensions import TypeOf, static_assert, is_assignable_to, Unknown
from typing import ParamSpec, Mapping, Callable, Any

P = ParamSpec("P")

def f(func: Callable[P, int], *args: P.args, **kwargs: P.kwargs) -> None:
    static_assert(is_assignable_to(TypeOf[args], tuple[Any, ...]))
    static_assert(is_assignable_to(TypeOf[args], tuple[object, ...]))
    static_assert(is_assignable_to(TypeOf[args], tuple[Unknown, ...]))
    static_assert(not is_assignable_to(TypeOf[args], tuple[int, ...]))
    static_assert(not is_assignable_to(TypeOf[args], tuple[int, str]))

    static_assert(not is_assignable_to(tuple[Any, ...], TypeOf[args]))
    static_assert(not is_assignable_to(tuple[object, ...], TypeOf[args]))
    static_assert(not is_assignable_to(tuple[Unknown, ...], TypeOf[args]))

    static_assert(is_assignable_to(TypeOf[kwargs], dict[str, Any]))
    static_assert(is_assignable_to(TypeOf[kwargs], dict[str, Unknown]))
    static_assert(not is_assignable_to(TypeOf[kwargs], dict[str, object]))
    static_assert(not is_assignable_to(TypeOf[kwargs], dict[str, int]))
    static_assert(is_assignable_to(TypeOf[kwargs], Mapping[str, Any]))
    static_assert(is_assignable_to(TypeOf[kwargs], Mapping[str, object]))
    static_assert(is_assignable_to(TypeOf[kwargs], Mapping[str, Unknown]))

    static_assert(not is_assignable_to(dict[str, Any], TypeOf[kwargs]))
    static_assert(not is_assignable_to(dict[str, object], TypeOf[kwargs]))
    static_assert(not is_assignable_to(dict[str, Unknown], TypeOf[kwargs]))
```

[gradual form]: https://typing.python.org/en/latest/spec/glossary.html#term-gradual-form
[gradual tuple]: https://typing.python.org/en/latest/spec/tuples.html#tuple-type-form
[typing documentation]: https://typing.python.org/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
