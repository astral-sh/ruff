# Dataclass fields

## Basic

```py
from dataclasses import dataclass, field

@dataclass
class Member:
    name: str
    role: str = field(default="user")
    tag: str | None = field(default=None, init=False)

# revealed: (self: Member, name: str, role: str = "user") -> None
reveal_type(Member.__init__)

alice = Member(name="Alice", role="admin")
reveal_type(alice.role)  # revealed: str
alice.role = "moderator"

# `tag` is marked as `init=False`, so this is an
# error: [unknown-argument] "Argument `tag` does not match any known parameter"
bob = Member(name="Bob", tag="VIP")
```

## Inheritance with defaults

```py
from dataclasses import dataclass

class Configuration: ...

@dataclass(frozen=True)
class SomeClass:
    config: Configuration | None

    def foo(self) -> int:
        raise NotImplementedError

class SpecificConfiguration(Configuration):
    x: int = 0

@dataclass(frozen=True)
class SpecificClass(SomeClass):
    config: SpecificConfiguration | None = None

    def foo(self) -> int:
        if self.config is None:
            return SpecificConfiguration().x
        return self.config.x

reveal_type(SpecificClass().config)  # revealed: SpecificConfiguration | None

@dataclass(frozen=True)
class NoDefaultSpecificClass(SomeClass):
    config: SpecificConfiguration | None

reveal_type(NoDefaultSpecificClass(SpecificConfiguration()).config)  # revealed: SpecificConfiguration | None
```

## Descriptor-typed fields

A dataclass field whose declared type is a descriptor should still resolve through the descriptor
protocol correctly on instance access. Whether the descriptor protocol is involved depends on
whether the descriptor object is present in the class namespace.

Data descriptors should use their `__get__` return type on instances and preserve the descriptor
type on class access:

```py
from dataclasses import dataclass
from typing import Any, overload

class Desc1:
    @overload
    def __get__(self, instance: None, owner: Any) -> "Desc1": ...
    @overload
    def __get__(self, instance: object, owner: Any) -> int: ...
    def __get__(self, instance: object | None, owner: Any) -> "Desc1 | int":
        raise NotImplementedError

    def __set__(self, instance: object, value: int) -> None:
        raise NotImplementedError

@dataclass
class DC1:
    y: Desc1 = Desc1()

dc1 = DC1(3)
reveal_type(dc1.y)  # revealed: int
reveal_type(DC1.y)  # revealed: Desc1
```

`Desc2` has `__get__` but no `__set__`, making it a non-data descriptor.

Without a class-body default, the descriptor object is not present in the class namespace, so the
dataclass initializer stores the constructor argument directly on the instance:

```py
from dataclasses import dataclass
from typing import Any, Generic, TypeVar, overload

T = TypeVar("T")

class Desc2(Generic[T]):
    @overload
    def __get__(self, instance: None, owner: Any) -> list[T]: ...
    @overload
    def __get__(self, instance: object, owner: Any) -> T: ...
    def __get__(self, instance: object | None, owner: Any) -> list[T] | T:
        raise NotImplementedError

@dataclass
class DC2:
    x: Desc2[int]
    y: Desc2[str]

dc2 = DC2(Desc2(), Desc2())

# Without a class-body binding, the class has no descriptor attribute.
# error: [unresolved-attribute]
DC2.x

# The synthesized `__init__` stores the constructor arguments as instance
# attributes, so instance access sees the descriptor objects directly.
reveal_type(dc2.x)  # revealed: Desc2[int]
reveal_type(dc2.y)  # revealed: Desc2[str]
```

With a class-body default, class access still resolves through `__get__`, and instance access
continues to be modeled as descriptor-backed access:

```py
from dataclasses import dataclass
from typing import Any, Generic, TypeVar, overload

T = TypeVar("T")

class Desc2(Generic[T]):
    @overload
    def __get__(self, instance: None, owner: Any) -> list[T]: ...
    @overload
    def __get__(self, instance: object, owner: Any) -> T: ...
    def __get__(self, instance: object | None, owner: Any) -> list[T] | T:
        raise NotImplementedError

@dataclass
class DC2Default:
    z: Desc2[str] = Desc2()

reveal_type(DC2Default.z)  # revealed: list[str]
reveal_type(DC2Default(Desc2()).z)  # revealed: str
```

Annotation-only data descriptors also do not create class attributes, but the synthesized
initializer still uses the descriptor's `__set__` value type for instance initialization:

```py
from dataclasses import dataclass
from typing import Any, overload

class Desc3:
    @overload
    def __get__(self, instance: None, owner: Any) -> "Desc3": ...
    @overload
    def __get__(self, instance: object, owner: Any) -> int: ...
    def __get__(self, instance: object | None, owner: Any) -> "Desc3 | int":
        raise NotImplementedError

    def __set__(self, instance: object, value: int) -> None:
        raise NotImplementedError

@dataclass
class DC3:
    x: Desc3

# error: [unresolved-attribute]
DC3.x

reveal_type(DC3(3).x)  # revealed: int
```

