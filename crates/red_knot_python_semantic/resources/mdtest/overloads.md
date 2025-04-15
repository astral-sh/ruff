# Overloads

Reference: <https://typing.python.org/en/latest/spec/overload.html>

## Invalid

### At least two overloads

At least two `@overload`-decorated definitions must be present.

```py
from typing import overload

# TODO: error
@overload
def func(x: int) -> int: ...
def func(x: int | str) -> int | str:
    return x
```

### Overload without an implementation

#### Regular modules

In regular modules, a series of `@overload`-decorated definitions must be followed by exactly one
non-`@overload`-decorated definition (for the same function/method).

```py
from typing import overload

# TODO: error because implementation does not exists
@overload
def func(x: int) -> int: ...
@overload
def func(x: str) -> str: ...

class Foo:
    # TODO: error because implementation does not exists
    @overload
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...
```

#### Stub files

Overload definitions within stub files are exempt from this check.

```pyi
from typing import overload

@overload
def func(x: int) -> int: ...
@overload
def func(x: str) -> str: ...
```

#### Protocols

Overload definitions within protocols are exempt from this check.

```py
from typing import Protocol, overload

class Foo(Protocol):
    @overload
    def f(self, x: int) -> int: ...
    @overload
    def f(self, x: str) -> str: ...
```

#### Abstract methods

Overload definitions within abstract base classes are exempt from this check.

```py
from abc import ABC, abstractmethod
from typing import overload

class AbstractFoo(ABC):
    @overload
    @abstractmethod
    def f(self, x: int) -> int: ...
    @overload
    @abstractmethod
    def f(self, x: str) -> str: ...
```

Using the `@abstractmethod` decorator requires that the class's metaclass is `ABCMeta` or is derived
from it.

```py
class Foo:
    # TODO: Error because implementation does not exists
    @overload
    @abstractmethod
    def f(self, x: int) -> int: ...
    @overload
    @abstractmethod
    def f(self, x: str) -> str: ...
```

And, the `@abstractmethod` decorator must be present on all the `@overload`-ed methods.

```py
class PartialFoo1(ABC):
    @overload
    @abstractmethod
    def f(self, x: int) -> int: ...
    @overload
    def f(self, x: str) -> str: ...

class PartialFoo(ABC):
    @overload
    def f(self, x: int) -> int: ...
    @overload
    @abstractmethod
    def f(self, x: str) -> str: ...
```

### Inconsistent decorators

#### `@staticmethod` / `@classmethod`

If one overload signature is decorated with `@staticmethod` or `@classmethod`, all overload
signatures must be similarly decorated. The implementation, if present, must also have a consistent
decorator.

```py
from __future__ import annotations

from typing import overload

class CheckStaticMethod:
    # TODO: error because `@staticmethod` does not exist on all overloads
    @overload
    def method1(x: int) -> int: ...
    @overload
    def method1(x: str) -> str: ...
    @staticmethod
    def method1(x: int | str) -> int | str:
        return x
    # TODO: error because `@staticmethod` does not exist on all overloads
    @overload
    def method2(x: int) -> int: ...
    @overload
    @staticmethod
    def method2(x: str) -> str: ...
    @staticmethod
    def method2(x: int | str) -> int | str:
        return x
    # TODO: error because `@staticmethod` does not exist on the implementation
    @overload
    @staticmethod
    def method3(x: int) -> int: ...
    @overload
    @staticmethod
    def method3(x: str) -> str: ...
    def method3(x: int | str) -> int | str:
        return x

    @overload
    @staticmethod
    def method4(x: int) -> int: ...
    @overload
    @staticmethod
    def method4(x: str) -> str: ...
    @staticmethod
    def method4(x: int | str) -> int | str:
        return x

class CheckClassMethod:
    def __init__(self, x: int) -> None:
        self.x = x
    # TODO: error because `@classmethod` does not exist on all overloads
    @overload
    @classmethod
    def try_from(cls, x: int) -> CheckClassMethod: ...
    @overload
    def try_from(cls, x: str) -> None: ...
    @classmethod
    def try_from(cls, x: int | str) -> CheckClassMethod | None:
        if isinstance(x, int):
            return cls(x)
        return None
    # TODO: error because `@classmethod` does not exist on all overloads
    @overload
    def try_from(cls, x: int) -> CheckClassMethod: ...
    @overload
    @classmethod
    def try_from(cls, x: str) -> None: ...
    @classmethod
    def try_from(cls, x: int | str) -> CheckClassMethod | None:
        if isinstance(x, int):
            return cls(x)
        return None
    # TODO: error because `@classmethod` does not exist on the implementation
    @overload
    @classmethod
    def try_from(cls, x: int) -> CheckClassMethod: ...
    @overload
    @classmethod
    def try_from(cls, x: str) -> None: ...
    def try_from(cls, x: int | str) -> CheckClassMethod | None:
        if isinstance(x, int):
            return cls(x)
        return None

    @overload
    @classmethod
    def try_from(cls, x: int) -> CheckClassMethod: ...
    @overload
    @classmethod
    def try_from(cls, x: str) -> None: ...
    @classmethod
    def try_from(cls, x: int | str) -> CheckClassMethod | None:
        if isinstance(x, int):
            return cls(x)
        return None
```

#### `@final` / `@override`

If a `@final` or `@override` decorator is supplied for a function with overloads, the decorator
should be applied only to the overload implementation if it is present.

```py
from typing_extensions import final, overload, override

class Foo:
    @overload
    def method1(self, x: int) -> int: ...
    @overload
    def method1(self, x: str) -> str: ...
    @final
    def method1(self, x: int | str) -> int | str:
        return x
    # TODO: error because `@final` is not on the implementation
    @overload
    @final
    def method2(self, x: int) -> int: ...
    @overload
    def method2(self, x: str) -> str: ...
    def method2(self, x: int | str) -> int | str:
        return x
    # TODO: error because `@final` is not on the implementation
    @overload
    def method3(self, x: int) -> int: ...
    @overload
    @final
    def method3(self, x: str) -> str: ...
    def method3(self, x: int | str) -> int | str:
        return x

class Base:
    @overload
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...
    def method(self, x: int | str) -> int | str:
        return x

class Sub1(Base):
    @overload
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...
    @override
    def method(self, x: int | str) -> int | str:
        return x

class Sub2(Base):
    # TODO: error because `@override` is not on the implementation
    @overload
    def method(self, x: int) -> int: ...
    @overload
    @override
    def method(self, x: str) -> str: ...
    def method(self, x: int | str) -> int | str:
        return x

class Sub3(Base):
    # TODO: error because `@override` is not on the implementation
    @overload
    @override
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...
    def method(self, x: int | str) -> int | str:
        return x
```

#### `@final` / `@override` in stub files

If an overload implementation isn’t present (for example, in a stub file), the `@final` or
`@override` decorator should be applied only to the first overload.

```pyi
from typing_extensions import final, overload, override

class Foo:
    @overload
    @final
    def method1(self, x: int) -> int: ...
    @overload
    def method1(self, x: str) -> str: ...

    # TODO: error because `@final` is not on the first overload
    @overload
    def method2(self, x: int) -> int: ...
    @final
    @overload
    def method2(self, x: str) -> str: ...

class Base:
    @overload
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...

class Sub1(Base):
    @overload
    @override
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...

class Sub2(Base):
    # TODO: error because `@override` is not on the first overload
    @overload
    def method(self, x: int) -> int: ...
    @overload
    @override
    def method(self, x: str) -> str: ...
```
