# Cycles in implicit instance attributes

## Unpacking

See: <https://github.com/astral-sh/ty/issues/364>

```py
class Point:
    def __init__(self, x: int = 0, y: int = 0) -> None:
        self.x = x
        self.y = y

    def replace_with(self, other: "Point") -> None:
        self.x, self.y = other.x, other.y

p = Point()
reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int
```

## Self-referential implicit attributes

```py
class Cyclic:
    def __init__(self, data: str | dict):  # error: [missing-type-argument]
        self.data = data

    def update(self):
        if isinstance(self.data, str):
            self.data = {"url": self.data}

# revealed: str | dict[Unknown, Unknown] | dict[str, str]
reveal_type(Cyclic("").data)
```

## Cycle normalization preserves non-gradual variadic parameters

Normalizing a recursive implicit-attribute type does not reinterpret specialized variadic parameters
as gradual:

```py
from typing import Any, Callable, Generic, TypeVar
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_subtype_of

T = TypeVar("T")
flag: bool

class C(Generic[T]):
    def method(self, *args: T, **kwargs: T) -> None: ...

c = C[Any]()

class Recursive:
    def __init__(self, other: "Recursive"):
        self.callback = c.method if flag else other.callback

def check(value: Recursive):
    reveal_type(value.callback)  # revealed: bound method C[Any].method(*args: Any, **kwargs: Any) -> None
    static_assert(is_subtype_of(TypeOf[value.callback], Callable[[], None]))
```

## Guarded instance attributes when the base is checked first

A guarded bound-method initializer remains valid, including when its receiver is explicitly
annotated, while another initializer still reports an attribute that is missing from the base class.
Calling the initialized method returns `str`. This reproduces
<https://github.com/astral-sh/ty/issues/4076>.

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
        if not hasattr(self, "z"):
            self.z = self.y  # error: [unresolved-attribute]

reveal_type(Base().x())  # revealed: str

class Annotated:
    def __init__(self: "Annotated"):
        if not hasattr(self, "value"):
            self.value = self.__str__
            self.missing  # error: [unresolved-attribute]
```

`child.py`:

```py
from base import Annotated, Base

class Child(Base):
    x = Base.__str__

    def z(self): ...
    def y(self): ...

class AnnotatedChild(Annotated):
    value = Annotated.__str__
```

## Guarded instance attributes when the subclass is checked first

Checking the subclass first preserves the valid initializer, its inferred return type, and the
missing-attribute diagnostic.

`child.py`:

```py
from base import Annotated, Base

class Child(Base):
    x = Base.__str__

    def z(self): ...
    def y(self): ...

class AnnotatedChild(Annotated):
    value = Annotated.__str__
```

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
        if not hasattr(self, "z"):
            self.z = self.y  # error: [unresolved-attribute]

reveal_type(Base().x())  # revealed: str

class Annotated:
    def __init__(self: "Annotated"):
        if not hasattr(self, "value"):
            self.value = self.__str__
            self.missing  # error: [unresolved-attribute]
```

## Named protocol guards when the base is checked first

Here, a runtime-checkable protocol with a read-only `x` property checks the same member presence as
`hasattr(self, "x")`. Whether the protocol is named does not change the initializer's reachability.
The initialized instance also satisfies the protocol outside the initializer.

`base.py`:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasX(Protocol):
    @property
    def x(self) -> object: ...

class Base:
    def __init__(self):
        if not isinstance(self, HasX):
            self.x = self.__str__
            self.missing  # error: [unresolved-attribute]

def accepts_x(value: HasX) -> None: ...

accepts_x(Base())
reveal_type(Base().x())  # revealed: str
```

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
```

## Named protocol guards when the subclass is checked first

Checking the subclass first preserves the reachable initializer and its missing-attribute error.

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
```

`base.py`:

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasX(Protocol):
    @property
    def x(self) -> object: ...

class Base:
    def __init__(self):
        if not isinstance(self, HasX):
            self.x = self.__str__
            self.missing  # error: [unresolved-attribute]

def accepts_x(value: HasX) -> None: ...

accepts_x(Base())
reveal_type(Base().x())  # revealed: str
```

## Nested and compound attribute guards when the base is checked first

An unrelated condition can appear outside an attribute guard, inside it, or on either side of a
compound condition without making a guarded initializer invalid. Previous narrowing of the receiver
must also preserve real diagnostics in the guarded branch.

`base.py`:

```py
class Marker: ...

class Base:
    def __init__(self, enabled: bool):
        if enabled:
            if not hasattr(self, "outer"):
                self.outer = self.__str__
        if not hasattr(self, "inner"):
            if enabled:
                self.inner = self.__str__
        if enabled and not hasattr(self, "leading"):
            self.leading = self.__str__
        if not hasattr(self, "trailing") and enabled:
            self.trailing = self.__str__
        if self is not None:
            if not hasattr(self, "nonnull"):
                self.nonnull = self.__str__
                self.nonnull_missing  # error: [unresolved-attribute]
        if not hasattr(self, "other"):
            if not hasattr(self, "unrelated"):
                self.unrelated = self.__str__
                self.unrelated_missing  # error: [unresolved-attribute]
        if isinstance(self, Marker):
            if not hasattr(self, "narrowed"):
                self.narrowed = self.__str__
                self.narrowed_missing  # error: [unresolved-attribute]
```

`child.py`:

