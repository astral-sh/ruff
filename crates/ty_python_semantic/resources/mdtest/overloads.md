# Overloads

Reference: <https://typing.python.org/en/latest/spec/overload.html>

## `typing.overload`

The definition of `typing.overload` in typeshed is an identity function.

```py
from typing import overload

def foo(x: int) -> int:
    return x

reveal_type(foo)  # revealed: def foo(x: int) -> int
bar = overload(foo)
reveal_type(bar)  # revealed: def foo(x: int) -> int
```

## Functions

```py
from typing import overload

@overload
def add() -> None: ...
@overload
def add(x: int) -> int: ...
@overload
def add(x: int, y: int) -> int: ...
def add(x: int | None = None, y: int | None = None) -> int | None:
    return (x or 0) + (y or 0)

reveal_type(add)  # revealed: Overload[() -> None, (x: int) -> int, (x: int, y: int) -> int]
reveal_type(add())  # revealed: None
reveal_type(add(1))  # revealed: int
reveal_type(add(1, 2))  # revealed: int
```

## Overriding

These scenarios are to verify that the overloaded and non-overloaded definitions are correctly
overridden by each other.

An overloaded function is overriding another overloaded function:

```py
from typing import overload

@overload
def foo() -> None: ...
@overload
def foo(x: int) -> int: ...
def foo(x: int | None = None) -> int | None:
    return x

reveal_type(foo)  # revealed: Overload[() -> None, (x: int) -> int]
reveal_type(foo())  # revealed: None
reveal_type(foo(1))  # revealed: int

@overload
def foo() -> None: ...
@overload
def foo(x: str) -> str: ...
def foo(x: str | None = None) -> str | None:
    return x

reveal_type(foo)  # revealed: Overload[() -> None, (x: str) -> str]
reveal_type(foo())  # revealed: None
reveal_type(foo(""))  # revealed: str
```

A non-overloaded function is overriding an overloaded function:

```py
def foo(x: int) -> int:
    return x

reveal_type(foo)  # revealed: def foo(x: int) -> int
```

An overloaded function is overriding a non-overloaded function:

```py
reveal_type(foo)  # revealed: def foo(x: int) -> int

@overload
def foo() -> None: ...
@overload
def foo(x: bytes) -> bytes: ...
def foo(x: bytes | None = None) -> bytes | None:
    return x

reveal_type(foo)  # revealed: Overload[() -> None, (x: bytes) -> bytes]
reveal_type(foo())  # revealed: None
reveal_type(foo(b""))  # revealed: bytes
```

## Methods

```py
from typing_extensions import Self, overload

class Foo1:
    @overload
    def method(self) -> None: ...
    @overload
    def method(self, x: int) -> int: ...
    def method(self, x: int | None = None) -> int | None:
        return x

foo1 = Foo1()
reveal_type(foo1.method)  # revealed: Overload[() -> None, (x: int) -> int]
reveal_type(foo1.method())  # revealed: None
reveal_type(foo1.method(1))  # revealed: int

class Foo2:
    @overload
    def method(self) -> None: ...
    @overload
    def method(self, x: str) -> str: ...
    def method(self, x: str | None = None) -> str | None:
        return x

foo2 = Foo2()
reveal_type(foo2.method)  # revealed: Overload[() -> None, (x: str) -> str]
reveal_type(foo2.method())  # revealed: None
reveal_type(foo2.method(""))  # revealed: str

class Foo3:
    @overload
    def takes_self_or_int(self: Self, x: Self) -> Self: ...
    @overload
    def takes_self_or_int(self: Self, x: int) -> int: ...
    def takes_self_or_int(self: Self, x: Self | int) -> Self | int:
        return x

foo3 = Foo3()
reveal_type(foo3.takes_self_or_int(foo3))  # revealed: Foo3
reveal_type(foo3.takes_self_or_int(1))  # revealed: int
```

## Constructor

