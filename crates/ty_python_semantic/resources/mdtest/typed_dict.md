# `TypedDict`

A [`TypedDict`] type represents dictionary objects with a specific set of string keys, and with
specific value types for each valid key. Each string key can be either required or non-required.

## Basic

Here, we define a `TypedDict` using the class-based syntax:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None
```

New instances can be created from dict literals. When accessing properties, the correct types should
be inferred based on the `TypedDict` definition:

```py
alice: Person = {"name": "Alice", "age": 30}

# TODO: this should be `str`
reveal_type(alice["name"])  # revealed: Unknown
# TODO: this should be `int | None`
reveal_type(alice["age"])  # revealed: Unknown
```

Instances can also be created through a constructor call:

```py
bob = Person(name="Bob", age=25)
```

Methods that are available on `dict` are also available on `TypedDict` instances:

```py
bob.update(age=26)
```

Construction of instances is checked for type correctness:

```py
# TODO: these should be errors (invalid argument type)
eve1a: Person = {"name": b"Eve", "age": None}
eve1b = Person(name=b"Eve", age=None)

# TODO: these should be errors (missing required key)
eve2a: Person = {"age": 22}
eve2b = Person(age=22)

# TODO: these should be errors (additional key)
eve3a: Person = {"name": "Eve", "age": 25, "extra": True}
eve3b = Person(name="Eve", age=25, extra=True)
```

Assignments to keys are also validated:

```py
# TODO: this should be an error
alice["name"] = None
```

Assignments to non-existing keys are disallowed:

```py
# TODO: this should be an error
alice["extra"] = True
```

## Structural assignability

Assignability between `TypedDict` types is structural, that is, it is based on the presence of keys
and their types, rather than the class hierarchy:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str

class Employee(TypedDict):
    name: str
    employee_id: int

p1: Person = Employee(name="Alice", employee_id=1)

# TODO: this should be an error
e1: Employee = Person(name="Eve")
```

All typed dictionaries can be assigned to `Mapping[str, object]`:

```py
from typing import Mapping, TypedDict

class Person(TypedDict):
    name: str
    age: int | None

m: Mapping[str, object] = Person(name="Alice", age=30)
```

## Types of keys and values

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

def _(p: Person) -> None:
    reveal_type(p.keys())  # revealed: @Todo(Support for `TypedDict`)
    reveal_type(p.values())  # revealed: @Todo(Support for `TypedDict`)
```

## Unlike normal classes

`TypedDict` types are not like normal classes. The "attributes" can not be accessed. Neither on the
type, nor on instances:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

# TODO: this should be an error
Person.name

# TODO: this should be an error
Person(name="Alice", age=30).name
```

## Special properties

`TypedDict` instances have some special properties that can be used for introspection:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

reveal_type(Person.__total__)  # revealed: @Todo(Support for `TypedDict`)
reveal_type(Person.__required_keys__)  # revealed: @Todo(Support for `TypedDict`)
reveal_type(Person.__optional_keys__)  # revealed: @Todo(Support for `TypedDict`)
```

## Subclassing

`TypedDict` types can be subclassed. The subclass can add new keys:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str

class Employee(Person):
    employee_id: int

alice: Employee = {"name": "Alice", "employee_id": 1}

# TODO: this should be an error (missing required key)
eve: Employee = {"name": "Eve"}
```

## Generic `TypedDict`

`TypedDict`s can be generic:

```py
from typing import Generic, TypeVar, TypedDict

T = TypeVar("T")

class TaggedData(TypedDict, Generic[T]):
    data: T
    tag: str

p1: TaggedData[int] = {"data": 42, "tag": "number"}
p2: TaggedData[str] = {"data": "Hello", "tag": "text"}

# TODO: this should be an error (type mismatch)
p3: TaggedData[int] = {"data": "not a number", "tag": "number"}
```

## Recursive `TypedDict`

`TypedDict`s can also be recursive, allowing for nested structures:

```py
from __future__ import annotations
from typing import TypedDict

class Node(TypedDict):
    name: str
    parent: Node | None

root: Node = {"name": "root", "parent": None}
child: Node = {"name": "child", "parent": root}
grandchild: Node = {"name": "grandchild", "parent": child}

nested: Node = {"name": "n1", "parent": {"name": "n2", "parent": {"name": "n3", "parent": None}}}

# TODO: this should be an error (invalid type for `name` in innermost node)
nested_invalid: Node = {"name": "n1", "parent": {"name": "n2", "parent": {"name": 3, "parent": None}}}
```

## Function/assignment syntax

This is not yet supported. Make sure that we do not emit false positives for this syntax:

```py
from typing_extensions import TypedDict, Required

# Alternative syntax
Message = TypedDict("Message", {"id": Required[int], "content": str}, total=False)

msg = Message(id=1, content="Hello")

# No errors for yet-unsupported features (`closed`):
OtherMessage = TypedDict("OtherMessage", {"id": int, "content": str}, closed=True)

reveal_type(Message.__required_keys__)  # revealed: @Todo(Support for `TypedDict`)
```

[`typeddict`]: https://typing.python.org/en/latest/spec/typeddict.html
