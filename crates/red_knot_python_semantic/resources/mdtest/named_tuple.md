# `NamedTuple`

`NamedTuple` is a type-safe way to define named tuples — a tuple where each field can be accessed by
name, and not just by its numeric position within the tuple:

## `typing.NamedTuple`

### Basics

```py
from typing import NamedTuple

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

# TODO: These should reveal the types of the fields
reveal_type(alice[0])  # revealed: Unknown
reveal_type(alice[1])  # revealed: Unknown
reveal_type(alice[2])  # revealed: Unknown

# error: [missing-argument]
Person(3)

# error: [too-many-positional-arguments]
Person(3, "Eve", 99, "extra")

# error: [invalid-argument-type]
Person(id="3", name="Eve")
```

Alternative functional syntax:

```py
Person2 = NamedTuple("Person", [("id", int), ("name", str)])
alice2 = Person2(1, "Alice")

# TODO: should be an error
Person2(1)

reveal_type(alice2.id)  # revealed: @Todo(GenericAlias instance)
reveal_type(alice2.name)  # revealed: @Todo(GenericAlias instance)
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

# TODO: this should be supported (no error, revealed type of `Property[float]`)
# error: [invalid-argument-type]
reveal_type(Property("height", 3.4))  # revealed: Property[Unknown]
```

## `collections.namedtuple`

```py
from collections import namedtuple

Person = namedtuple("Person", ["id", "name", "age"], defaults=[None])

alice = Person(1, "Alice", 42)
bob = Person(2, "Bob")
```