```py
from typing import overload

class Foo:
    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self, x: int) -> None: ...
    def __init__(self, x: int | None = None) -> None:
        self.x = x

foo = Foo()
reveal_type(foo)  # revealed: Foo
reveal_type(foo.x)  # revealed: Unknown | int | None

foo1 = Foo(1)
reveal_type(foo1)  # revealed: Foo
reveal_type(foo1.x)  # revealed: Unknown | int | None
```

## Version specific

Function definitions can vary between multiple Python versions.

### Overload and non-overload (3.9)

Here, the same function is overloaded in one version and not in another.

```toml
[environment]
python-version = "3.9"
```

```py
import sys
from typing import overload

if sys.version_info < (3, 10):
    def func(x: int) -> int:
        return x

elif sys.version_info <= (3, 12):
    @overload
    def func() -> None: ...
    @overload
    def func(x: int) -> int: ...
    def func(x: int | None = None) -> int | None:
        return x

reveal_type(func)  # revealed: def func(x: int) -> int
func()  # error: [missing-argument]
```

### Overload and non-overload (3.10)

```toml
[environment]
python-version = "3.10"
```

```py
import sys
from typing import overload

if sys.version_info < (3, 10):
    def func(x: int) -> int:
        return x

elif sys.version_info <= (3, 12):
    @overload
    def func() -> None: ...
    @overload
    def func(x: int) -> int: ...
    def func(x: int | None = None) -> int | None:
        return x

reveal_type(func)  # revealed: Overload[() -> None, (x: int) -> int]
reveal_type(func())  # revealed: None
reveal_type(func(1))  # revealed: int
```

### Some overloads are version specific (3.9)

```toml
[environment]
python-version = "3.9"
```

`overloaded.pyi`:

```pyi
import sys
from typing import overload

if sys.version_info >= (3, 10):
    @overload
    def func() -> None: ...

@overload
def func(x: int) -> int: ...
@overload
def func(x: str) -> str: ...
```

`main.py`:

```py
from overloaded import func

reveal_type(func)  # revealed: Overload[(x: int) -> int, (x: str) -> str]
func()  # error: [no-matching-overload]
reveal_type(func(1))  # revealed: int
reveal_type(func(""))  # revealed: str
```

### Some overloads are version specific (3.10)

```toml
[environment]
python-version = "3.10"
```

`overloaded.pyi`:

```pyi
import sys
from typing import overload

@overload
def func() -> None: ...

if sys.version_info >= (3, 10):
    @overload
    def func(x: int) -> int: ...

@overload
def func(x: str) -> str: ...
```

`main.py`:

```py
from overloaded import func

reveal_type(func)  # revealed: Overload[() -> None, (x: int) -> int, (x: str) -> str]
reveal_type(func())  # revealed: None
reveal_type(func(1))  # revealed: int
reveal_type(func(""))  # revealed: str
```

## Generic

```toml
[environment]
python-version = "3.12"
```

For an overloaded generic function, it's not necessary for all overloads to be generic.

```py
from typing import overload

@overload
def func() -> None: ...
@overload
def func[T](x: T) -> T: ...
def func[T](x: T | None = None) -> T | None:
    return x

reveal_type(func)  # revealed: Overload[() -> None, (x: T@func) -> T@func]
reveal_type(func())  # revealed: None
reveal_type(func(1))  # revealed: Literal[1]
reveal_type(func(""))  # revealed: Literal[""]
```

## Invalid

### At least two overloads

<!-- snapshot-diagnostics -->

At least two `@overload`-decorated definitions must be present.

```py
from typing import overload

@overload
def func(x: int) -> int: ...

# error: [invalid-overload]
def func(x: int | str) -> int | str:
    return x
```

```pyi
from typing import overload

@overload
# error: [invalid-overload]
def func(x: int) -> int: ...
```

### Overload without an implementation

#### Regular modules

<!-- snapshot-diagnostics -->

In regular modules, a series of `@overload`-decorated definitions must be followed by exactly one
non-`@overload`-decorated definition (for the same function/method).

