# `TypedDict`

## Basic

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

alice: Person = {"name": "Alice", "age": 30}

# TODO: this should be `str`
reveal_type(alice["name"])  # revealed: Unknown
# TODO: this should be `int | None`
reveal_type(alice["age"])  # revealed: Unknown

reveal_type(Person.__required_keys__)  # revealed: @Todo(Support for `TypedDict`)

# TODO: this should be an error
bob: Person = {"name": b"Bob"}
```

## Structural subtyping

Subtyping between `TypedDict` types is structural, that is, it is based on the presence of keys and
their types, rather than the class hierarchy.

```py
from typing import TypedDict

class Person(TypedDict):
    name: str

class Employee(TypedDict):
    name: str
    employee_id: int

def accepts_person(p: Person) -> None:
    pass

def f(e: Employee) -> None:
    accepts_person(e)  # This is fine, as `Employee` has all keys of `Person`
```

## Function/assignment syntax

This is not yet supported. Make sure that we do not emit false positives for this syntax:

```py
from typing import TypedDict, Required

# Alternative syntax
Message = TypedDict("Message", {"id": Required[int], "content": str}, total=False)

msg = Message(id=1, content="Hello")

# No errors for yet-unsupported features (`closed`):
OtherMessage = TypedDict("OtherMessage", {"id": int, "content": str}, closed=True)

reveal_type(Message.__required_keys__)  # revealed: @Todo(Support for `TypedDict`)
```
