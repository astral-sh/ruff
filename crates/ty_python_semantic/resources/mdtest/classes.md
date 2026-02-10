# Class definitions

## `__new__` return type

Python's `__new__` method can return any type, not just an instance of the class. When `__new__`
returns a type that is not assignable to the class instance type, we use that return type directly.

### `__new__` returning a different type

```py
class ReturnsInt:
    def __new__(cls) -> int:
        return 42

reveal_type(ReturnsInt())  # revealed: int

x: int = ReturnsInt()  # OK
y: ReturnsInt = ReturnsInt()  # error: [invalid-assignment]
```

### `__new__` returning a union type

```py
class MaybeInt:
    def __new__(cls, value: str) -> "int | MaybeInt":
        try:
            return int(value)
        except ValueError:
            return object.__new__(cls)

reveal_type(MaybeInt("42"))  # revealed: int | MaybeInt

a: int | MaybeInt = MaybeInt("42")  # OK
b: int = MaybeInt("42")  # error: [invalid-assignment]
```

### `__new__` returning the class type

When `__new__` returns the class type (or `Self`), the normal instance type is used.

```py
class Normal:
    def __new__(cls) -> "Normal":
        return object.__new__(cls)

reveal_type(Normal())  # revealed: Normal
```

### `__new__` with no return type annotation

When `__new__` has no return type annotation, we fall back to the instance type.

```py
class NoAnnotation:
    def __new__(cls):
        return object.__new__(cls)

reveal_type(NoAnnotation())  # revealed: NoAnnotation
```

### `__new__` returning `Any`

Per the spec, "an explicit return type of `Any` should be treated as a type that is not an instance
of the class being constructed." This means `__init__` is not called and the return type is `Any`.

```py
from typing import Any

class ReturnsAny:
    def __new__(cls) -> Any:
        return 42

    def __init__(self, x: int) -> None:
        pass

# __init__ is skipped because `-> Any` is treated as non-instance per spec
reveal_type(ReturnsAny())  # revealed: Any
```

### `__new__` returning a union containing `Any`

When `__new__` returns a union containing `Any`, `Any` is not a subtype of the instance type, so
`__init__` is skipped.

```py
from typing import Any

class MaybeAny:
    def __new__(cls, value: int) -> "MaybeAny | Any":
        if value > 0:
            return object.__new__(cls)
        return None

    def __init__(self, value: int) -> None:
        pass

reveal_type(MaybeAny(1))  # revealed: MaybeAny | Any
```

## Deferred resolution of bases

### Only the stringified name is deferred

If a class base contains a stringified name, only that name is deferred. Other names are resolved
normally.

```toml
[environment]
python-version = "3.12"
```

```py
from ty_extensions import reveal_mro

A = int

class G[T]: ...
class C(A, G["B"]): ...

A = str
B = bytes

reveal_mro(C)  # revealed: (<class 'C'>, <class 'int'>, <class 'G[bytes]'>, typing.Generic, <class 'object'>)
```

## Starred bases

Fixed-length tuples are unpacked when used as starred base classes:

```py
from ty_extensions import reveal_mro

class A: ...
class B: ...
class C: ...

bases = (A, B, C)

class Foo(*bases): ...

# revealed: (<class 'Foo'>, <class 'A'>, <class 'B'>, <class 'C'>, <class 'object'>)
reveal_mro(Foo)
```

Variable-length tuples cannot be unpacked, so they fall back to `Unknown`:

```py
from ty_extensions import reveal_mro

def get_bases() -> tuple[type, ...]:
    return (int, str)

# error: [unsupported-base] "Unsupported class base"
class Bar(*get_bases()): ...

# revealed: (<class 'Bar'>, Unknown, <class 'object'>)
reveal_mro(Bar)
```
