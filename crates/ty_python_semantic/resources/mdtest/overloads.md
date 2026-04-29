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
from typing import Any, Generic, TypeVar, overload

class Foo:
    @overload
    def __init__(self) -> None: ...
    @overload
    def __init__(self, x: int) -> None: ...
    def __init__(self, x: int | None = None) -> None:
        self.x = x

foo = Foo()
reveal_type(foo)  # revealed: Foo
reveal_type(foo.x)  # revealed: int | None

foo1 = Foo(1)
reveal_type(foo1)  # revealed: Foo
reveal_type(foo1.x)  # revealed: int | None

T = TypeVar("T")

class GenericConstructor(Generic[T]):
    @overload
    def __init__(self: "GenericConstructor[list[int]]", value: int) -> None: ...
    @overload
    def __init__(self: "GenericConstructor[set[str]]", value: str) -> None: ...
    @overload
    def __init__(self, value: T) -> None: ...
    def __init__(self, value: Any) -> None:
        pass
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
from __future__ import annotations

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

reveal_type(func)  # revealed: Overload[() -> None, [T](x: T) -> T]
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
# error: [invalid-overload]
def func(x: int) -> int: ...
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
# error: [invalid-overload] "Overloads for function `func` must be followed by a non-`@overload`-decorated implementation function"
def func(x: int) -> int: ...
@overload
def func(x: str) -> str: ...

class Foo:
    @overload
    # error: [invalid-overload] "Overloads for function `method` must be followed by a non-`@overload`-decorated implementation function"
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
    # error: [invalid-overload]
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
    # error: [invalid-overload]
    def f(self, x: int) -> int: ...
    @overload
    def f(self, x: str) -> str: ...

class PartialFoo(ABC):
    @overload
    # error: [invalid-overload]
    def f(self, x: int) -> int: ...
    @overload
    @abstractmethod
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
    import sys

    if sys.platform == "win32":
        pass
    else:
        @overload
        def d() -> bytes: ...
        @overload
        def d(x: int) -> int: ...

if TYPE_CHECKING:
    @overload
    # not all overloads are in a `TYPE_CHECKING` block, so this is an error
    def c() -> None: ...  # error: [invalid-overload]

@overload
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

### Implementation consistency

The overload implementation must accept all arguments accepted by the overloads, and all overload
return types must be assignable to the implementation return type.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import (
    Any,
    Callable,
    Concatenate,
    Coroutine,
    Final,
    Generic,
    Literal,
    ParamSpec,
    Protocol,
    Sequence,
    TypedDict,
    TypeVar,
    Unpack,
    overload,
)

T = TypeVar("T")
TIntStr = TypeVar("TIntStr", int, str)

@overload
def return_type(x: int) -> int: ...
@overload
# error: [invalid-overload] "Overload return type is not assignable to implementation return type"
def return_type(x: str) -> str: ...
def return_type(x: int | str) -> int:
    return 1

@overload
def parameter_type(x: int) -> int: ...
@overload
# error: [invalid-overload] "Implementation does not accept all arguments of this overload"
def parameter_type(x: str) -> str: ...
def parameter_type(x: int) -> int | str:
    return 1

@overload
# error: [invalid-overload]
def mixed_generic(x: T) -> T: ...
@overload
# error: [invalid-overload]
def mixed_generic(x: int) -> int: ...
def mixed_generic(x: str) -> object:
    return x

@overload
# error: [invalid-overload]
def generic_parameter_concrete_return(x: list[T]) -> int: ...
@overload
def generic_parameter_concrete_return(x: str) -> str: ...
def generic_parameter_concrete_return(x: object) -> str:
    return ""

@overload
# error: [invalid-overload]
def generic_parameter_domain(x: list[T]) -> None: ...
@overload
def generic_parameter_domain(x: int) -> None: ...
def generic_parameter_domain(x: list[int] | int) -> None:
    pass

@overload
# error: [invalid-overload]
def generic_parameter_domain_both_generic(x: list[T]) -> None: ...
@overload
def generic_parameter_domain_both_generic(x: int) -> None: ...
def generic_parameter_domain_both_generic(x: tuple[T] | int) -> None:
    pass

@overload
# error: [invalid-overload]
def generic_parameter_domain_upper_bound(x: list[T]) -> None: ...
@overload
def generic_parameter_domain_upper_bound(x: int) -> None: ...
def generic_parameter_domain_upper_bound(x: list[object] | int) -> None:
    pass

