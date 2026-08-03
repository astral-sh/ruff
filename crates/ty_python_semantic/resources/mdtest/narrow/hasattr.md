# Narrowing using `hasattr()`

## Nominal and structural types

The builtin function `hasattr()` can be used to narrow nominal and structural types. This is
accomplished using an intersection with a synthesized protocol:

```py
from typing import final
from typing_extensions import LiteralString

class NonFinalClass: ...

def _(obj: NonFinalClass):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: NonFinalClass & <Protocol with members 'spam'>
        reveal_type(obj.spam)  # revealed: object
    else:
        reveal_type(obj)  # revealed: NonFinalClass & ~<Protocol with members 'spam'>

        # error: [unresolved-attribute]
        reveal_type(obj.spam)  # revealed: Unknown

    if hasattr(obj, "not-an-identifier"):
        reveal_type(obj)  # revealed: NonFinalClass
    else:
        reveal_type(obj)  # revealed: NonFinalClass
```

For a final class, we recognize that there is no way that an object of `FinalClass` could ever have
a `spam` attribute, so the type is narrowed to `Never`:

```py
@final
class FinalClass: ...

def _(obj: FinalClass):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: Never
        reveal_type(obj.spam)  # revealed: Never
    else:
        reveal_type(obj)  # revealed: FinalClass

        # error: [unresolved-attribute]
        reveal_type(obj.spam)  # revealed: Unknown
```

When the corresponding attribute is already defined on the class, `hasattr` narrowing does not
change the type. `<Protocol with members 'spam'>` is a supertype of `WithSpam`, and so
`WithSpam & <Protocol …>` simplifies to `WithSpam`:

```py
class WithSpam:
    spam: int = 42

def _(obj: WithSpam):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: WithSpam
        reveal_type(obj.spam)  # revealed: int
    else:
        reveal_type(obj)  # revealed: Never
```

When a class may or may not have a `spam` attribute, `hasattr` narrowing can provide evidence that
the attribute exists. Here, no `possibly-missing-attribute` error is emitted in the `if` branch:

```py
def returns_bool() -> bool:
    return False

class MaybeWithSpam:
    if returns_bool():
        spam: int = 42

def _(obj: MaybeWithSpam):
    # error: [possibly-missing-attribute]
    reveal_type(obj.spam)  # revealed: int

    if hasattr(obj, "spam"):
        reveal_type(obj)  #  revealed: MaybeWithSpam & <Protocol with members 'spam'>
        reveal_type(obj.spam)  # revealed: int
    else:
        reveal_type(obj)  # revealed: MaybeWithSpam & ~<Protocol with members 'spam'>

        # TODO: Ideally, we would emit `[unresolved-attribute]` and reveal `Unknown` here:
        # error: [possibly-missing-attribute]
        reveal_type(obj.spam)  # revealed: int
```

All attribute available on `object` are still available on these synthesized protocols, but
attributes that are not present on `object` are not available:

```py
def f(x: object):
    if hasattr(x, "__qualname__"):
        reveal_type(x.__repr__)  # revealed: bound method object.__repr__() -> str
        reveal_type(x.__str__)  # revealed: bound method object.__str__() -> str
        reveal_type(x.__dict__)  # revealed: dict[str, Any]

        # error: [unresolved-attribute] "Object of type `<Protocol with members '__qualname__'>` has no attribute `foo`"
        reveal_type(x.foo)  # revealed: Unknown
```

## Guarded instance assignment across modules, base checked first

Checking the base class first preserves the negative `hasattr` fact. Its inferred method assignment
is valid; the existing callback assignment remains a consistent, unrelated false positive.

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
            self.callback = self.callback_fallback  # error: [invalid-assignment]

    def callback_fallback(self, value): ...
    def callback(self, value): ...
```

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
    callback = Base.callback_fallback
```

## Guarded instance assignment across modules, child checked first

Checking the subclass first must produce exactly the same diagnostic as checking its base first.

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
    callback = Base.callback_fallback
```

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
            self.callback = self.callback_fallback  # error: [invalid-assignment]

    def callback_fallback(self, value): ...
    def callback(self, value): ...
```

## Negative guards preserve their receiver constraint

Assigning a bound method to an inferred attribute does not discard the fact that the attribute is
absent before the assignment.

```py
class C:
    def initialize(self) -> None:
        if not hasattr(self, "x"):
            reveal_type(self)  # revealed: Self@initialize & ~<Protocol with members 'x'>
            self.x = self.__str__
```

## Negative guards preserve explicit method receiver contracts

A method requiring an existing `x` attribute cannot be called to initialize that same attribute
while a `hasattr` guard proves it absent.

```py
from typing import Protocol

class HasX(Protocol):
    x: int

class C:
    def needs_x(self: HasX) -> int:
        return self.x

    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.needs_x()  # error: [invalid-argument-type]

value = C()
value.initialize()

if hasattr(value, "x"):
    result: int = value.x
```

## Chained guarded assignments retain every target

A single guarded initializer can initialize two attributes without discarding the shared right-hand
side or inventing a missing-attribute diagnostic.

```py
class C:
    source = 1

    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.y = self.source

value = C()

if hasattr(value, "x"):
    reveal_type(value.x)  # revealed: int

reveal_type(value.y)  # revealed: int
```

## Guarded instance attributes preserve structural transitivity

An inferred attribute remains compatible with an ordinary protocol and the synthesized `hasattr`
protocol after its guarded initializer has been evaluated.

```py
from typing import Protocol

class HasX(Protocol):
    @property
    def x(self) -> object: ...

class C:
    source = 1

    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.source

def accepts(value: HasX) -> None: ...
def check(value: C) -> None:
    accepts(value)

    if not hasattr(value, "x"):
        reveal_type(value)  # revealed: Never
```
