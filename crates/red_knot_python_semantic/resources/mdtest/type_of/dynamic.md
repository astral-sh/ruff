# `type[Any]`

This file contains tests for non-fully-static `type[]` types, such as `type[Any]` and
`type[Unknown]`.

## Simple

```py
from typing import Any

def f(x: type[Any], y: type[str]):
    reveal_type(x)  # revealed: type[Any]
    # TODO: could be `<object.__repr__ type> & Any`
    reveal_type(x.__repr__)  # revealed: Any

    # type[str] and type[Any] are assignable to each other
    a: type[str] = x
    b: type[Any] = y

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
    reveal_type(x.__repr__)  # revealed: <bound method `__repr__` of `type`>

class A: ...

x: type = object
x: type = type
x: type = A
x: type = A()  # error: [invalid-assignment]
```

## type[object] != type[Any]

```py
def f(x: type[object]):
    reveal_type(x)  # revealed: type
    reveal_type(x.__repr__)  # revealed: <bound method `__repr__` of `type`>

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

## `type[Unknown]` has similar properties to `type[Any]`

```py
import abc
from typing import Any
from does_not_exist import SomethingUnknown  # error: [unresolved-import]

has_unknown_type = SomethingUnknown.__class__
reveal_type(has_unknown_type)  # revealed: type[Unknown]

def test(x: type[str], y: type[Any]):
    """Both `type[Any]` and `type[Unknown]` are assignable to all `type[]` types"""
    a: type[Any] = x
    b: type[str] = y
    c: type[Any] = has_unknown_type
    d: type[str] = has_unknown_type

def test2(a: type[Any]):
    """`type[Any]` and `type[Unknown]` are also assignable to all instances of `type` subclasses"""
    b: abc.ABCMeta = a
    b: abc.ABCMeta = has_unknown_type
```
