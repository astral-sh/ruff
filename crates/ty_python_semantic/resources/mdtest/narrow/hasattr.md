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

Checking the base class before its subclass must succeed when one guarded assignment initializes an
inferred attribute and another assigns a bound method to an existing method attribute.

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
            self.callback = self.callback_fallback

    def callback_fallback(self, value): ...
    def callback(self, value): ...
```

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
    callback = Base.callback_fallback

reveal_type(Base().x())  # revealed: str
```

## Guarded instance assignment across modules, child checked first

Checking the subclass first must produce the same result as checking its base class first.

`child.py`:

```py
from base import Base

class Child(Base):
    x = Base.__str__
    callback = Base.callback_fallback

reveal_type(Base().x())  # revealed: str
```

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            self.x = self.__str__
            self.callback = self.callback_fallback

    def callback_fallback(self, value): ...
    def callback(self, value): ...
```

## Guarded initializers preserve inferred instance attributes

Reading another attribute from the receiver while initializing a guarded attribute must not cause
the inferred attribute to disappear.

```py
class Cached:
    source = 1

    def metadata(self) -> int:
        if not hasattr(self, "value"):
            self.value = self.source
        return self.value

reveal_type(Cached().metadata())  # revealed: int
```

## Guarded initializers preserve unrelated receiver narrowing

An `isinstance` guard can provide the subclass-only attribute used to initialize a different,
`hasattr`-guarded instance attribute.

```py
class Base:
    def initialize(self) -> None:
        if isinstance(self, Child):
            if not hasattr(self, "x"):
                self.x = self.child_value

class Child(Base):
    child_value = 1

reveal_type(Child().x)  # revealed: int
```

## Structural protocol narrowing remains transitive

A class accepted by a structural protocol must behave consistently with that protocol when both are
narrowed using the same attribute.

```py
from typing import Protocol

class HasX(Protocol):
    @property
    def x(self) -> object: ...

class C:
    def initialize(self) -> None:
        self.x = 1

def check_protocol(value: HasX) -> None:
    if not hasattr(value, "x"):
        reveal_type(value)  # revealed: Never

def check_class(value: C) -> None:
    check_protocol(value)

    if not hasattr(value, "x"):
        reveal_type(value)  # revealed: Never
```

## Assigned attributes remain present

After an instance attribute is assigned, its absence cannot make a subsequent branch reachable.

```py
class C:
    x: int

    def initialize(self) -> None:
        self.x = 1

        if not hasattr(self, "x"):
            reveal_type(self)  # revealed: Never
            self.x = "unreachable"
            self.x = self.missing

reveal_type(C().x)  # revealed: int
```

## Class-backed attributes remain present

A class attribute makes a negative `hasattr` branch unreachable. An assignment in that branch must
not introduce diagnostics or widen the attribute's public type.

```py
class C:
    x = 1

    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.missing

reveal_type(C().x)  # revealed: int
```

## Inherited class-backed attributes remain present

An inherited class attribute has the same presence guarantee as an attribute defined directly on the
class.

```py
class Base:
    x = 1

class Child(Base):
    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.missing

reveal_type(Child().x)  # revealed: int
```

## Contradictory nested guards do not define attributes

An attribute assigned only under contradictory positive and negative `hasattr` guards does not exist
on instances of the class.

```py
class C:
    def initialize(self) -> None:
        if hasattr(self, "x"):
            if not hasattr(self, "x"):
                self.x = self.missing

# error: [unresolved-attribute]
reveal_type(C().x)  # revealed: Unknown
```

## Reversed contradictory nested guards do not define attributes

Swapping the positive and negative guards does not make their conjunction reachable.

```py
class C:
    def initialize(self) -> None:
        if not hasattr(self, "x"):
            if hasattr(self, "x"):
                self.x = self.missing

# error: [unresolved-attribute]
reveal_type(C().x)  # revealed: Unknown
```

## Static methods preserve their receiver's attribute presence

A static method's first parameter is not an instance of its containing class. A class-backed
attribute on that parameter still makes a negative `hasattr` branch unreachable.

```py
class Target:
    x = 1

class C:
    @staticmethod
    def initialize(target: Target) -> None:
        if not hasattr(target, "x"):
            target.x = target.missing

reveal_type(Target().x)  # revealed: int
```

## Statically conditional class attributes remain present

A statically true version check guarantees that a class attribute exists, both on the defining class
and on subclasses that inherit it.

```py
import sys

class Base:
    if sys.version_info >= (3, 0):
        x = 1

    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.missing

class Child(Base):
    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.missing

reveal_type(Base().x)  # revealed: int
reveal_type(Child().x)  # revealed: int
```

## Possibly absent class attributes retain initializer diagnostics

A statically false or dynamically uncertain class binding does not make an instance initializer
unreachable, so missing attributes referenced by that initializer must still be reported.

```py
class StaticallyAbsent:
    if False:
        x = 1

    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.missing  # error: [unresolved-attribute]

# error: [possibly-missing-attribute]
reveal_type(StaticallyAbsent().x)  # revealed: Unknown

def condition() -> bool:
    return False

class PossiblyPresent:
    if condition():
        x = 1

    def initialize(self) -> None:
        if not hasattr(self, "x"):
            self.x = self.missing  # error: [unresolved-attribute]

# error: [possibly-missing-attribute]
reveal_type(PossiblyPresent().x)  # revealed: int | Unknown
```
