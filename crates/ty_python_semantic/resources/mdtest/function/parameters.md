# Function parameter types

Within a function scope, the declared type of each parameter is its annotated type (or Unknown if
not annotated). The initial inferred type is the annotated type of the parameter, if any. If there
is no annotation, it is the union of `Unknown` with the type of the default value expression (if
any).

The variadic parameter is a variadic tuple of its annotated type; the variadic-keywords parameter is
a dictionary from strings to its annotated type.

## Parameter kinds

```py
from typing import Literal

def f(a, b: int, c=1, d: int = 2, /, e=3, f: Literal[4] = 4, *args: object, g=5, h: Literal[6] = 6, **kwargs: str):
    reveal_type(a)  # revealed: Unknown
    reveal_type(b)  # revealed: int
    reveal_type(c)  # revealed: Unknown | Literal[1]
    reveal_type(d)  # revealed: int
    reveal_type(e)  # revealed: Unknown | Literal[3]
    reveal_type(f)  # revealed: Literal[4]
    reveal_type(g)  # revealed: Unknown | Literal[5]
    reveal_type(h)  # revealed: Literal[6]
    reveal_type(args)  # revealed: tuple[object, ...]
    reveal_type(kwargs)  # revealed: dict[str, str]
```

## Unannotated variadic parameters

...are inferred as tuple of Unknown or dict from string to Unknown.

```py
def g(*args, **kwargs):
    reveal_type(args)  # revealed: tuple[Unknown, ...]
    reveal_type(kwargs)  # revealed: dict[str, Unknown]
```

## Annotation is present but not a fully static type

If there is an annotation, we respect it fully and don't union in the default value type.

```py
from typing import Any

def f(x: Any = 1):
    reveal_type(x)  # revealed: Any
```

## Default value type must be assignable to annotated type

The default value type must be assignable to the annotated type. If not, we emit a diagnostic, and
fall back to inferring the annotated type, ignoring the default value type.

```py
# error: [invalid-parameter-default]
def f(x: int = "foo"):
    reveal_type(x)  # revealed: int

# The check is assignable-to, not subtype-of, so this is fine:
from typing import Any

def g(x: Any = "foo"):
    reveal_type(x)  # revealed: Any
```

## Method receiver type must accept the enclosing class

```toml
[environment]
python-version = "3.13"
```

