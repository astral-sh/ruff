# Assignable-to relation

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

Gradual types do not participate in subtyping, but can still be assignable to other types (and
static types can be assignable to gradual types):

```py
from ty_extensions import static_assert, is_assignable_to, Unknown
from typing import Any, Literal

static_assert(is_assignable_to(Unknown, Literal[1]))
static_assert(is_assignable_to(Any, Literal[1]))
static_assert(is_assignable_to(Literal[1], Unknown))
static_assert(is_assignable_to(Literal[1], Any))

class SubtypeOfAny(Any): ...

static_assert(is_assignable_to(SubtypeOfAny, Any))
static_assert(is_assignable_to(SubtypeOfAny, int))
static_assert(is_assignable_to(Any, SubtypeOfAny))
static_assert(not is_assignable_to(int, SubtypeOfAny))
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

static_assert(is_assignable_to(Literal["foo"], Literal["foo"]))
static_assert(is_assignable_to(Literal["foo"], LiteralString))
static_assert(is_assignable_to(Literal["foo"], str))

static_assert(is_assignable_to(LiteralString, str))

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

class AnyMeta(metaclass=Any): ...

static_assert(is_assignable_to(type[AnyMeta], type))
static_assert(is_assignable_to(type[AnyMeta], type[object]))
static_assert(is_assignable_to(type[AnyMeta], type[Any]))
```

## Class-literals that inherit from `Any`

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

While a homogeneous tuple type is not assignable to any heterogeneous tuple types, a heterogeneous
tuple type can be assignable to a homogeneous tuple type, and homogeneous tuple types can be
assignable to `Sequence`:

```py
from typing import Literal, Any, Sequence
from ty_extensions import static_assert, is_assignable_to, Not, AlwaysFalsy

static_assert(is_assignable_to(tuple[Literal[1], Literal[2]], tuple[Literal[1, 2], ...]))
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

c: Callable[[str], Any] = str
c: Callable[[str], Any] = int

# error: [invalid-assignment]
c: Callable[[str], Any] = object
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

### Classes with `__call__` as attribute

An instance type is assignable to a compatible callable type if the instance type's class has a
callable `__call__` attribute.

TODO: for the moment, we don't consider the callable type as a bound-method descriptor, but this may
change for better compatibility with mypy/pyright.

```py
from typing import Callable
from ty_extensions import static_assert, is_assignable_to

def call_impl(a: int) -> str:
    return ""

class A:
    __call__: Callable[[int], str] = call_impl

static_assert(is_assignable_to(A, Callable[[int], str]))
static_assert(not is_assignable_to(A, Callable[[int], int]))
reveal_type(A()(1))  # revealed: str
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
`bool | Any` that are not assignable to `int`. It is therefore *not* necessary for `X` to be
gradually equivalent to `Y` in order for `Foo[X]` to be assignable to `Foo[Y]`; it is *only*
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

[typing documentation]: https://typing.python.org/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