```py
from typing import overload

@overload
def func(x: int) -> int: ...
@overload
# error: [invalid-overload] "Overloads for function `func` must be followed by a non-`@overload`-decorated implementation function"
def func(x: str) -> str: ...

class Foo:
    @overload
    def method(self, x: int) -> int: ...
    @overload
    # error: [invalid-overload] "Overloads for function `method` must be followed by a non-`@overload`-decorated implementation function"
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
from abc import ABCMeta

class CustomAbstractMetaclass(ABCMeta): ...

class Fine(metaclass=CustomAbstractMetaclass):
    @overload
    @abstractmethod
    def f(self, x: int) -> int: ...
    @overload
    @abstractmethod
    def f(self, x: str) -> str: ...

class Foo:
    @overload
    @abstractmethod
    def f(self, x: int) -> int: ...
    @overload
    @abstractmethod
    # error: [invalid-overload]
    def f(self, x: str) -> str: ...
```

And, the `@abstractmethod` decorator must be present on all the `@overload`-ed methods.

```py
class PartialFoo1(ABC):
    @overload
    @abstractmethod
    def f(self, x: int) -> int: ...
    @overload
    # error: [invalid-overload]
    def f(self, x: str) -> str: ...

class PartialFoo(ABC):
    @overload
    def f(self, x: int) -> int: ...
    @overload
    @abstractmethod
    # error: [invalid-overload]
    def f(self, x: str) -> str: ...
```

#### `TYPE_CHECKING` blocks

As in other areas of ty, we treat `TYPE_CHECKING` blocks the same as "inline stub files", so we
permit overloaded functions to exist without an implementation if all overloads are defined inside
an `if TYPE_CHECKING` block:

```py
from typing import overload, TYPE_CHECKING

if TYPE_CHECKING:
    @overload
    def a() -> str: ...
    @overload
    def a(x: int) -> int: ...

    class F:
        @overload
        def method(self) -> None: ...
        @overload
        def method(self, x: int) -> int: ...

class G:
    if TYPE_CHECKING:
        @overload
        def method(self) -> None: ...
        @overload
        def method(self, x: int) -> int: ...

if TYPE_CHECKING:
    @overload
    def b() -> str: ...

if TYPE_CHECKING:
    @overload
    def b(x: int) -> int: ...

if TYPE_CHECKING:
    @overload
    def c() -> None: ...

# not all overloads are in a `TYPE_CHECKING` block, so this is an error
@overload
# error: [invalid-overload]
def c(x: int) -> int: ...
```

### `@overload`-decorated functions with non-stub bodies

<!-- snapshot-diagnostics -->

If an `@overload`-decorated function has a non-trivial body, it likely indicates a misunderstanding
on the part of the user. We emit a warning-level diagnostic to alert them of this.

`...`, `pass` and docstrings are all fine:

```py
from typing import overload

@overload
def x(y: int) -> int: ...
@overload
def x(y: str) -> str:
    """Docstring"""

@overload
def x(y: bytes) -> bytes:
    pass

@overload
def x(y: memoryview) -> memoryview:
    """More docs"""
    pass
    ...

def x(y):
    return y
```

Anything else, however, will trigger the lint:

```py
@overload
def foo(x: int) -> int:
    return x  # error: [useless-overload-body]

@overload
def foo(x: str) -> None:
    """Docstring"""
    pass
    print("oh no, a string")  # error: [useless-overload-body]

def foo(x):
    return x
```

### Inconsistent decorators

#### `@staticmethod`

If one overload signature is decorated with `@staticmethod`, all overload signatures must be
similarly decorated. The implementation, if present, must also have a consistent decorator.

```py
from __future__ import annotations

from typing import overload

class CheckStaticMethod:
    @overload
    def method1(x: int) -> int: ...
    @overload
    def method1(x: str) -> str: ...
    @staticmethod
    # error: [invalid-overload] "Overloaded function `method1` does not use the `@staticmethod` decorator consistently"
    def method1(x: int | str) -> int | str:
        return x

    @overload
    def method2(x: int) -> int: ...
    @overload
    @staticmethod
    def method2(x: str) -> str: ...
    @staticmethod
    # error: [invalid-overload]
    def method2(x: int | str) -> int | str:
        return x

    @overload
    @staticmethod
    def method3(x: int) -> int: ...
    @overload
    @staticmethod
    def method3(x: str) -> str: ...
    # error: [invalid-overload]
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
```

