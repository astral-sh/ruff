# Narrowing using `hasattr()`

## Basic checks

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

## Presence at branch joins

An attribute is available after a guard when it either passed the check or was assigned in the
missing branch. Assigning an undeclared attribute still reports an error, but the subsequent read
does not repeat it.

```py
class Item: ...

def initialized(item: Item):
    if not hasattr(item, "value"):
        item.value = 1  # error: [invalid-assignment]
    reveal_type(item.value)  # revealed: object
```

A successful guard on only one branch does not establish presence after the conditional. Aliases of
`hasattr` provide the same presence information as the builtin.

```py
from builtins import hasattr as has_attribute

def partially_checked(item: Item, condition: bool):
    if condition:
        assert has_attribute(item, "value")
        reveal_type(item.value)  # revealed: object
    item.value  # error: [unresolved-attribute]
```

## Receiver reassignment

Reassigning the receiver forgets the successful guard's presence information. The new object does
not inherit the member established for the previous object.

```py
class Item: ...

def f(item: Item, other: Item):
    if hasattr(item, "value"):
        reveal_type(item.value)  # revealed: object
        item = other
        item.value  # error: [unresolved-attribute]
```

## Negative checks and later narrowing

A failed check retains its negative protocol constraint. Later checks can use that constraint to
rule out a receiver with the attribute, regardless of the order of the checks.

```py
class WithValue:
    value = 1

def f(obj: object):
    if not hasattr(obj, "value") and isinstance(obj, WithValue):
        reveal_type(obj)  # revealed: Never

    if isinstance(obj, WithValue) and not hasattr(obj, "value"):
        reveal_type(obj)  # revealed: Never
```

The constraint also excludes intersections of a remaining union member with a type that has the
attribute.

```py
class Other: ...

def g(obj: WithValue | Other):
    if not hasattr(obj, "value"):
        reveal_type(obj)  # revealed: Other & ~<Protocol with members 'value'>
        if isinstance(obj, WithValue):
            reveal_type(obj)  # revealed: Never
```