@overload
# error: [invalid-overload]
def generic_return(x: list[T]) -> T: ...
@overload
def generic_return(x: int) -> int: ...
def generic_return(x: object) -> int:
    return 1

@overload
# error: [invalid-overload]
def generic_implementation_parameter(x: str) -> None: ...
@overload
# error: [invalid-overload]
def generic_implementation_parameter(x: int) -> None: ...
def generic_implementation_parameter(x: list[T]) -> None:
    pass

@overload
# error: [invalid-overload]
def generic_implementation_return(x: int) -> str: ...
@overload
def generic_implementation_return(x: str) -> str: ...
def generic_implementation_return(x: T) -> T:
    return x

@overload
def keyword_only_generic_return(*, x: int) -> int: ...
@overload
def keyword_only_generic_return(*, x: str) -> str: ...
def keyword_only_generic_return(*, x: T) -> T:
    return x

class Future[T]:
    result: T

@overload
def implementation_return_typevar_identity[R, *Ts](
    target: Callable[[*Ts], Coroutine[Any, Any, R]],
    *args: *Ts,
) -> Future[R] | None: ...
@overload
def implementation_return_typevar_identity[R, *Ts](
    target: Callable[[*Ts], Coroutine[Any, Any, R] | R],
    *args: *Ts,
) -> Future[R] | None: ...
def implementation_return_typevar_identity[R, *Ts](
    target: Callable[[*Ts], Coroutine[Any, Any, R] | R] | Coroutine[Any, Any, R],
    *args: *Ts,
) -> Future[R] | None:
    raise NotImplementedError

class HassJob[**P, R]:
    def __init__(self, target: Callable[P, R]) -> None:
        self.target: Final = target

@overload
def implementation_return_typevar_identity_in_generic_wrapper[R](
    hassjob: HassJob[..., Coroutine[Any, Any, R]],
    *args: Any,
) -> Future[R] | None: ...
@overload
def implementation_return_typevar_identity_in_generic_wrapper[R](
    hassjob: HassJob[..., Coroutine[Any, Any, R] | R],
    *args: Any,
) -> Future[R] | None: ...
def implementation_return_typevar_identity_in_generic_wrapper[R](
    hassjob: HassJob[..., Coroutine[Any, Any, R] | R],
    *args: Any,
) -> Future[R] | None:
    raise NotImplementedError

class Request: ...
class Response: ...
class StreamResponse: ...
class HomeAssistantView: ...

type DecoratorResponse = Response | StreamResponse
type DecoratorFunc[T, **P, R] = Callable[
    Concatenate[T, Request, P],
    Coroutine[Any, Any, R],
]

@overload
def decorator_alias_union_return[
    ViewT: HomeAssistantView,
    **P,
    ResponseT: DecoratorResponse,
](
    func: None = None,
) -> Callable[
    [DecoratorFunc[ViewT, P, ResponseT]],
    DecoratorFunc[ViewT, P, ResponseT],
]: ...
@overload
def decorator_alias_union_return[
    ViewT: HomeAssistantView,
    **P,
    ResponseT: DecoratorResponse,
](
    func: DecoratorFunc[ViewT, P, ResponseT],
) -> DecoratorFunc[ViewT, P, ResponseT]: ...
def decorator_alias_union_return[
    ViewT: HomeAssistantView,
    **P,
    ResponseT: DecoratorResponse,
](
    func: DecoratorFunc[ViewT, P, ResponseT] | None = None,
) -> (
    Callable[
        [DecoratorFunc[ViewT, P, ResponseT]],
        DecoratorFunc[ViewT, P, ResponseT],
    ]
    | DecoratorFunc[ViewT, P, ResponseT]
):
    raise NotImplementedError

@overload
# error: [invalid-overload]
def dynamic_implementation_keyword_name(y: int) -> int: ...
@overload
def dynamic_implementation_keyword_name(x: str) -> str: ...
def dynamic_implementation_keyword_name(x) -> int | str:
    return x

@overload
# error: [invalid-overload]
def dynamic_implementation_self_name_is_not_receiver(y: int) -> int: ...
@overload
# error: [invalid-overload]
def dynamic_implementation_self_name_is_not_receiver(x: str) -> str: ...
def dynamic_implementation_self_name_is_not_receiver(self) -> int | str:
    return self