```py
from __future__ import annotations

from typing import Any, LiteralString, Never, Protocol, Self, TypeVar, Unpack, final, overload

class Parent: ...
class Unrelated: ...

T = TypeVar("T")
T_Parent = TypeVar("T_Parent", bound=Parent)
T_Unrelated = TypeVar("T_Unrelated", bound=Unrelated)

class Foo(Parent):
    # error: [invalid-method-receiver]
    def invalid(self: int): ...

    # error: [invalid-method-receiver]
    def invalid_class_object(self: type[Foo]): ...

    # error: [invalid-method-receiver]
    def invalid_bound_typevar(self: T_Unrelated): ...

    # error: [invalid-method-receiver]
    def invalid_constrained_typevar[T: (int, str)](self: T): ...
    def valid_exact(self: Foo): ...
    def valid_self(self: Self): ...
    def valid_parent(self: Parent): ...
    def valid_object(self: object): ...
    def valid_any(self: Any): ...
    def valid_never(self: Never): ...
    def valid_unbound_typevar(self: T): ...
    def valid_bound_typevar(self: T_Parent): ...
    @classmethod
    # error: [invalid-method-receiver]
    def invalid_classmethod(cls: Foo): ...
    @classmethod
    # error: [invalid-method-receiver]
    def invalid_classmethod_parent(cls: type[Unrelated]): ...
    @classmethod
    def valid_classmethod(cls: type[Foo]): ...
    @classmethod
    def valid_classmethod_parent(cls: type[Parent]): ...

    # error: [invalid-method-receiver]
    def __new__(cls: Foo): ...
    @staticmethod
    def static(value: int): ...
    # error: [invalid-method-receiver]
    def restricted(self: FooChild): ...
    @overload
    def overloaded(self: FooChild, value: int) -> int: ...
    @overload
    def overloaded(self, value: str) -> str: ...
    def overloaded(self, value: int | str) -> int | str:
        return value

class GenericClass[T]:
    # error: [invalid-method-receiver]
    def invalid(self: T): ...
    # TODO: error: [invalid-method-receiver]
    # Class-scoped type variables nested in unions are erased during signature normalization.
    def invalid_union(self: T | int): ...
    def valid_union(self: T | GenericClass[T]): ...

class OuterGenericClass[T]:
    class Inner:
        # error: [invalid-method-receiver]
        def invalid(self: T): ...

class FooChild(Foo): ...

class ReceiverProtocol(Protocol):
    attribute: int

T_ReceiverProtocol = TypeVar("T_ReceiverProtocol", bound=ReceiverProtocol)

class Mixin:
    def method(self: ReceiverProtocol) -> int:
        return self.attribute

    def generic_method(self: T_ReceiverProtocol) -> int:
        return self.attribute

class ProtocolClass(Protocol):
    def method(self: int): ...

class ReceiverClassProtocol(Protocol):
    value: object

class GenericReceiverClassProtocol[T](Protocol):
    value: T

type GenericReceiverAlias[T] = type[GenericReceiverClassProtocol[T]] | type[str]

class InvalidProtocolClassReceiver:
    @classmethod
    # TODO: error: [invalid-method-receiver]
    # `type[Protocol]` is currently represented using a `Todo` type.
    def method(cls: type[ReceiverClassProtocol]): ...

class ValidGenericProtocolClassReceiver:
    value: type[int] = int
    @classmethod
    def method(cls: GenericReceiverAlias[type[int]]): ...

class InvalidGenericProtocolClassReceiver:
    value: int = 1
    @classmethod
    # TODO: error: [invalid-method-receiver]
    def method(cls: GenericReceiverAlias[type[int]]): ...

class StrSubclass(str):
    # error: [invalid-method-receiver]
    def method(self: LiteralString): ...

class EnumLike:
    def _generate_next_value_(name: str, start: int, count: int, last_values: list[object]): ...

def nested():
    def not_a_method(value: int): ...

class HasNestedFunction:
    def outer(self):
        def __new__(value: int): ...

class RestrictedMeta(type):
    def restricted(cls: type[int]): ...
    def restricted_to_final(cls: type[FinalClass]): ...
    def restricted_to_union(cls: type[FinalClass] | type[OtherFinalClass]): ...
    def restricted_to_typevar[T: type[FinalClass]](cls: T): ...
    @classmethod
    # error: [invalid-method-receiver]
    def invalid_classmethod(cls: type[int]): ...
    def __new__(
        cls: type[int],  # error: [invalid-method-receiver]
        name: str,
        bases: tuple[type, ...],
        namespace: dict[str, Any],
    ): ...

class GenericRestrictedMeta[T: type[int]](type):
    # error: [invalid-method-receiver]
    def restricted(cls: T): ...

@final
class FinalClass(metaclass=RestrictedMeta): ...

@final
class OtherFinalClass(metaclass=RestrictedMeta): ...

class VariadicReceiver:
    # TODO: error: [invalid-method-receiver]
    def invalid(*args: int): ...
    # TODO: error: [invalid-method-receiver]
    def invalid_keywords(**kwargs: object): ...
    def valid(*args: object): ...
    def valid_fixed(*args: Unpack[tuple[VariadicReceiver, int]]): ...
```

## TypedDict defaults use annotation context

```py
from typing import TypedDict

class Foo(TypedDict):
    x: int

def x(a: Foo = {"x": 42}): ...
def y(a: Foo = dict(x=42)): ...
```

## TypedDict defaults still validate keys and value types

```py
from typing import TypedDict

class Foo(TypedDict):
    x: int
    y: int

# error: [missing-typed-dict-key]
def missing_key(a: Foo = {"x": 42}): ...

# error: [invalid-argument-type]
def wrong_type(a: Foo = {"x": "s", "y": 1}): ...

# error: [invalid-key]
def extra_key(a: Foo = {"x": 1, "y": 2, "z": 3}): ...
```

## Stub functions

```toml
[environment]
python-version = "3.12"
```

### In Protocol

```py
from typing import Protocol

class Foo(Protocol):
    def x(self, y: bool = ...): ...
    def y[T](self, y: T = ...) -> T: ...

class GenericFoo[T](Protocol):
    def x(self, y: bool = ...) -> T: ...
```

### In abstract method

```py
from abc import abstractmethod

class Bar:
    @abstractmethod
    def x(self, y: bool = ...): ...
    @abstractmethod
    def y[T](self, y: T = ...) -> T: ...
```

### In function overload

```py
from typing import overload

@overload
def x(y: None = ...) -> None: ...
@overload
def x(y: int) -> str: ...
def x(y: int | None = None) -> str | None: ...
```

### In `if TYPE_CHECKING` blocks

We generally view code in `if TYPE_CHECKING` blocks as having the same semantics and exemptions to
code in stub files:

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    def foo(x: bool = ...): ...  # fine
```