#### `@classmethod`

<!-- snapshot-diagnostics -->

The same rules apply for `@classmethod` as for [`@staticmethod`](#staticmethod).

```py
from __future__ import annotations

from typing import overload

class CheckClassMethod:
    def __init__(self, x: int) -> None:
        self.x = x

    @overload
    @classmethod
    def try_from1(cls, x: int) -> CheckClassMethod: ...
    @overload
    def try_from1(cls, x: str) -> None: ...
    @classmethod
    # error: [invalid-overload] "Overloaded function `try_from1` does not use the `@classmethod` decorator consistently"
    def try_from1(cls, x: int | str) -> CheckClassMethod | None:
        if isinstance(x, int):
            return cls(x)
        return None

    @overload
    def try_from2(cls, x: int) -> CheckClassMethod: ...
    @overload
    @classmethod
    def try_from2(cls, x: str) -> None: ...
    @classmethod
    # error: [invalid-overload]
    def try_from2(cls, x: int | str) -> CheckClassMethod | None:
        if isinstance(x, int):
            return cls(x)
        return None

    @overload
    @classmethod
    def try_from3(cls, x: int) -> CheckClassMethod: ...
    @overload
    @classmethod
    def try_from3(cls, x: str) -> None: ...
    # error: [invalid-overload]
    def try_from3(cls, x: int | str) -> CheckClassMethod | None:
        if isinstance(x, int):
            # error: [call-non-callable]
            return cls(x)
        return None

    @overload
    @classmethod
    def try_from4(cls, x: int) -> CheckClassMethod: ...
    @overload
    @classmethod
    def try_from4(cls, x: str) -> None: ...
    @classmethod
    def try_from4(cls, x: int | str) -> CheckClassMethod | None:
        if isinstance(x, int):
            return cls(x)
        return None
```

#### `@final`

<!-- snapshot-diagnostics -->

If a `@final` decorator is supplied for a function with overloads, the decorator should be applied
only to the overload implementation if it is present.

```py
from typing_extensions import final, overload

class Foo:
    @overload
    def method1(self, x: int) -> int: ...
    @overload
    def method1(self, x: str) -> str: ...
    @final
    def method1(self, x: int | str) -> int | str:
        return x

    @overload
    @final
    def method2(self, x: int) -> int: ...
    @overload
    def method2(self, x: str) -> str: ...
    # error: [invalid-overload]
    def method2(self, x: int | str) -> int | str:
        return x

    @overload
    def method3(self, x: int) -> int: ...
    @overload
    @final
    def method3(self, x: str) -> str: ...
    # error: [invalid-overload]
    def method3(self, x: int | str) -> int | str:
        return x
```

If an overload implementation isn't present (for example, in a stub file), the `@final` decorator
should be applied only to the first overload.

```pyi
from typing_extensions import final, overload

class Foo:
    @overload
    @final
    def method1(self, x: int) -> int: ...
    @overload
    def method1(self, x: str) -> str: ...

    @overload
    def method2(self, x: int) -> int: ...
    @final
    @overload
    # error: [invalid-overload]
    def method2(self, x: str) -> str: ...
```

#### `@override`

<!-- snapshot-diagnostics -->

The same rules apply for `@override` as for [`@final`](#final).

```py
from typing_extensions import overload, override

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
    @overload
    def method(self, x: int) -> int: ...
    @overload
    @override
    def method(self, x: str) -> str: ...
    # error: [invalid-overload]
    def method(self, x: int | str) -> int | str:
        return x

class Sub3(Base):
    @overload
    @override
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...
    # error: [invalid-overload]
    def method(self, x: int | str) -> int | str:
        return x
```

And, similarly, in stub files:

```pyi
from typing_extensions import overload, override

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
    @overload
    def method(self, x: int) -> int: ...
    @overload
    @override
    # error: [invalid-overload]
    def method(self, x: str) -> str: ...
```