@overload
# error: [invalid-overload]
def dynamic_implementation_mixed_keyword_call(x: int, y: int) -> int: ...
@overload
# error: [invalid-overload]
def dynamic_implementation_mixed_keyword_call(x: str, y: str) -> str: ...
def dynamic_implementation_mixed_keyword_call(y, x) -> int | str:
    return x

@overload
# error: [invalid-overload]
def dynamic_implementation_omitted_default(x: int = 0) -> int: ...
@overload
def dynamic_implementation_omitted_default(x: str) -> str: ...
def dynamic_implementation_omitted_default(x) -> int | str:
    return x

@overload
# error: [invalid-overload]
def dynamic_implementation_variadic_arguments(*args: int) -> int: ...
@overload
def dynamic_implementation_variadic_arguments(x: str) -> str: ...
def dynamic_implementation_variadic_arguments(x) -> int | str:
    return x

@overload
def generic_container_implementation(x: list[int]) -> int: ...
@overload
def generic_container_implementation(x: list[str]) -> str: ...
def generic_container_implementation(x: list[T]) -> T:
    return x[0]

@overload
def constrained_implementation_parameter(x: int) -> object: ...
@overload
# error: [invalid-overload]
def constrained_implementation_parameter(x: bytes) -> object: ...
def constrained_implementation_parameter(x: TIntStr) -> object:
    return x

@overload
def constrained_nested_implementation_parameter(x: list[int]) -> object: ...
@overload
# error: [invalid-overload]
def constrained_nested_implementation_parameter(x: list[bytes]) -> object: ...
def constrained_nested_implementation_parameter(x: list[TIntStr]) -> object:
    return x

@overload
def constrained_nested_overload_parameter(x: list[TIntStr]) -> object: ...
@overload
def constrained_nested_overload_parameter(x: list[bytes]) -> object: ...
def constrained_nested_overload_parameter(x: list[int] | list[str] | list[bytes]) -> object:
    return x

class Box(Generic[T]):
    @overload
    def method(self, x: int) -> int: ...
    @overload
    # error: [invalid-overload]
    def method(self, x: str) -> str: ...
    def method(self, x: int) -> int | str:
        return x

class StaticmethodSelfIsExplicit:
    @overload
    @staticmethod
    # error: [invalid-overload]
    def method(y: int) -> int: ...
    @overload
    @staticmethod
    # error: [invalid-overload]
    def method(x: str) -> str: ...
    @staticmethod
    def method(self) -> int | str:
        return self

@overload
def generic_union_implementation(x: list[T]) -> T: ...
@overload
def generic_union_implementation(x: None) -> None: ...
def generic_union_implementation(x: list[T] | None) -> T | None:
    return None if x is None else x[0]

Row_co = TypeVar("Row_co", covariant=True)

class Connection(Generic[Row_co]): ...

class RowFactory(Protocol[Row_co]):
    def __call__(self) -> Row_co: ...

class PsycopgCursor(Generic[Row_co]):
    @overload
    def __init__(self, connection: Connection[Row_co]) -> None: ...
    @overload
    def __init__(self, connection: Connection[Any], *, row_factory: RowFactory[Row_co]) -> None: ...
    def __init__(self, connection: Connection[Any], *, row_factory: RowFactory[Row_co] | None = None) -> None: ...

@overload
def literal_keyword_implementation(x: int, *, flag: Literal[True]) -> int: ...
@overload
def literal_keyword_implementation(x: str, *, flag: bool = False) -> str: ...
def literal_keyword_implementation(x: int | str, *, flag: bool = False) -> int | str:
    return x

class Dataset: ...
class DataArray: ...

DatasetOrDataArrayT = TypeVar("DatasetOrDataArrayT", bound=Dataset | DataArray)
DatasetOrDataArrayU = TypeVar("DatasetOrDataArrayU", bound=Dataset | DataArray)
DatasetOrDataArrayV = TypeVar("DatasetOrDataArrayV", bound=Dataset | DataArray)

