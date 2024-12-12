# `type[Any]`

## Simple

```py
def f(x: type[Any]):
    reveal_type(x)  # revealed: type[Any]
    # TODO: could be `<object.__repr__ type> & Any`
    reveal_type(x.__repr__)  # revealed: Any

class A: ...

x: type[Any] = object
x: type[Any] = type
x: type[Any] = A
x: type[Any] = A()  # error: [invalid-assignment]
```

## Bare type

The interpretation of bare `type` is not clear: existing wording in the spec does not match the
behavior of mypy or pyright. For now we interpret it as simply "an instance of `builtins.type`",
which is equivalent to `type[object]`. This is similar to the current behavior of mypy, and pyright
in strict mode.

```py
def f(x: type):
    reveal_type(x)  # revealed: type
    reveal_type(x.__repr__)  # revealed: @Todo(instance attributes)

class A: ...

x: type = object
x: type = type
x: type = A
x: type = A()  # error: [invalid-assignment]
```

## type[object] != type[Any]

```py
def f(x: type[object]):
    reveal_type(x)  # revealed: type[object]
    # TODO: bound method types
    reveal_type(x.__repr__)  # revealed: Literal[__repr__]

class A: ...

x: type[object] = object
x: type[object] = type
x: type[object] = A
x: type[object] = A()  # error: [invalid-assignment]
```

## The type of `Any` is `type[Any]`

`Any` represents an unknown set of possible runtime values. If `x` is of type `Any`, the type of
`x.__class__` is also unknown and remains dynamic, *except* that we know it must be a class object
of some kind. As such, the type of `x.__class__` is `type[Any]` rather than `Any`:

```py
from typing import Any
from does_not_exist import SomethingUnknown  # error: [unresolved-import]

reveal_type(SomethingUnknown)  # revealed: Unknown

def test(x: Any, y: SomethingUnknown):
    reveal_type(x.__class__)  # revealed: type[Any]
    reveal_type(x.__class__.__class__.__class__.__class__)  # revealed: type[Any]
    reveal_type(y.__class__)  # revealed: type[Unknown]
    reveal_type(y.__class__.__class__.__class__.__class__)  # revealed: type[Unknown]
```
