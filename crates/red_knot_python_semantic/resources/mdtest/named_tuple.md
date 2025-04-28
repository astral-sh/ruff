# `NamedTuple`

`NamedTuple` is a type-safe way to define named tuples — a tuple where each field can be accessed by
name, and not just by its numeric position within the tuple:

## `typing.NamedTuple`

```py
from typing import NamedTuple

class Person(NamedTuple):
    id: int
    name: str
    age: int | None = None

alice = Person(1, "Alice", 42)  # error: [no-matching-overload]
alice = Person(id=1, name="Alice", age=42)  # error: [no-matching-overload]
bob = Person(2, "Bob")  # error: [no-matching-overload]
bob = Person(id=2, name="Bob")  # error: [no-matching-overload]

reveal_type(alice.id)  # revealed: @Todo(GenericAlias instance)
reveal_type(alice.name)  # revealed: @Todo(GenericAlias instance)
reveal_type(alice.age)  # revealed: int | None | @Todo(instance attribute on class with dynamic base)
```

## `collections.namedtuple`

```py
from collections import namedtuple

Person = namedtuple("Person", ["id", "name", "age"], defaults=[None])

alice = Person(1, "Alice", 42)
bob = Person(2, "Bob")
```