@overload
def bounded_typevar_variadic_implementation(obj: DatasetOrDataArrayT, /) -> tuple[DatasetOrDataArrayT]: ...
@overload
def bounded_typevar_variadic_implementation(
    obj1: DatasetOrDataArrayT,
    obj2: DatasetOrDataArrayU,
    /,
) -> tuple[DatasetOrDataArrayT, DatasetOrDataArrayU]: ...
@overload
def bounded_typevar_variadic_implementation(
    obj1: DatasetOrDataArrayT,
    obj2: DatasetOrDataArrayU,
    obj3: DatasetOrDataArrayV,
    /,
) -> tuple[DatasetOrDataArrayT, DatasetOrDataArrayU, DatasetOrDataArrayV]: ...
@overload
def bounded_typevar_variadic_implementation(*objects: Dataset | DataArray) -> tuple[Dataset | DataArray, ...]: ...
def bounded_typevar_variadic_implementation(*objects: Dataset | DataArray) -> tuple[Dataset | DataArray, ...]:
    raise NotImplementedError

class Command(Generic[T]): ...

AnyCommandT = TypeVar("AnyCommandT", bound=Command[Any])
IntCommandT = TypeVar("IntCommandT", bound=Command[int])
StrCommandT = TypeVar("StrCommandT", bound=Command[str])

class Hooks(Generic[T]):
    @overload
    def add(self: "Hooks[int]", command: IntCommandT) -> IntCommandT: ...
    @overload
    def add(self: "Hooks[str]", command: StrCommandT) -> StrCommandT: ...
    def add(self, command: AnyCommandT) -> AnyCommandT:
        return command

class Builds(Protocol[T]): ...

R = TypeVar("R")
P = ParamSpec("P")

class BuildsWithSig(Builds[T], Protocol[T, P]): ...

Importable = TypeVar("Importable", bound=Callable[..., Any])

class BuildFactory:
    @overload
    def __call__(
        self,
        target: type[BuildsWithSig[type[R], P]],
        *,
        populate_full_signature: Literal[True],
        partial: Literal[False, None] = ...,
    ) -> type[BuildsWithSig[type[R], P]]: ...
    @overload
    def __call__(
        self,
        target: Callable[P, R],
        *,
        populate_full_signature: Literal[True],
        partial: Literal[False, None] = ...,
    ) -> type[BuildsWithSig[type[R], P]]: ...
    @overload
    def __call__(
        self,
        target: Importable,
        *args: T,
        populate_full_signature: bool = ...,
        partial: Literal[False, None] = ...,
        **kwargs: T,
    ) -> type[Builds[Importable]]: ...
    def __call__(
        self,
        target: Callable[P, R] | type[Builds[Importable]] | type[BuildsWithSig[type[R], P]],
        *args: T,
        populate_full_signature: bool = False,
        partial: bool | None = None,
        **kwargs: T,
    ) -> Any:
        return Builds

class ClassTypeVarBox[T]:
    @overload
    # error: [invalid-overload]
    def method(self, x: int) -> None: ...
    @overload
    # error: [invalid-overload]
    def method(self, x: str) -> None: ...
    def method(self, x: T) -> None:
        pass

class SpecializedSelfBox[T]:
    # Like mypy and pyright, we do not re-specialize the implementation separately
    # for each overload based on the overload's explicit `self` annotation.
    @overload
    # error: [invalid-overload]
    def method(self: "SpecializedSelfBox[int]", x: int) -> int: ...
    @overload
    # error: [invalid-overload]
    def method(self: "SpecializedSelfBox[str]", x: str) -> str: ...
    def method(self, x: T) -> T:
        return x

class ReadSharedKwds(TypedDict, total=False):
    sep: str
    header: int

@overload
# error: [invalid-overload]
def typed_dict_kwargs_explicit_implementation(path: str, **kwds: Unpack[ReadSharedKwds]) -> int: ...
@overload
# error: [invalid-overload]
def typed_dict_kwargs_explicit_implementation(path: bytes, **kwds: Unpack[ReadSharedKwds]) -> str: ...
def typed_dict_kwargs_explicit_implementation(
    path: str | bytes,
    *,
    sep: str = ",",
    header: int = 0,
) -> int | str:
    return 1

@overload
def paramspec_overload_gradual_variadic_implementation(func: Callable[P, T], *args: P.args, **kwargs: P.kwargs) -> T: ...
@overload
def paramspec_overload_gradual_variadic_implementation(func: Callable[..., T]) -> T: ...
def paramspec_overload_gradual_variadic_implementation(func: Callable[..., T], *args: Any, **kwargs: Any) -> T:
    return func(*args, **kwargs)

