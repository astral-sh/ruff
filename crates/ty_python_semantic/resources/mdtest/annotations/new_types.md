# NewType

## Valid forms

```py
from typing_extensions import NewType
from types import GenericAlias

X = GenericAlias(type, ())
A = NewType("A", int)
# TODO: typeshed for `typing.GenericAlias` uses `type` for the first argument. `NewType` should be special-cased
# to be compatible with `type`
# error: [invalid-argument-type] "Argument to function `__new__` is incorrect: Expected `type`, found `<NewType pseudo-class 'A'>`"
B = GenericAlias(A, ())

def _(
    a: A,
    b: B,
):
    reveal_type(a)  # revealed: A
    reveal_type(b)  # revealed: @Todo(Support for `typing.GenericAlias` instances in type expressions)
```

## Subtyping

The basic purpose of `NewType` is that it acts like a subtype of its base, but not the exact same
type (i.e. not an alias).

```py
from typing_extensions import NewType

Foo = NewType("Foo", int)
Bar = NewType("Bar", Foo)

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

## `NewType` wrapper functions are `Callable`

```py
from collections.abc import Callable
from typing_extensions import NewType

Foo = NewType("Foo", int)

def f(_: Callable[[int], Foo]): ...

f(Foo)
map(Foo, [1, 2, 3])

def g(_: Callable[[str], Foo]): ...

g(Foo)  # error: [invalid-argument-type]
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
Foo = NewType(name, int)  # allowed
```

## The second argument must be a class type or another newtype

Other typing constructs like `Union` are not allowed.

```py
from typing_extensions import NewType

# error: [invalid-newtype] "invalid base for `typing.NewType`"
Foo = NewType("Foo", int | str)
# error: [invalid-newtype] "invalid base for `typing.NewType`"
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
Foo = NewType("Foo", 42)
```

## Newtypes can be cyclic in various ways

Cyclic newtypes are kind of silly, but it's possible for the user to express them, and it's
important that we don't go into infinite recursive loops and crash with a stack overflow. In fact,
this is *why* base type evaluation is deferred; otherwise Salsa itself would crash.

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
```
