# NewType

## Basic usage

`NewType` can be used to create distinct types that are based on existing types:

```py
from typing_extensions import NewType

UserId = NewType("UserId", int)

def _(user_id: UserId):
    reveal_type(user_id)  # revealed: UserId
```

## Subtyping

The basic purpose of `NewType` is that it acts like a subtype of its base, but not the exact same
type (i.e. not an alias).

```py
from typing_extensions import NewType
from ty_extensions import static_assert, is_subtype_of, is_equivalent_to

Foo = NewType("Foo", int)
Bar = NewType("Bar", Foo)

static_assert(is_subtype_of(Foo, int))
static_assert(not is_equivalent_to(Foo, int))

static_assert(is_subtype_of(Bar, Foo))
static_assert(is_subtype_of(Bar, int))
static_assert(not is_equivalent_to(Bar, Foo))

Foo(42)
Foo(Foo(42))  # allowed: `Foo` is a subtype of `int`.
Foo(Bar(Foo(42)))  # allowed: `Bar` is a subtype of `int`.
Foo(True)  # allowed: `bool` is a subtype of `int`.
Foo("forty-two")  # error: [invalid-argument-type] "Argument is incorrect: Expected `int`, found `Literal["forty-two"]`"

def f(_: int): ...
def g(_: Foo): ...
def h(_: Bar): ...

f(42)
f(Foo(42))
f(Bar(Foo(42)))

g(42)  # error: [invalid-argument-type] "Argument to function `g` is incorrect: Expected `Foo`, found `Literal[42]`"
g(Foo(42))
g(Bar(Foo(42)))

h(42)  # error: [invalid-argument-type] "Argument to function `h` is incorrect: Expected `Bar`, found `Literal[42]`"
h(Foo(42))  # error: [invalid-argument-type] "Argument to function `h` is incorrect: Expected `Bar`, found `Foo`"
h(Bar(Foo(42)))
```

## Member and method lookup work

```py
from typing_extensions import NewType

class Foo:
    foo_member: str = "hello"
    def foo_method(self) -> int:
        return 42

Bar = NewType("Bar", Foo)
Baz = NewType("Baz", Bar)
baz = Baz(Bar(Foo()))
reveal_type(baz.foo_member)  # revealed: str
reveal_type(baz.foo_method())  # revealed: int
```

We also infer member access on the `NewType` pseudo-type itself correctly:

```py
reveal_type(Bar.__supertype__)  # revealed: type | NewType
reveal_type(Baz.__supertype__)  # revealed: type | NewType
```

## `NewType` wrapper functions are `Callable`

```py
from collections.abc import Callable
from typing_extensions import NewType
from ty_extensions import CallableTypeOf

Foo = NewType("Foo", int)

def _(obj: CallableTypeOf[Foo]):
    reveal_type(obj)  # revealed: (int, /) -> Foo

def f(_: Callable[[int], Foo]): ...

f(Foo)
map(Foo, [1, 2, 3])

def g(_: Callable[[str], Foo]): ...

g(Foo)  # error: [invalid-argument-type]
```

## `NewType` instances are `Callable` if the base type is

```py
from typing import NewType, Callable, Any
from ty_extensions import CallableTypeOf

N = NewType("N", int)
i = N(42)

y: Callable[..., Any] = i  # error: [invalid-assignment] "Object of type `N` is not assignable to `(...) -> Any`"

# error: [invalid-type-form] "Expected the first argument to `ty_extensions.CallableTypeOf` to be a callable object, but got an object of type `N`"
def f(x: CallableTypeOf[i]):
    reveal_type(x)  # revealed: Unknown

class SomethingCallable:
    def __call__(self, a: str) -> bytes:
        raise NotImplementedError

N2 = NewType("N2", SomethingCallable)
j = N2(SomethingCallable())

z: Callable[[str], bytes] = j  # fine

def g(x: CallableTypeOf[j]):
    reveal_type(x)  # revealed: (a: str) -> bytes
```

## The name must be a string literal

```py
from typing_extensions import NewType

def _(name: str) -> None:
    _ = NewType(name, int)  # error: [invalid-newtype] "The first argument to `NewType` must be a string literal"
```

However, the literal doesn't necessarily need to be inline, as long as we infer it:

```py
name = "Foo"
Foo = NewType(name, int)
reveal_type(Foo)  # revealed: <NewType pseudo-class 'Foo'>
```

## The base must be a class type or another newtype

Other typing constructs like `Union` are not _generally_ allowed. (However, see the next section for
a couple special cases.)

```py
from typing_extensions import NewType

# error: [invalid-newtype] "invalid base for `typing.NewType`"
Foo = NewType("Foo", int | str)
```

