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

An inferred instance attribute is different: it is not guaranteed to exist until the method that
initializes it has run. Its absence must remain possible even though structural protocol
assignability recognizes the attribute.

```py
class WithInstanceSpam:
    def initialize(self) -> None:
        self.spam = 42

def _(obj: WithInstanceSpam):
    if hasattr(obj, "spam"):
        reveal_type(obj)  # revealed: WithInstanceSpam
        reveal_type(obj.spam)  # revealed: int
    else:
        reveal_type(obj)  # revealed: WithInstanceSpam
```

An instance assignment does not make the negative branch reachable if the attribute is already
guaranteed by a class-level value.

```py
class WithClassAndInstanceSpam:
    spam = 42

    def initialize(self) -> None:
        self.spam = 43

def _(obj: WithClassAndInstanceSpam):
    if not hasattr(obj, "spam"):
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

## Instance attribute initialized behind its own `hasattr` guard

An instance attribute can be discovered without first inferring the guarded assignment that
initializes it. The negative branch remains reachable, and its receiver keeps its original `Self`
type even when the value assigned to the attribute is a bound method.

```py
class Base:
    def __init__(self):
        if not hasattr(self, "x"):
            reveal_type(self)  # revealed: Self@__init__
            self.x = self.__str__

class Child(Base):
    x = Base.__str__
```

## Instance annotations do not guarantee attribute presence

Annotating an attribute in a method discovers its name but does not create the attribute at runtime,
so a negative `hasattr` guard must remain reachable.

```py
class C:
    def initialize(self) -> None:
        self.x: int

def f(value: C):
    if not hasattr(value, "x"):
        reveal_type(value)  # revealed: C
```

## Class-body annotations do not guarantee attribute presence

A class-body annotation does not create a class attribute. An instance assignment may eventually
provide the value, but its absence remains possible until that assignment runs.

```py
class C:
    x: int

    def initialize(self) -> None:
        self.x = 1

def f(value: C):
    if not hasattr(value, "x"):
        reveal_type(value)  # revealed: C
```

## Inherited class attributes remain definitely present

An instance assignment does not make an inherited class attribute optional: its class-level value is
already available before the instance assignment runs.

```py
class Base:
    x = 42

class Child(Base):
    def initialize(self) -> None:
        self.x = 43

def f(value: Child):
    if not hasattr(value, "x"):
        reveal_type(value)  # revealed: Never
```

## Inherited instance attributes remain potentially absent

An instance attribute assigned in a base-class method is still only potentially present on a
subclass instance: the initializing method might not have run yet.

```py
class Base:
    def initialize(self) -> None:
        self.x = 42

class Child(Base): ...

def f(value: Child):
    if not hasattr(value, "x"):
        reveal_type(value)  # revealed: Child
```

## Unrelated attributes retain negative narrowing

Discovering one instance attribute does not prevent `hasattr` from narrowing the receiver when
testing for a different, unknown attribute.

```py
class C:
    def initialize(self) -> None:
        self.x = 42

def f(value: C):
    if not hasattr(value, "other"):
        reveal_type(value)  # revealed: C & ~<Protocol with members 'other'>
```

## Static methods do not discover instance assignments

A static method's first parameter is unrelated to its containing class. Assigning an attribute to
that parameter does not disable negative narrowing, including when `staticmethod` is aliased.

```py
my_staticmethod = staticmethod

class C:
    @staticmethod
    def mutate(target):
        target.x = 1

    @my_staticmethod
    def mutate_alias(target):
        target.y = 1

def f(value: C):
    if not hasattr(value, "x"):
        reveal_type(value)  # revealed: C & ~<Protocol with members 'x'>

    if not hasattr(value, "y"):
        reveal_type(value)  # revealed: C & ~<Protocol with members 'y'>
```

## Reading an attribute does not discover an instance assignment

Merely referencing an attribute in a method does not make it an inferred instance attribute. A
negative `hasattr` guard for that name must therefore retain its usual protocol narrowing.

```py
class C:
    def read(self):
        return self.spam  # error: [unresolved-attribute]

    def check(self) -> None:
        if not hasattr(self, "spam"):
            reveal_type(self)  # revealed: Self@check & ~<Protocol with members 'spam'>
```
