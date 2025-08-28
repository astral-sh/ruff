# NewType

## Valid forms

```py
from typing_extensions import NewType
from types import GenericAlias

X = GenericAlias(type, ())
A = NewType("A", int)
# TODO: typeshed for `typing.GenericAlias` uses `type` for the first argument. `NewType` should be special-cased
# to be compatible with `type`
# error: [invalid-argument-type] "Argument to function `__new__` is incorrect: Expected `type`, found `A`"
B = GenericAlias(A, ())

def _(
    a: A,
    b: B,
):
    reveal_type(a)  # revealed: A
    reveal_type(b)  # revealed: @Todo(Support for `typing.GenericAlias` instances in type expressions)
```

## Subtyping

```py
from typing_extensions import NewType

Foo = NewType("Foo", int)
Bar = NewType("Bar", Foo)

Foo(42)
Foo(Foo(42))       # allowed: `Foo` is a subtype of `int`.
Foo(Bar(Foo(42)))  # allowed: `Bar` is a subtype of `int`.
Foo(True)          # allowed: `bool` is a subtype of `int`.
Foo("fourty-two")  # error: [invalid-argument-type]

def f(_: int): ...
def g(_: Foo): ...
def h(_: Bar): ...

f(42)
f(Foo(42))
f(Bar(Foo(42)))

g(42)  # error: [invalid-argument-type]
g(Foo(42))
g(Bar(Foo(42)))

h(42)       # error: [invalid-argument-type]
h(Foo(42))  # error: [invalid-argument-type]
h(Bar(Foo(42)))
```