We don't emit the "invalid base" diagnostic for `Unknown`, because that typically results from other
errors that already have a diagnostic, and there's no need to pile on. For example, this mistake
gives you an "Int literals are not allowed" error, and we'd rather not see an "invalid base" error
on top of that:

```py
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
Foo = NewType("Foo", 42)
```

## `float` and `complex` special cases

`float` and `complex` are subject to a special case in the typing spec, which we currently interpret
to mean that `float` in type position is `int | float`, and `complex` in type position is
`int | float | complex`. This is awkward for `NewType`, because as we just tested above, unions
aren't generally valid `NewType` bases. However, `float` and `complex` _are_ valid `NewType` bases,
and we accept the unions they expand into.

```py
from typing import NewType

Foo = NewType("Foo", float)
Foo(3.14)
Foo(42)
Foo("hello")  # error: [invalid-argument-type] "Argument is incorrect: Expected `int | float`, found `Literal["hello"]`"

reveal_type(Foo(3.14).__class__)  # revealed: type[int] | type[float]
reveal_type(Foo(42).__class__)  # revealed: type[int] | type[float]

Bar = NewType("Bar", complex)
Bar(1 + 2j)
Bar(3.14)
Bar(42)
Bar("goodbye")  # error: [invalid-argument-type]

reveal_type(Bar(1 + 2j).__class__)  # revealed: type[int] | type[float] | type[complex]
reveal_type(Bar(3.14).__class__)  # revealed: type[int] | type[float] | type[complex]
reveal_type(Bar(42).__class__)  # revealed: type[int] | type[float] | type[complex]
```

We don't currently try to distinguish between an implicit union (e.g. `float`) and the equivalent
explicit union (e.g. `int | float`), so these two explicit unions are also allowed. But again, most
unions are not allowed:

```py
Baz = NewType("Baz", int | float)
Baz = NewType("Baz", int | float | complex)
Baz = NewType("Baz", int | str)  # error: [invalid-newtype] "invalid base for `typing.NewType`"
```

Similarly, a `NewType` of `float` or `complex` is valid as a `Callable` of the corresponding union
type:

```py
from collections.abc import Callable

def f(_: Callable[[int | float], Foo]): ...

f(Foo)

def g(_: Callable[[int | float | complex], Bar]): ...

g(Bar)
```

## A `NewType` definition must be a simple variable assignment

```py
from typing import NewType

N: NewType = NewType("N", int)  # error: [invalid-newtype] "A `NewType` definition must be a simple variable assignment"
```

## Newtypes can be cyclic in various ways

Cyclic newtypes are kind of silly, but it's possible for the user to express them, and it's
important that we don't go into infinite recursive loops and crash with a stack overflow. In fact,
this is _why_ base type evaluation is deferred; otherwise Salsa itself would crash.

```py
from typing_extensions import NewType, reveal_type, cast

# Define a directly cyclic newtype.
A = NewType("A", "A")
reveal_type(A)  # revealed: <NewType pseudo-class 'A'>

# Typechecking still works. We can't construct an `A` "honestly", but we can `cast` into one.
a: A
a = 42  # error: [invalid-assignment] "Object of type `Literal[42]` is not assignable to `A`"
a = A(42)  # error: [invalid-argument-type] "Argument is incorrect: Expected `A`, found `Literal[42]`"
a = cast(A, 42)
reveal_type(a)  # revealed: A

# A newtype cycle might involve more than one step.
B = NewType("B", "C")
C = NewType("C", "B")
reveal_type(B)  # revealed: <NewType pseudo-class 'B'>
reveal_type(C)  # revealed: <NewType pseudo-class 'C'>
b: B = cast(B, 42)
c: C = C(b)
reveal_type(b)  # revealed: B
reveal_type(c)  # revealed: C
# Cyclic types behave in surprising ways. These assignments are legal, even though B and C aren't
# the same type, because each of them is a subtype of the other.
b = c
c = b

# Another newtype could inherit from a cyclic one.
D = NewType("D", C)
reveal_type(D)  # revealed: <NewType pseudo-class 'D'>
d: D
d = D(42)  # error: [invalid-argument-type] "Argument is incorrect: Expected `C`, found `Literal[42]`"
d = D(c)
d = D(b)  # Allowed, the same surprise as above. B and C are subtypes of each other.
reveal_type(d)  # revealed: D
```

Normal classes can't inherit from newtypes, but generic classes can be parametrized with them, so we
also need to detect "ordinary" type cycles that happen to involve a newtype.