@overload
def paramspec_overload_with_static_prefix_gradual_variadic_implementation(
    obj: T,
    func: Callable[P, T],
    *args: P.args,
    **kwargs: P.kwargs,
) -> T: ...
@overload
def paramspec_overload_with_static_prefix_gradual_variadic_implementation(
    obj: Any,
    func: tuple[Callable[..., T], str],
    *args: Any,
    **kwargs: Any,
) -> T: ...
def paramspec_overload_with_static_prefix_gradual_variadic_implementation(
    obj: T,
    func: Callable[P, T] | tuple[Callable[..., T], str],
    *args: Any,
    **kwargs: Any,
) -> T:
    return obj

@overload
def paramspec_kwargs_only_implementation(
    func: Callable[P, T],
    **kwargs: P.kwargs,  # error: [invalid-paramspec]
) -> T: ...
@overload
def paramspec_kwargs_only_implementation(
    func: Callable[P, Sequence[T]],
    **kwargs: P.kwargs,  # error: [invalid-paramspec]
) -> tuple[T, ...]: ...
def paramspec_kwargs_only_implementation(
    func: Callable[P, T | Sequence[T]],
    **kwargs: P.kwargs,  # error: [invalid-paramspec]
) -> T | tuple[T, ...]:
    return ()

@overload
def structured_gradual_callable_variadic_implementation(fn: Callable[[int], str], /) -> Any: ...
@overload
def structured_gradual_callable_variadic_implementation(
    fn: Callable[[int], str],
    other: Callable[[str], bytes],
    /,
) -> Any: ...
def structured_gradual_callable_variadic_implementation(*fns: Callable[[Any], Any]) -> Any:
    return fns[0]

@overload
def prefixed_structured_gradual_callable_variadic_implementation(value: int, /) -> int: ...
@overload
def prefixed_structured_gradual_callable_variadic_implementation(value: int, fn: Callable[[int], str], /) -> str: ...
@overload
def prefixed_structured_gradual_callable_variadic_implementation(
    value: int,
    fn: Callable[[int], str],
    other: Callable[[str], bytes],
    /,
) -> bytes: ...
def prefixed_structured_gradual_callable_variadic_implementation(
    value: Any,
    *fns: Callable[[Any], Any],
) -> Any:
    return value

@overload
def ellipsis_callable_variadic_implementation(fn: Callable[[int], str], /) -> Any: ...
@overload
def ellipsis_callable_variadic_implementation(
    fn: Callable[[int], str],
    other: Callable[[str], bytes],
    /,
) -> Any: ...
def ellipsis_callable_variadic_implementation(*fns: Callable[..., Any]) -> Any:
    return fns[0]
```

### Inserted positional implementation parameter before variadic arguments

An overload with `*args: Any` accepts arbitrary extra positional arguments. If the implementation
inserts an annotated optional positional parameter before its own `*args`, those same overload
arguments bind to the inserted parameter in the implementation, not to the variadic parameter. The
implementation must therefore accept the inserted parameter's type for every extra positional
argument that the overload accepts.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Any, TypeVar, overload

T = TypeVar("T")

@overload
# error: [invalid-overload]
def command(name: str = ..., *args: Any) -> int: ...
@overload
def command(name: str = ..., cls: type[T] = ..., *args: Any) -> T: ...
def command(
    name: str = "",
    cls: type[Any] | None = None,
    *args: Any,
) -> Any:
    return None
```

### Positional-only implementation parameters before overload keyword parameter

An overload parameter after `/` is still positional-or-keyword. If the implementation inserts
additional positional-only parameters before that parameter, a positional call accepted by the
overload binds to a different implementation parameter.

```py
from typing import overload

@overload
# error: [invalid-overload]
def timestamp(value: object, /, timezone: str | None = None) -> str: ...
@overload
def timestamp(year: int, month: int, day: int, /) -> str: ...
def timestamp(
    value_or_year: object,
    month: int | None = None,
    day: int | None = None,
    /,
    timezone: str | None = None,
) -> str:
    return ""
```

### Invariant generic wrapper implementation parameter

An implementation parameter using an invariant generic wrapper around a union does not accept an
overload parameter using the same wrapper around one member of that union.