```py
from base import Base, Marker

class Child(Base, Marker):
    outer = Base.__str__
    inner = Base.__str__
    leading = Base.__str__
    trailing = Base.__str__
    nonnull = Base.__str__
    unrelated = Base.__str__
    narrowed = Base.__str__
```

## Nested and compound attribute guards when the subclass is checked first

Checking the subclass first must preserve the same nested and compound guarded initializers.

`child.py`:

```py
from base import Base, Marker

class Child(Base, Marker):
    outer = Base.__str__
    inner = Base.__str__
    leading = Base.__str__
    trailing = Base.__str__
    nonnull = Base.__str__
    unrelated = Base.__str__
    narrowed = Base.__str__
```

`base.py`:

```py
class Marker: ...

class Base:
    def __init__(self, enabled: bool):
        if enabled:
            if not hasattr(self, "outer"):
                self.outer = self.__str__
        if not hasattr(self, "inner"):
            if enabled:
                self.inner = self.__str__
        if enabled and not hasattr(self, "leading"):
            self.leading = self.__str__
        if not hasattr(self, "trailing") and enabled:
            self.trailing = self.__str__
        if self is not None:
            if not hasattr(self, "nonnull"):
                self.nonnull = self.__str__
                self.nonnull_missing  # error: [unresolved-attribute]
        if not hasattr(self, "other"):
            if not hasattr(self, "unrelated"):
                self.unrelated = self.__str__
                self.unrelated_missing  # error: [unresolved-attribute]
        if isinstance(self, Marker):
            if not hasattr(self, "narrowed"):
                self.narrowed = self.__str__
                self.narrowed_missing  # error: [unresolved-attribute]
```

## Class attributes independently establish presence

A class attribute is present before an initializer runs, including when it is inherited. Assigning
to the same name inside a negative guard does not make that branch reachable. This applies to both
`hasattr` and named protocols with a read-only `object` property.

```py
from typing import Protocol, runtime_checkable

@runtime_checkable
class HasX(Protocol):
    @property
    def x(self) -> object: ...

class Base:
    x = 1

    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
            self.missing
        if not isinstance(self, HasX):
            self.x = self.__str__
            self.missing

class Child(Base):
    def initialize(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
            self.missing
```

## Class attributes establish presence through aliased protocol members

A read-only protocol property typed as an alias of `object` imposes the same presence requirement as
`object` itself. The class attribute makes the negative guard unreachable, even when that branch
assigns to the same attribute.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Protocol, runtime_checkable

type Top = object

@runtime_checkable
class HasX(Protocol):
    @property
    def x(self) -> Top: ...

class C:
    x = 1

    def __init__(self):
        if not isinstance(self, HasX):
            self.x = self.__str__
            self.missing
```

## Unreachable and deleted class attributes do not prevent guarded initialization

A class attribute that was never assigned or was deleted cannot make a later instance initializer
unreachable.

`base.py`:

```py
class Base:
    if False:
        unreachable = 1

    deleted = 1
    del deleted

    def __init__(self):
        if not hasattr(self, "unreachable"):
            self.unreachable = self.__str__
        if not hasattr(self, "deleted"):
            self.deleted = self.__str__
```

`child.py`:

```py
from base import Base

class Child(Base):
    unreachable = Base.__str__
    deleted = Base.__str__
```

## Guarded instance attributes after a call when the base is checked first

A call before a guarded initializer must not make its validity or later diagnostics depend on file
order.

`base.py`:

```py
def prepare() -> None: ...

class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            prepare()
            self.x = self.__str__
            self.missing  # error: [unresolved-attribute]
```

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
```

## Guarded instance attributes after a call when the subclass is checked first

Checking the subclass first must preserve the same guarded assignment and genuine missing-attribute
diagnostic after the intervening call.

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
```

`base.py`:

```py
def prepare() -> None: ...

class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            prepare()
            self.x = self.__str__
            self.missing  # error: [unresolved-attribute]
```

## Non-returning initializers do not define instance attributes

An assignment whose initializer never returns cannot make its target attribute present.

```py
from typing import NoReturn

def fail() -> NoReturn:
    raise RuntimeError

class C:
    def initialize(self):
        if not hasattr(self, "x"):
            self.x = fail()  # error: [invalid-assignment]

C().x  # error: [unresolved-attribute]
```

## Assignments in the opposite guard branch do not initialize an attribute

Assigning an existing attribute when `hasattr` succeeds does not initialize it in the opposite
branch. That branch remains unreachable and cannot create another instance attribute.

```py
class C:
    def __init__(self):
        self.x = 1

    def update(self):
        if hasattr(self, "x"):
            self.x = 2
        else:
            self.y = self.missing

C().y  # error: [unresolved-attribute]
```

## Contradictory attribute guards do not initialize an attribute

An impossible inner `hasattr` branch cannot create an instance attribute.

```py
class C:
    def initialize(self):
        if hasattr(self, "x"):
            if not hasattr(self, "x"):
                self.x = self.missing

C().x  # error: [unresolved-attribute]
```

## Lazy cached property behind `hasattr`

This pattern used to panic with "too many cycle iterations".

```py
class Cached:
    def get(self) -> int:
        return 0

    @property
    def metadata(self) -> int:
        if not hasattr(self, "_metadata"):
            self._metadata = self.get()
        return self._metadata

reveal_type(Cached().metadata)  # revealed: int
```
