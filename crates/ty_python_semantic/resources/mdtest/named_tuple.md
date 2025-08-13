# `NamedTuple`

`NamedTuple` is a type-safe way to define named tuples — a tuple where each field can be accessed by
name, and not just by its numeric position within the tuple:

## `typing.NamedTuple`

### Basics

```py
from typing import NamedTuple
from ty_extensions import static_assert, is_subtype_of, is_assignable_to

class Person(NamedTuple):
    id: int
    name: str
    age: int | None = None

alice = Person(1, "Alice", 42)
alice = Person(id=1, name="Alice", age=42)
bob = Person(2, "Bob")
bob = Person(id=2, name="Bob")

reveal_type(alice.id)  # revealed: int
reveal_type(alice.name)  # revealed: str
reveal_type(alice.age)  # revealed: int | None

# revealed: tuple[<class 'Person'>, <class 'tuple[int, str, int | None]'>, <class 'Sequence[int | str | None]'>, <class 'Reversible[int | str | None]'>, <class 'Collection[int | str | None]'>, <class 'Iterable[int | str | None]'>, <class 'Container[int | str | None]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(Person.__mro__)

static_assert(is_subtype_of(Person, tuple[int, str, int | None]))
static_assert(is_subtype_of(Person, tuple[object, ...]))
static_assert(not is_assignable_to(Person, tuple[int, str, int]))
static_assert(not is_assignable_to(Person, tuple[int, str]))

reveal_type(len(alice))  # revealed: Literal[3]
reveal_type(bool(alice))  # revealed: Literal[True]

reveal_type(alice[0])  # revealed: int
reveal_type(alice[1])  # revealed: str
reveal_type(alice[2])  # revealed: int | None

# error: [index-out-of-bounds] "Index 3 is out of bounds for tuple `Person` with length 3"
reveal_type(alice[3])  # revealed: Unknown

reveal_type(alice[-1])  # revealed: int | None
reveal_type(alice[-2])  # revealed: str
reveal_type(alice[-3])  # revealed: int

# error: [index-out-of-bounds] "Index -4 is out of bounds for tuple `Person` with length 3"
reveal_type(alice[-4])  # revealed: Unknown

reveal_type(alice[1:])  # revealed: tuple[str, int | None]
reveal_type(alice[::-1])  # revealed: tuple[int | None, str, int]

alice_id, alice_name, alice_age = alice
reveal_type(alice_id)  # revealed: int
reveal_type(alice_name)  # revealed: str
reveal_type(alice_age)  # revealed: int | None

# error: [invalid-assignment] "Not enough values to unpack: Expected 4"
a, b, c, d = alice
# error: [invalid-assignment] "Too many values to unpack: Expected 2"
a, b = alice
*_, age = alice
reveal_type(age)  # revealed: int | None

# error: [missing-argument]
Person(3)

# error: [too-many-positional-arguments]
Person(3, "Eve", 99, "extra")

# error: [invalid-argument-type]
Person(id="3", name="Eve")

# TODO: over-writing NamedTuple fields should be an error
alice.id = 42
bob.age = None
```

Alternative functional syntax:

```py
Person2 = NamedTuple("Person", [("id", int), ("name", str)])
alice2 = Person2(1, "Alice")

# TODO: should be an error
Person2(1)

reveal_type(alice2.id)  # revealed: @Todo(functional `NamedTuple` syntax)
reveal_type(alice2.name)  # revealed: @Todo(functional `NamedTuple` syntax)
```

### Definition

TODO: Fields without default values should come before fields with.

```py
from typing import NamedTuple

class Location(NamedTuple):
    altitude: float = 0.0
    latitude: float  # this should be an error
    longitude: float
```

### Multiple Inheritance

Multiple inheritance is not supported for `NamedTuple` classes:

```py
from typing import NamedTuple

# This should ideally emit a diagnostic
class C(NamedTuple, object):
    id: int
    name: str
```

### Inheriting from a `NamedTuple`

Inheriting from a `NamedTuple` is supported, but new fields on the subclass will not be part of the
synthesized `__new__` signature:

```py
from typing import NamedTuple

class User(NamedTuple):
    id: int
    name: str

class SuperUser(User):
    level: int

# This is fine:
alice = SuperUser(1, "Alice")
reveal_type(alice.level)  # revealed: int

# This is an error because `level` is not part of the signature:
# error: [too-many-positional-arguments]
alice = SuperUser(1, "Alice", 3)
```

TODO: If any fields added by the subclass conflict with those in the base class, that should be
flagged.

```py
from typing import NamedTuple

class User(NamedTuple):
    id: int
    name: str

class SuperUser(User):
    id: int  # this should be an error
```

### Generic named tuples

```toml
[environment]
python-version = "3.12"
```

```py
from typing import NamedTuple

class Property[T](NamedTuple):
    name: str
    value: T

reveal_type(Property("height", 3.4))  # revealed: Property[float]
```

## Attributes on `NamedTuple`

The following attributes are available on `NamedTuple` classes / instances:

```py
from typing import NamedTuple

class Person(NamedTuple):
    name: str
    age: int | None = None

reveal_type(Person._field_defaults)  # revealed: dict[str, Any]
reveal_type(Person._fields)  # revealed: tuple[str, ...]
reveal_type(Person._make)  # revealed: bound method <class 'Person'>._make(iterable: Iterable[Any]) -> Self@_make
reveal_type(Person._asdict)  # revealed: def _asdict(self) -> dict[str, Any]
reveal_type(Person._replace)  # revealed: def _replace(self, **kwargs: Any) -> Self@_replace

# TODO: should be `Person` once we support `Self`
reveal_type(Person._make(("Alice", 42)))  # revealed: Unknown

person = Person("Alice", 42)

reveal_type(person._asdict())  # revealed: dict[str, Any]
# TODO: should be `Person` once we support `Self`
reveal_type(person._replace(name="Bob"))  # revealed: Unknown
```

## `collections.namedtuple`

```py
from collections import namedtuple

Person = namedtuple("Person", ["id", "name", "age"], defaults=[None])

alice = Person(1, "Alice", 42)
bob = Person(2, "Bob")
```

## NamedTuple with custom `__getattr__`

This is a regression test for <https://github.com/astral-sh/ty/issues/322>. Make sure that the
`__getattr__` method does not interfere with the `NamedTuple` behavior.

```py
from typing import NamedTuple

class Vec2(NamedTuple):
    x: float = 0.0
    y: float = 0.0

    def __getattr__(self, attrs: str): ...

Vec2(0.0, 0.0)
```