Descriptors with `init=False` are not shadowed by the synthesized initializer, so instance access
continues to resolve through the descriptor protocol:

```py
from dataclasses import dataclass, field
from typing import Any, Generic, TypeVar, overload

T = TypeVar("T")

class Desc4(Generic[T]):
    @overload
    def __get__(self, instance: None, owner: Any) -> list[T]: ...
    @overload
    def __get__(self, instance: object, owner: Any) -> T: ...
    def __get__(self, instance: object | None, owner: Any) -> list[T] | T:
        raise NotImplementedError

@dataclass
class DC4:
    x: Desc4[str] = field(default=Desc4(), init=False)

reveal_type(DC4.x)  # revealed: list[str]
reveal_type(DC4().x)  # revealed: str
```

## `default_factory`

The `default_factory` argument can be used to specify a callable that provides a default value for a
field:

```py
from dataclasses import dataclass, field
from datetime import datetime

@dataclass
class Data:
    content: list[int] = field(default_factory=list)
    timestamp: datetime = field(default_factory=datetime.now, init=False)

# revealed: (self: Data, content: list[int] = ...) -> None
reveal_type(Data.__init__)

data = Data([1, 2, 3])
reveal_type(data.content)  # revealed: list[int]
reveal_type(data.timestamp)  # revealed: datetime
```

## `kw_only`

```toml
[environment]
python-version = "3.12"
```

If `kw_only` is set to `True`, the field can only be set using keyword arguments:

```py
from dataclasses import dataclass, field

@dataclass
class Person:
    name: str
    age: int | None = field(default=None, kw_only=True)
    role: str = field(default="user", kw_only=True)

# revealed: (self: Person, name: str, *, age: int | None = None, role: str = "user") -> None
reveal_type(Person.__init__)

alice = Person(role="admin", name="Alice")

# error: [too-many-positional-arguments] "Too many positional arguments: expected 1, got 2"
bob = Person("Bob", 30)
```

## `KW_ONLY` sentinel

The `KW_ONLY` sentinel is a marker, not a real attribute. It should not appear as an instance or
class attribute.

```toml
[environment]
python-version = "3.12"
```

Although `_` is the conventional name, any name can be used for the sentinel. Accessing the sentinel
field on an instance or the class should not resolve to `KW_ONLY`:

```py
from dataclasses import dataclass, KW_ONLY

@dataclass
class DC:
    sentinel: KW_ONLY
    name: str

dc = DC(name="Alice")

# error: [unresolved-attribute]
dc.sentinel

# error: [unresolved-attribute]
DC.sentinel
```

When a child uses `_: KW_ONLY` and a parent defines `_` as a real field, the parent's `_` field is
inherited and the sentinel only affects subsequent fields in the child:

```py
from dataclasses import dataclass, KW_ONLY

@dataclass
class Parent:
    _: int

@dataclass
class Child(Parent):
    _: KW_ONLY
    name: str

# Parent's `_: int` field is inherited; the sentinel makes `name` keyword-only.
# revealed: (self: Child, _: int, *, name: str) -> None
reveal_type(Child.__init__)

c = Child(1, name="Alice")
reveal_type(c._)  # revealed: int
```

## dataclass_transform field_specifiers

If `field_specifiers` is not specified, it defaults to an empty tuple, meaning no field specifiers
are supported and `dataclasses.field` and `dataclasses.Field` should not be accepted by default.

```py
from typing_extensions import dataclass_transform
from dataclasses import field, dataclass
from typing import Any, TypeVar

T = TypeVar("T")

@dataclass_transform()
def create_model(*, init: bool = True):
    def deco(cls: type[T]) -> type[T]:
        return cls
    return deco

@create_model()
class A:
    name: str = field(init=False)

# Without explicit field_specifiers, field(init=False) is an ordinary default RHS.
reveal_type(A.__init__)  # revealed: (self: A, name: str = ...) -> None

class OtherFieldInfo:
    def __init__(self, default: Any = None, **kwargs: Any) -> None: ...

def other_field(default: Any = None, **kwargs: Any) -> OtherFieldInfo:
    return OtherFieldInfo(default=default, **kwargs)

@dataclass_transform(field_specifiers=(other_field, OtherFieldInfo))
def create_model_with_other_specifiers(*, init: bool = True):
    def deco(cls: type[T]) -> type[T]:
        return cls
    return deco

@create_model_with_other_specifiers()
class C:
    name: str = field(init=False)

# Even with other active field_specifiers, an unlisted RHS is an ordinary default value.
reveal_type(C.__init__)  # revealed: (self: C, name: str = ...) -> None

@dataclass
class B:
    name: str = field(init=False)

# Regular @dataclass should respect field(init=False)
reveal_type(B.__init__)  # revealed: (self: B) -> None
```

Test constructor calls:

```py
# These should NOT error because A's `field(...)` call is treated like any other default value
A()
A(name="foo")
C()
C(name="foo")

# This should error because field(init=False) is respected for B
# error: [unknown-argument]
B(name="foo")
```
