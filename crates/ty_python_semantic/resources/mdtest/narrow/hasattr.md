# Narrowing using `hasattr()`

## Basic guards

The builtin function `hasattr()` can be used to narrow nominal and structural types. Positive checks
can add a synthesized protocol to the receiver type. Negative checks record the absence of the
member separately from its declared or inferred value type:

```py
from typing import final
from typing_extensions import LiteralString

class NonFinalClass: ...

def _(obj: NonFinalClass):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: NonFinalClass & <Protocol with members 'spam'>
        reveal_type(obj.spam)  # revealed: object
    else:
        reveal_type(obj)  # revealed: NonFinalClass

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
        reveal_type(obj)  # revealed: MaybeWithSpam

        # error: [unresolved-attribute]
        reveal_type(obj.spam)  # revealed: Unknown
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

## Union receivers

A negative check eliminates union members with an initialized class attribute, but retains members
whose attribute is only declared. The retained declaration still does not make the missing member
readable in that branch.

```py
class WithValue:
    value = 1

class DeclaredValue:
    value: int

def f(obj: WithValue | DeclaredValue):
    if not hasattr(obj, "value"):
        reveal_type(obj)  # revealed: DeclaredValue
        obj.value  # error: [unresolved-attribute]
```

## Guarded initialization

An assignment establishes the type of an instance attribute, but does not prove that it is present
before the assignment executes. Both constant and receiver-dependent initializers remain reachable.

```py
class Cached:
    def initialize(self):
        if not hasattr(self, "number"):
            self.number = 1
            self.missing  # error: [unresolved-attribute]
        if not hasattr(self, "method"):
            self.method = self.__str__
            self.other_missing  # error: [unresolved-attribute]

reveal_type(Cached().number)  # revealed: int
```

## Presence before and after assignment

The negative guard establishes absence. An assignment supersedes that fact, so subsequent reads use
the assigned value. This also applies to slots, which reserve storage without initializing it.

```py
class Cached:
    __slots__ = ("value",)
    value: int

    def initialize(self):
        if not hasattr(self, "value"):
            self.value  # error: [unresolved-attribute]
            self.value = 1
            reveal_type(self.value)  # revealed: Literal[1]
```

## Initialization on the missing branch

The attribute is present after the conditional: it either existed before the check or was assigned
in the missing branch. This avoids a possibly-missing-attribute error even when the class only
conditionally defines the member.

```py
def condition() -> bool:
    return True

class Cached:
    if condition():
        value = 0

    def get(self) -> int:
        if not hasattr(self, "value"):
            self.value = 1
        return self.value
```

## Receiver reassignment

Reassigning the receiver forgets the guard's presence information. The new object has the member's
ordinary declared type, rather than the absence established for the previous object.

```py
class Item:
    value: int

def f(item: Item, other: Item):
    if not hasattr(item, "value"):
        item.value  # error: [unresolved-attribute]
        item = other
        reveal_type(item.value)  # revealed: int
```

## Compound guards and aliases

Aliases of the builtin and boolean combinations retain the same presence information.

```py
from builtins import hasattr as has_attribute

class Cached:
    def initialize(self, enabled: bool):
        if enabled and not has_attribute(self, "value"):
            self.value = self.__str__
            self.missing  # error: [unresolved-attribute]
```

## Eager nested scopes

An eagerly evaluated comprehension retains the enclosing guard's presence information.

```py
class Item:
    value: int

def f(item: Item):
    if not hasattr(item, "value"):
        [item.value for _ in range(1)]  # error: [unresolved-attribute]
```

## Dynamic receivers

A dynamic type does not prove that a member exists. A successful guard leaves its value type
unrestricted, while a failed guard establishes absence.

```py
from typing import Any

def f(value: Any):
    if hasattr(value, "field"):
        reveal_type(value.field)  # revealed: Any
    else:
        value.field  # error: [unresolved-attribute]

def g(value: type[Any]):
    if not hasattr(value, "field"):
        value.field  # error: [unresolved-attribute]
```

## Properties

A property can raise `AttributeError`, so its definition alone does not prove that reading it
succeeds. A negative guard remains reachable even though the property belongs to the class.

```py
class WithProperty:
    @property
    def value(self) -> int:
        raise AttributeError

def f(obj: WithProperty):
    if not hasattr(obj, "value"):
        obj.missing  # error: [unresolved-attribute]
```

## Guarded bound-method initialization across modules

An initializer can install a default bound method when the subclass does not provide one. Checking
either module first preserves the valid assignment and the diagnostic for a missing attribute. This
reproduces <https://github.com/astral-sh/ty/issues/4076>.

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "method"):
            self.method = self.__str__
            self.missing  # error: [unresolved-attribute]
```

`child.py`:

```py
from base import Base

class Child(Base):
    method = Base.__str__
```

## Guarded bound-method initialization with the subclass checked first

Reversing the module order produces the same diagnostics.

`child.py`:

```py
from base import Base

class Child(Base):
    method = Base.__str__
```

`base.py`:

```py
class Base:
    def __init__(self):
        if not hasattr(self, "method"):
            self.method = self.__str__
            self.missing  # error: [unresolved-attribute]
```
