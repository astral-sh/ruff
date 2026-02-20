# Calling `__init__` on instances

Calling `__init__` directly on an existing instance is unsound because the Liskov Substitution
Principle is not enforced on `__init__`, and `__init__` is excluded from variance inference for
generic classes.

## Basic error case

```py
class Foo:
    def __init__(self, x: int) -> None: ...

obj = Foo(1)
obj.__init__(2)  # error: [unsafe-init-on-instance]
```

## `self.__init__()` is an error

Even calling `__init__` on `self` is flagged, since `self` could be a subclass instance where
`__init__` has a different signature.

```py
class Foo:
    def __init__(self, x: int) -> None: ...
    def reinit(self) -> None:
        self.__init__(42)  # error: [unsafe-init-on-instance]
```

## `super().__init__()` is allowed

The standard pattern of calling `super().__init__()` inside a constructor is always allowed.

```py
class Base:
    def __init__(self, x: int) -> None: ...

class Child(Base):
    def __init__(self, x: int) -> None:
        super().__init__(x)  # OK
```

## `Class.__init__(self)` is allowed

Calling `__init__` as an unbound method on a class literal is allowed.

```py
class Base:
    def __init__(self, x: int) -> None: ...

class Child(Base):
    def __init__(self, x: int) -> None:
        Base.__init__(self, x)  # OK
```

## `super(Class, self).__init__()` is allowed

```py
class A:
    def __init__(self, a: int) -> None: ...

class B(A):
    def __init__(self, a: int) -> None:
        super(B, self).__init__(a)  # OK
```

## Calling `__init__` on a literal value

```py
x = 42
x.__init__()  # error: [unsafe-init-on-instance]
```

## Dynamic types are not flagged

```py
from typing import Any

def f(x: Any) -> None:
    x.__init__()  # OK (dynamic type)
```

## Union types

```py
class Foo:
    def __init__(self) -> None: ...

class Bar:
    def __init__(self) -> None: ...

def f(x: Foo | Bar) -> None:
    x.__init__()  # error: [unsafe-init-on-instance]
```

## `self.__init__()` inside `__init__` is an error

```py
class Foo:
    def __init__(self, x: int) -> None:
        self.__init__(x)  # error: [unsafe-init-on-instance]
```

## Calling `__init__` on a parameter

```py
class Base:
    def __init__(self) -> None: ...

def f(obj: Base) -> None:
    obj.__init__()  # error: [unsafe-init-on-instance]
```

## `type[X].__init__()` is allowed

```py
class Base:
    def __init__(self) -> None: ...

def f(cls: type[Base]) -> None:
    cls.__init__(cls())  # OK
```

## Bare `type.__init__()` is allowed

```py
def f(cls: type) -> None:
    cls.__init__(cls())  # OK
```

## TypeVar bounded by `type[X]` is allowed

```py
from typing import TypeVar

class Base:
    def __init__(self) -> None: ...

T = TypeVar("T", bound=type[Base])

def f(cls: T) -> None:
    cls.__init__(cls())  # OK
```

## TypeVar constrained to `type[X]` variants is allowed

```py
from typing import TypeVar

class A:
    def __init__(self) -> None: ...

class B:
    def __init__(self) -> None: ...

T = TypeVar("T", type[A], type[B])

def f(cls: T) -> None:
    cls.__init__(cls())  # OK
```

## `__new__` is not flagged

This lint is specific to `__init__`. Calling `__new__` is a separate concern.

```py
class Foo:
    def __new__(cls) -> "Foo":
        return super().__new__(cls)

obj = Foo()
obj.__new__(Foo)  # OK (not __init__)
```

## Multiple inheritance with `super().__init__()`

```py
class A:
    def __init__(self) -> None: ...

class B:
    def __init__(self) -> None: ...

class C(A, B):
    def __init__(self) -> None:
        super().__init__()  # OK
        A.__init__(self)  # OK
        B.__init__(self)  # OK
```

## Generic class instances

```py
from typing import Generic, TypeVar

T = TypeVar("T")

class Box(Generic[T]):
    def __init__(self, value: T) -> None: ...

box = Box(42)
box.__init__(99)  # error: [unsafe-init-on-instance]
```

## Intersection types

```py
from ty_extensions import Intersection

class A:
    def __init__(self) -> None: ...

class B:
    def __init__(self) -> None: ...

def f(x: Intersection[A, B]) -> None:
    x.__init__()  # error: [unsafe-init-on-instance]
```

## TypedDict instances

```py
from typing import TypedDict

class TD(TypedDict):
    x: int

def f(td: TD) -> None:
    td.__init__()  # error: [unsafe-init-on-instance]
```