```py
from collections.abc import Coroutine
from typing import Any, Generic, ParamSpec, TypeVar, overload

P = ParamSpec("P")
R = TypeVar("R")

class Task(Generic[P, R]): ...
class Future(Generic[R]): ...

class TaskRunner:
    @overload
    # error: [invalid-overload]
    def submit(
        self,
        task: Task[P, Coroutine[Any, Any, R]],
        *args: P.args,
        **kwargs: P.kwargs,
    ) -> Future[R]: ...
    @overload
    def submit(
        self,
        task: Task[P, R],
        *args: P.args,
        **kwargs: P.kwargs,
    ) -> Future[R]: ...
    def submit(
        self,
        task: Task[P, R | Coroutine[Any, Any, R]],
        *args: P.args,
        **kwargs: P.kwargs,
    ) -> Future[R]:
        raise NotImplementedError
```

### Callable return type variance

An overload return type must be assignable to the implementation return type. Callable parameter
types are contravariant, so an implementation returning a callable that may require either `int` or
`str` is not compatible with an overload that promises a callable accepting only `int`.

```py
from collections.abc import Callable
from typing import Generic, Literal, TypeVar, overload

D = TypeVar("D", Literal[0], Literal[1])

class Wrapper(Generic[D]): ...

@overload
# error: [invalid-overload]
def decorator(
    dimensionality: Literal[0] = 0,
) -> Callable[[Callable[[int], int]], Wrapper[Literal[0]]]: ...
@overload
# error: [invalid-overload]
def decorator(
    dimensionality: Literal[1],
) -> Callable[[Callable[[str], str]], Wrapper[Literal[1]]]: ...
def decorator(
    dimensionality: D | Literal[0] = 0,
) -> Callable[
    [Callable[[int], int] | Callable[[str], str]],
    Wrapper[D | Literal[0]],
]:
    raise NotImplementedError
```

### Optional positional implementation parameter

An implementation with an optional positional parameter accepts calls that omit that parameter,
including an overload that only exposes keyword-only parameters.

```py
from typing import overload

class CallableWithOptionalParameter:
    @overload
    def __call__(self, *, flag: bool = ...) -> int: ...
    @overload
    def __call__(self, __value: int, *, flag: bool = ...) -> int: ...
    def __call__(self, __value: int | None = None, *, flag: bool = False) -> int:
        return 1
```

### Specialized receiver annotations

Explicit `self` annotations on overloads narrow the overload call surface, but ty follows mypy and
pyright in not re-specializing the implementation separately for each overload.

```py
from typing import Generic, Literal, TypeVar, overload

Choice = TypeVar("Choice", Literal[0], Literal[1])

class Zero: ...
class One: ...

class SpecializedReceiver(Generic[Choice]):
    @overload
    # error: [invalid-overload]
    def method(self: "SpecializedReceiver[Literal[0]]") -> Zero: ...
    @overload
    # error: [invalid-overload]
    def method(self: "SpecializedReceiver[Literal[1]]") -> One: ...
    def method(self: "SpecializedReceiver[Choice]") -> Zero | One:
        raise NotImplementedError
```

### Special method parameter names

Special methods can still be called directly. A mismatched overload parameter name is therefore
observable because `obj.__getitem__(index=...)` is accepted by the overload but not by the
implementation.

```py
from typing import overload

class Diff:
    @overload
    # error: [invalid-overload]
    def __getitem__(self, index: int) -> int: ...
    @overload
    def __getitem__(self, item: str) -> str: ...
    def __getitem__(self, item: int | str) -> int | str:
        return 1
```

### `Unpack[TypedDict]` implementation expansion

We intentionally do not treat an implementation's explicit keyword-only parameters as equivalent to
an overload `**kwargs: Unpack[TypedDict]` parameter during overload implementation consistency.

```py
from typing import overload
from typing_extensions import TypedDict, Unpack

class Kwargs(TypedDict, total=False):
    key: int

@overload
# error: [invalid-overload]
def unpacked_kwargs(**kwargs: Unpack[Kwargs]) -> None: ...
@overload
def unpacked_kwargs() -> None: ...
def unpacked_kwargs(*, key: int = 0) -> None:
    pass
```

### Async implementation return

For `async def` overloads, implementation consistency compares the coroutine result type, not the
outer coroutine object type.

```py
from collections.abc import Awaitable
from typing import TypeVar, overload

T = TypeVar("T")

@overload
async def async_identity(value: Awaitable[T]) -> T: ...
@overload
async def async_identity(value: T) -> T: ...
async def async_identity(value: T | Awaitable[T]) -> T:
    raise NotImplementedError
```

