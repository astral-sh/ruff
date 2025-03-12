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

## `__new__` present on the class itself

If the class has an `__new__` method, we can infer the signature of the constructor from it.

```py
class Foo:
    def __new__(cls, x: int) -> "Foo":
        return object.__new__(cls)

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to function `__new__`: expected 1, got 2"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## `__new__` present on a superclass

If the `__new__` method is defined on a superclass, we can still infer the signature of the
constructor from it.

```py
from typing_extensions import Self

class Base:
    def __new__(cls, x: int) -> Self: ...

class Foo(Base): ...

reveal_type(Foo(1))  # revealed: Foo

# error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
reveal_type(Foo())  # revealed: Foo
# error: [too-many-positional-arguments] "Too many positional arguments to function `__new__`: expected 1, got 2"
reveal_type(Foo(1, 2))  # revealed: Foo
```

## Conditional `__new__`

```py
def _(flag: bool) -> None:
    class Foo:
        if flag:
            def __new__(cls, x: int): ...
        else:
            def __new__(cls, x: int, y: int = 1): ...

    reveal_type(Foo(1))  # revealed: Foo
    # error: [invalid-argument-type] "Object of type `Literal["1"]` cannot be assigned to parameter 2 (`x`) of function `__new__`; expected type `int`"
    reveal_type(Foo("1"))  # revealed: Foo
    # error: [missing-argument] "No argument provided for required parameter `x` of function `__new__`"
    reveal_type(Foo())  # revealed: Foo
```

## A descriptor in place of `__new__`

```py
class SomeCallable:
    def __call__(self, cls, x: int) -> "Foo":
        obj = object.__new__(cls)
        obj.x = x
        return obj

class Descriptor:
    def __get__(self, instance, owner) -> SomeCallable:
        return SomeCallable()

class Foo:
    __new__: Descriptor = Descriptor()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
reveal_type(Foo())  # revealed: Foo
```

## A callable instance in place of `__new__`

### Bound

```py
class Callable:
    def __call__(self, cls, x: int) -> "Foo":
        return object.__new__(cls)

class Foo:
    __new__ = Callable()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
reveal_type(Foo())  # revealed: Foo
```

### Possibly Unbound

```py
def _(flag: bool) -> None:
    class Callable:
        if flag:
            def __call__(self, cls, x: int) -> "Foo":
                return object.__new__(cls)

    class Foo:
        __new__ = Callable()

    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly unbound `__call__` method)"
    reveal_type(Foo(1))  # revealed: Foo
    # error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
    reveal_type(Foo())  # revealed: Foo
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
class SomeCallable:
    # TODO: at runtime `__init__` is checked to return `None` and
    # a `TypeError` is raised if it doesn't. However, apparently
    # this is not true when the descriptor is used as `__init__`.
    # However, we may still want to check this.
    def __call__(self, x: int) -> str:
        return "a"

class Descriptor:
    def __get__(self, instance, owner) -> SomeCallable:
        return SomeCallable()

class Foo:
    __init__: Descriptor = Descriptor()

reveal_type(Foo(1))  # revealed: Foo
# error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
reveal_type(Foo())  # revealed: Foo
```

## A callable instance in place of `__init__`

### Bound

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

### Possibly Unbound

```py
def _(flag: bool) -> None:
    class Callable:
        if flag:
            def __call__(self, x: int) -> None:
                pass

    class Foo:
        __init__ = Callable()

    # error: [call-non-callable] "Object of type `Callable` is not callable (possibly unbound `__call__` method)"
    reveal_type(Foo(1))  # revealed: Foo
    # error: [missing-argument] "No argument provided for required parameter `x` of bound method `__call__`"
    reveal_type(Foo())  # revealed: Foo
```
