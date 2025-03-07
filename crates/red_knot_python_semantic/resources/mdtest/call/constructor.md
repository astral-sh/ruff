# Constructor

## No init or new

Every class has `object` in it's MRO, so if no `__init__` method is provided, we fall back to
`object.__init__`, which can only be called with zero arguments:

```py
class Foo: ...

reveal_type(Foo())  # revealed: Foo

# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 0, got 1"
reveal_type(Foo(1))  # revealed: Foo
```

## `__init__` present on the class itself

If the class has an `__init__` method, we can infer the signature of the constructor from it.

```py
class Foo:
    def __init__(self, x: int): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 1, got 2"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## `__init__` present on a superclass

If the `__init__` method is defined on a superclass, we can still infer the signature of the
constructor from it.

```py
class Base:
    def __init__(self, x: int): ...

class Foo(Base): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to bound method `__init__`: expected 1, got 2"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## Conditional `__init__`

```py
def _(flag: bool) -> None:
    class Foo:
        if flag:
            def __init__(self, x: int): ...
        else:
            def __init__(self, x: int, y: int = 1): ...

    reveal_type(Foo(1))  # revealed: Foo
    # error: [invalid-argument-type] "Object of type `Literal["1"]` cannot be assigned to parameter 2 (`x`) of bound method `__init__`; expected type `int`"
    reveal_type(Foo("1"))  # revealed: Foo
    # error: [missing-argument] "No argument provided for required parameter `x` of bound method `__init__`"
    reveal_type(Foo())  # revealed: Foo
```

## A descriptor in place of `__init__`

```py
from typing import Callable

class SomeCallable:
    def __call__(self, x: int) -> str:
        return "a"

class Descriptor:
    def __get__(self, instance, owner) -> SomeCallable:
        return lambda x: None

class Foo:
    __init__: Descriptor = Descriptor()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
reveal_type(Foo())  # revealed: Foo
```

## A callable instance in place of `__init__`

```py
class Callable:
    def __call__(self, x: int) -> None:
        pass

class Foo:
    __init__ = Callable()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
reveal_type(Foo())  # revealed: Foo
```