### Awaitable callback return

When a callback can return either `T` or `Awaitable[T]`, the overload return type must preserve the
same `T` through an invariant generic wrapper.

```toml
[environment]
python-version = "3.12"
```

```py
from asyncio import Future
from collections.abc import Awaitable, Callable
from typing import TypeVar, overload

T = TypeVar("T")
S = TypeVar("S")

class Thenable(Future[T]):
    @overload
    def then(self, callback: Callable[[T], Awaitable[S]]) -> "Thenable[S]": ...
    @overload
    def then(self, callback: Callable[[T], S]) -> "Thenable[S]": ...
    def then(self, callback: Callable[[T], S | Awaitable[S]]) -> "Thenable[S]":
        raise NotImplementedError
```

### Positional-only implementation parameter

A defaulted positional-only implementation parameter still accepts calls that omit it, including an
overload that only accepts keyword arguments.

```py
from typing import overload

class PositionalOnlyImplementation:
    @overload
    def update(self, value: int, /) -> None: ...
    @overload
    def update(self, **kwargs: int) -> None: ...
    def update(self, value: object = (), /, **kwargs: object) -> None:
        pass
```

### `TypeIs` return

`TypeIs` uses a top-materialized type for narrowing, but overload implementation consistency still
relates the declared `TypeIs` argument so a gradual implementation return remains compatible.

```py
from collections.abc import Iterable, Sequence
from typing import Any, TypeVar, overload
from typing_extensions import TypeIs

T = TypeVar("T")

@overload
def is_sequence(value: Iterable[T]) -> TypeIs[Sequence[T]]: ...
@overload
def is_sequence(value: object) -> TypeIs[Sequence[Any]]: ...
def is_sequence(value: object) -> TypeIs[Sequence[Any]]:
    return isinstance(value, Sequence)
```

### `TypeGuard` return with specialized receivers

`TypeGuard` is covariant in its guarded type, so an implementation return can stay generic over the
class type parameter while overloads use specialized receiver types.

```py
from typing import Generic, TypeGuard, TypeVar, overload

T_co = TypeVar("T_co", bound=BaseException, covariant=True)
E = TypeVar("E", bound=Exception)
B = TypeVar("B", bound=BaseException)

class Guarded(Generic[T_co]): ...

class Receiver(Generic[T_co]):
    @overload
    def accepts(self: "Receiver[E]", value: Exception) -> TypeGuard[Guarded[E]]: ...
    @overload
    def accepts(self: "Receiver[B]", value: BaseException) -> TypeGuard[Guarded[B]]: ...
    def accepts(self, value: BaseException) -> TypeGuard[Guarded[T_co]]:
        raise NotImplementedError
```

### Implementation consistency parameter mismatch diagnostics

Non-generic implementation checks require parameter names and positional-only forms to line up with
each overload signature.

```toml
[environment]
python-version = "3.12"
```

```py
from collections.abc import Iterable
from typing import overload

class ColumnSelector:
    @overload
    def _extract(self, row_key: int) -> object: ...
    @overload
    # snapshot: invalid-overload
    def _extract(self, column_key: int) -> object: ...
    def _extract(self, row_key: int | None = None, column_key: int | None = None) -> object:
        return object()

class PositionalOnlyWithKwargs:
    @overload
    def update(self, params: Iterable[tuple[str, str | Iterable[str]]], /, **kwds: str) -> None: ...
    @overload
    # snapshot: invalid-overload
    def update(self, **kwds: str | Iterable[str]) -> None: ...
    def update(self, params=(), /, **kwds) -> None:
        pass
```

```snapshot
error[invalid-overload]: Implementation does not accept all arguments of this overload
  --> src/mdtest_snippet.py:9:9
   |
 9 |     def _extract(self, column_key: int) -> object: ...
   |         ^^^^^^^^
10 |     def _extract(self, row_key: int | None = None, column_key: int | None = None) -> object:
   |         -------- Implementation defined here
   |
info: Implementation signature `(self, row_key: int | None = None, column_key: int | None = None) -> object` is not assignable to overload signature `(self, column_key: int) -> object`
info: the parameter named `row_key` does not match `column_key` (and can be used as a keyword parameter)


error[invalid-overload]: Implementation does not accept all arguments of this overload
  --> src/mdtest_snippet.py:18:9
   |
18 |     def update(self, **kwds: str | Iterable[str]) -> None: ...
   |         ^^^^^^
19 |     def update(self, params=(), /, **kwds) -> None:
   |         ------ Implementation defined here
   |
info: Implementation signature `(self, params=..., /, **kwds) -> None` is not assignable to overload signature `(self, **kwds: Iterable[str]) -> None`
info: parameter `self` is positional-only but must also accept keyword arguments
```