```py
E = NewType("E", list["E"])
reveal_type(E)  # revealed: <NewType pseudo-class 'E'>
e: E = E([])
reveal_type(e)  # revealed: E
reveal_type(E(E(E(E(E([]))))))  # revealed: E
reveal_type(E([E([E([]), E([E([])])]), E([])]))  # revealed: E
E(["foo"])  # error: [invalid-argument-type]
E(E(E(["foo"])))  # error: [invalid-argument-type]
```

## `NewType` wrapping preserves singleton-ness and single-valued-ness

```py
from typing_extensions import NewType
from ty_extensions import is_singleton, is_single_valued, static_assert
from types import EllipsisType

A = NewType("A", EllipsisType)
static_assert(is_singleton(A))
static_assert(is_single_valued(A))
reveal_type(type(A(...)) is EllipsisType)  # revealed: Literal[True]
# TODO: This should be `Literal[True]` also.
reveal_type(A(...) is ...)  # revealed: bool

B = NewType("B", int)
static_assert(not is_singleton(B))
static_assert(not is_single_valued(B))
```

## `NewType`s of tuples can be iterated/unpacked

```py
from typing import NewType

N = NewType("N", tuple[int, str])

a, b = N((1, "foo"))

reveal_type(a)  # revealed: int
reveal_type(b)  # revealed: str
```

## `isinstance` of a `NewType` instance and its base class is inferred as `Literal[True]`

```py
from typing import NewType

N = NewType("N", int)

def f(x: N):
    reveal_type(isinstance(x, int))  # revealed: Literal[True]
```

However, a `NewType` isn't a real class, so it isn't a valid second argument to `isinstance`:

```py
def f(x: N):
    # error: [invalid-argument-type] "Argument to function `isinstance` is incorrect"
    reveal_type(isinstance(x, N))  # revealed: bool
```

Because of that, we don't generate any narrowing constraints for it:

```py
def f(x: N | str):
    if isinstance(x, N):  # error: [invalid-argument-type]
        reveal_type(x)  # revealed: N | str
    else:
        reveal_type(x)  # revealed: N | str
```

## Trying to subclass a `NewType` produces an error matching CPython

<!-- snapshot-diagnostics -->

```py
from typing import NewType

X = NewType("X", int)

class Foo(X): ...  # error: [invalid-base]
```

## Don't narrow `NewType`-wrapped `Enum`s inside of match arms

`Literal[Foo.X]` is actually disjoint from `N` here:

```py
from enum import Enum
from typing import NewType

class Foo(Enum):
    X = 0
    Y = 1

N = NewType("N", Foo)

def f(x: N):
    match x:
        case Foo.X:
            reveal_type(x)  # revealed: N
        case Foo.Y:
            reveal_type(x)  # revealed: N
        case _:
            reveal_type(x)  # revealed: N
```

## We don't support `NewType` on Python 3.9

We implement `typing.NewType` as a `KnownClass`, but in Python 3.9 it's actually a function, so all
we get is the `Any` annotations from typeshed. However, `typing_extensions.NewType` is always a
class. This could be improved in the future, but Python 3.9 is now end-of-life, so it's not
high-priority.

```toml
[environment]
python-version = "3.9"
```

```py
from typing import NewType

Foo = NewType("Foo", int)
reveal_type(Foo)  # revealed: Any
reveal_type(Foo(42))  # revealed: Any

from typing_extensions import NewType

Bar = NewType("Bar", int)
reveal_type(Bar)  # revealed: <NewType pseudo-class 'Bar'>
reveal_type(Bar(42))  # revealed: Bar
```

## The base of a `NewType` can't be a protocol class or a `TypedDict`

<!-- snapshot-diagnostics -->

```py
from typing import NewType, Protocol, TypedDict

class Id(Protocol):
    code: int

UserId = NewType("UserId", Id)  # error: [invalid-newtype]

class Foo(TypedDict):
    a: int

Bar = NewType("Bar", Foo)  # error: [invalid-newtype]
```

## TODO: A `NewType` cannot be generic

```py
from typing import Any, NewType, TypeVar

# All of these are allowed.
A = NewType("A", list)
B = NewType("B", list[int])
B = NewType("B", list[Any])

# But a free typevar is not allowed.
T = TypeVar("T")
C = NewType("C", list[T])  # TODO: should be "error: [invalid-newtype]"
```

## Forward references in stub files

Stubs natively support forward references, so patterns that would raise `NameError` at runtime are
allowed in stub files:

`stub.pyi`:

```pyi
from typing import NewType

N = NewType("N", A)

class A: ...
```

`main.py`:

```py
from stub import N, A

n = N(A())  # fine

def f(x: A): ...

f(n)  # fine

class Invalid: ...

bad = N(Invalid())  # error: [invalid-argument-type]
```
