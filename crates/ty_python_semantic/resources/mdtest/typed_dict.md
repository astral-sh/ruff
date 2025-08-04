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

New inhabitants can be created from dict literals. When accessing keys, the correct types should be
inferred based on the `TypedDict` definition:

```py
alice: Person = {"name": "Alice", "age": 30}

# TODO: this should be `str`
reveal_type(alice["name"])  # revealed: Unknown
# TODO: this should be `int | None`
reveal_type(alice["age"])  # revealed: Unknown
```

Inhabitants can also be created through a constructor call:

```py
bob = Person(name="Bob", age=25)
```

Methods that are available on `dict`s are also available on `TypedDict`s:

```py
bob.update(age=26)
```

The construction of a `TypedDict` is checked for type correctness:

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
# TODO: this should be an error
bob["name"] = None
```

Assignments to non-existing keys are disallowed:

```py
# TODO: this should be an error
alice["extra"] = True
# TODO: this should be an error
bob["extra"] = True
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

They can *not* be assigned to `dict[str, object]`, as that would allow them to be mutated in unsafe
ways:

```py
from typing import TypedDict

def dangerous(d: dict[str, object]) -> None:
    d["name"] = 1

class Person(TypedDict):
    name: str

alice: Person = {"name": "Alice"}

# TODO: this should be an invalid-assignment error
dangerous(alice)

# TODO: this should be `str`
reveal_type(alice["name"])  # revealed: Unknown
```

## Methods on `TypedDict`

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

def _(p: Person) -> None:
    reveal_type(p.keys())  # revealed: dict_keys[str, object]
    reveal_type(p.values())  # revealed: dict_values[str, object]

    reveal_type(p.setdefault("name", "Alice"))  # revealed: @Todo(Support for `TypedDict`)

    reveal_type(p.get("name"))  # revealed: @Todo(Support for `TypedDict`)
    reveal_type(p.get("name", "Unknown"))  # revealed: @Todo(Support for `TypedDict`)
```

## Unlike normal classes

`TypedDict` types are not like normal classes. The "attributes" can not be accessed. Neither on the
class itself, nor on inhabitants of the type defined by the class:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

# error: [unresolved-attribute] "Type `<class 'Person'>` has no attribute `name`"
Person.name

def _(P: type[Person]):
    # error: [unresolved-attribute] "Type `type[Person]` has no attribute `name`"
    P.name

def _(p: Person) -> None:
    # error: [unresolved-attribute] "Type `Person` has no attribute `name`"
    p.name
```

## Special properties

`TypedDict` class definitions have some special properties that can be used for introspection:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

reveal_type(Person.__total__)  # revealed: bool
reveal_type(Person.__required_keys__)  # revealed: frozenset[str]
reveal_type(Person.__optional_keys__)  # revealed: frozenset[str]
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

`TypedDict`s can also be generic.

### Legacy generics

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

### PEP-695 generics

```toml
[environment]
python-version = "3.12"
```

```py
from typing import TypedDict

class TaggedData[T](TypedDict):
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

# TODO: this should be an error
msg.content
```

[`typeddict`]: https://typing.python.org/en/latest/spec/typeddict.html