### Implementation consistency return type diagnostics

Non-generic implementation checks show why an overload return type is not assignable to the
implementation return type.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import overload

@overload
# snapshot: invalid-overload
def return_tuple(x: int) -> tuple[str]: ...
@overload
def return_tuple(x: str) -> tuple[int]: ...
def return_tuple(x: int | str) -> tuple[int]:
    return (1,)
```

```snapshot
error[invalid-overload]: Overload return type is not assignable to implementation return type
 --> src/mdtest_snippet.py:5:5
  |
5 | def return_tuple(x: int) -> tuple[str]: ...
  |     ^^^^^^^^^^^^
6 | @overload
7 | def return_tuple(x: str) -> tuple[int]: ...
8 | def return_tuple(x: int | str) -> tuple[int]:
  |     ------------ Implementation defined here
  |
info: Overload returns `tuple[str]`, which is not assignable to implementation return type `tuple[int]`
info: the first tuple element is not compatible: `str` is not assignable to `int`
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
    # error: [invalid-overload]
    def method2(self, x: int) -> int: ...
    @overload
    def method2(self, x: str) -> str: ...
    def method2(self, x: int | str) -> int | str:
        return x

    @overload
    def method3(self, x: int) -> int: ...
    @overload
    @final
    # error: [invalid-overload]
    def method3(self, x: str) -> str: ...
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
    @overload
    def method3(self, x: int) -> int: ...
    @final
    @overload
    def method3(self, x: str) -> int: ...  # error: [invalid-overload]
    @overload
    @final
    def method3(self, x: bytes) -> bytes: ...  # error: [invalid-overload]
    @overload
    def method3(self, x: bytearray) -> bytearray: ...
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
    # error: [invalid-overload]
    def method(self, x: str) -> str: ...
    def method(self, x: int | str) -> int | str:
        return x

class Sub3(Base):
    @overload
    @override
    # error: [invalid-overload]
    def method(self, x: int) -> int: ...
    @overload
    def method(self, x: str) -> str: ...
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

### Regression: `def` statement shadows a non-`def` symbol with the same name

We used to panic on snippets like these (see <https://github.com/astral-sh/ty/issues/1867>), because
"iterating over the overloads" for the `def` statement would incorrectly list the overloads of the
imported function.

`module.pyi`:

```pyi
from typing import overload

@overload
def f() -> int: ...
@overload
def f(x) -> str: ...
@overload
def g() -> int: ...
@overload
def g(x) -> str: ...
```

`main.py`:

```py
import module

foo = module.f

# revealed: Overload[() -> int, (x) -> str]
reveal_type(foo)

def foo(): ...

# revealed: def foo() -> Unknown
reveal_type(foo)

bar = module.g

# revealed: Overload[() -> int, (x) -> str]
reveal_type(bar)

@staticmethod
def bar(): ...

# revealed: def bar() -> Unknown
reveal_type(bar)
```

### Regression: `def` statement shadows a non-`def` symbol with the same name, defined in the same scope

This is an even more pathological version of the above test. This version used to fail in the same
way as the above snippet, but would only fail in a stub file, or in a `.py` file that had an
overloaded function without an implementation. (Note that this is not always invalid even in `.py`
files: we allow overloaded functions to omit the implementation function if they are decorated with
`@abstractmethod` or they are defined in `if TYPE_CHECKING` blocks.)

```pyi
from typing import overload

@overload
def h() -> int: ...
@overload
def h(x) -> str: ...

baz = h

# revealed: Overload[() -> int, (x) -> str]
reveal_type(baz)

# This function is distinct from `h`, despite `h` originating
# from the same scope and being aliased to the same name
# in the same scope!
@overload
def baz(x, y) -> bytes: ...
@overload
def baz(x, y, z) -> list[str]: ...
def baz(x, y, z=None) -> bytes | list[str]:
    return b""

# revealed: Overload[(x, y) -> bytes, (x, y, z) -> list[str]]
reveal_type(baz)
```
