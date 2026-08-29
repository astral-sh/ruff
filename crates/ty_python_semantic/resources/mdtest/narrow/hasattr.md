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

The negative guard establishes that the attribute is absent. An assignment supersedes that fact; a
later presence check succeeds. Deleting the instance attribute makes it absent again.

```py
class Cached:
    def initialize(self):
        if not hasattr(self, "value"):
            self.value  # error: [unresolved-attribute]
            self.value = 1
            reveal_type(self.value)  # revealed: Literal[1]
            reveal_type(hasattr(self, "value"))  # revealed: Literal[True]
            del self.value
            reveal_type(hasattr(self, "value"))  # revealed: Literal[False]
            self.value  # error: [unresolved-attribute]
```

## Initialization on the missing branch

The attribute is present after the conditional: it either existed before the check or was assigned
in the missing branch. The inferred value type remains available independently of presence.

```py
class Cached:
    def get(self) -> int:
        if not hasattr(self, "value"):
            self.value = 1
        reveal_type(hasattr(self, "value"))  # revealed: Literal[True]
        return self.value
```

## Deletion with a class fallback

Deleting an instance attribute exposes the class attribute of the same name, so the member remains
readable after deletion.

```py
class Cached:
    value = 1

    def reset(self):
        self.value = 2
        del self.value
        reveal_type(hasattr(self, "value"))  # revealed: Literal[True]
        reveal_type(self.value)  # revealed: int
```

## Conditional initialization

An assignment on only one branch does not establish presence after the branches merge. A guard can
still distinguish the two cases.

```py
class Cached:
    def initialize(self, enabled: bool):
        if enabled:
            self.value = 1
        reveal_type(hasattr(self, "value"))  # revealed: bool
        if not hasattr(self, "value"):
            self.value  # error: [unresolved-attribute]
            self.value = 2
        reveal_type(hasattr(self, "value"))  # revealed: Literal[True]
```

## Receiver reassignment

Presence belongs to the receiver at the time of the guard. Assigning another object to that name
invalidates both positive and negative facts about its members.

```py
class Item:
    value: int

def f(item: Item, other: Item):
    if hasattr(item, "value"):
        reveal_type(hasattr(item, "value"))  # revealed: Literal[True]
        item = other
        reveal_type(hasattr(item, "value"))  # revealed: bool
        reveal_type(item.value)  # revealed: int
    else:
        reveal_type(hasattr(item, "value"))  # revealed: Literal[False]
        item = other
        reveal_type(hasattr(item, "value"))  # revealed: bool
        reveal_type(item.value)  # revealed: int
```

## Compound guards and aliases

Aliases of the builtin and boolean combinations retain the same presence information. A
contradictory nested guard is unreachable, but an unrelated condition does not establish presence.

```py
from builtins import hasattr as has_attribute

class Cached:
    def initialize(self, enabled: bool):
        if enabled and not has_attribute(self, "value"):
            self.value = self.__str__
            self.missing  # error: [unresolved-attribute]
        if has_attribute(self, "value"):
            if not has_attribute(self, "value"):
                self.missing
        if enabled or has_attribute(self, "value"):
            reveal_type(has_attribute(self, "value"))  # revealed: bool
```

## Dynamic receivers

A dynamic type does not prove that a member exists. A successful guard establishes its presence
without restricting its value type, and a failed guard establishes absence.

```py
from typing import Any

def f(value: Any):
    reveal_type(hasattr(value, "field"))  # revealed: bool
    if hasattr(value, "field"):
        reveal_type(hasattr(value, "field"))  # revealed: Literal[True]
        reveal_type(value.field)  # revealed: Any
    else:
        reveal_type(hasattr(value, "field"))  # revealed: Literal[False]
        value.field  # error: [unresolved-attribute]

def g(value: type[Any]):
    if not hasattr(value, "field"):
        value.field  # error: [unresolved-attribute]
```

## Slots

A slot reserves storage without initializing its value. Assigning and deleting the slot changes its
presence just as it does for an attribute stored in an instance dictionary.

```py
class Cached:
    __slots__ = ("value",)
    value: int

    def initialize(self):
        if not hasattr(self, "value"):
            self.value = 1
        reveal_type(hasattr(self, "value"))  # revealed: Literal[True]
        del self.value
        reveal_type(hasattr(self, "value"))  # revealed: Literal[False]
        self.value  # error: [unresolved-attribute]
```

## Descriptors and dynamic attribute lookup

A property can raise `AttributeError`, so its definition alone does not prove that reading it
succeeds. Likewise, deleting an instance attribute does not establish absence when `__getattr__` can
provide a replacement.

```py
class WithProperty:
    @property
    def value(self) -> int:
        raise AttributeError

    @value.setter
    def value(self, value: int) -> None:
        pass

def f(obj: WithProperty):
    reveal_type(hasattr(obj, "value"))  # revealed: bool
    if not hasattr(obj, "value"):
        obj.missing  # error: [unresolved-attribute]
    obj.value = 1
    reveal_type(hasattr(obj, "value"))  # revealed: bool

class Dynamic:
    def __getattr__(self, name: str) -> int:
        return 1

    def reset(self):
        self.value = 2
        del self.value
        reveal_type(hasattr(self, "value"))  # revealed: bool
        reveal_type(self.value)  # revealed: int
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
