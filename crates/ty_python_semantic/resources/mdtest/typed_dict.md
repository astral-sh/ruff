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

# TODO: this should reveal `Unknown`, and it should emit an error
reveal_type(alice["non_existing"])  # revealed: Unknown
```

Inhabitants can also be created through a constructor call:

```py
bob = Person(name="Bob", age=25)

reveal_type(bob["name"])  # revealed: str
reveal_type(bob["age"])  # revealed: int | None

# error: [invalid-key] "Invalid key access on TypedDict `Person`: Unknown key "non_existing""
reveal_type(bob["non_existing"])  # revealed: Unknown
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

# error: [invalid-assignment] "Invalid assignment to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
bob["name"] = None
```

Assignments to non-existing keys are disallowed:

```py
# TODO: this should be an error
alice["extra"] = True

# error: [invalid-key] "Invalid key access on TypedDict `Person`: Unknown key "extra""
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

## Key-based access

### Reading

```py
from typing import TypedDict, Final, Literal, Any

class Person(TypedDict):
    name: str
    age: int | None

NAME_FINAL: Final = "name"
AGE_FINAL: Final[Literal["age"]] = "age"

def _(person: Person, literal_key: Literal["age"], union_of_keys: Literal["age", "name"], str_key: str, unknown_key: Any) -> None:
    reveal_type(person["name"])  # revealed: str
    reveal_type(person["age"])  # revealed: int | None

    reveal_type(person[NAME_FINAL])  # revealed: str
    reveal_type(person[AGE_FINAL])  # revealed: int | None

    reveal_type(person[literal_key])  # revealed: int | None

    reveal_type(person[union_of_keys])  # revealed: int | None | str

    # error: [invalid-key] "Invalid key access on TypedDict `Person`: Unknown key "non_existing""
    reveal_type(person["non_existing"])  # revealed: Unknown

    # error: [invalid-key] "TypedDict `Person` cannot be indexed with a key of type `str`"
    reveal_type(person[str_key])  # revealed: Unknown

    # No error here:
    reveal_type(person[unknown_key])  # revealed: Unknown
```

### Writing

```py
from typing_extensions import TypedDict, Final, Literal, LiteralString, Any

class Person(TypedDict):
    name: str
    surname: str
    age: int | None

NAME_FINAL: Final = "name"
AGE_FINAL: Final[Literal["age"]] = "age"

def _(person: Person):
    person["name"] = "Alice"
    person["age"] = 30

    # error: [invalid-key] "Invalid key access on TypedDict `Person`: Unknown key "naem" - did you mean "name"?"
    person["naem"] = "Alice"

def _(person: Person):
    person[NAME_FINAL] = "Alice"
    person[AGE_FINAL] = 30

def _(person: Person, literal_key: Literal["age"]):
    person[literal_key] = 22

def _(person: Person, union_of_keys: Literal["name", "surname"]):
    person[union_of_keys] = "unknown"

    # error: [invalid-assignment] "Cannot assign value of type `Literal[1]` to key of type `Literal["name", "surname"]` on TypedDict `Person`"
    person[union_of_keys] = 1

def _(person: Person, union_of_keys: Literal["name", "age"], unknown_value: Any):
    person[union_of_keys] = unknown_value

    # error: [invalid-assignment] "Cannot assign value of type `None` to key of type `Literal["name", "age"]` on TypedDict `Person`"
    person[union_of_keys] = None

def _(person: Person, str_key: str, literalstr_key: LiteralString):
    # error: [invalid-key] "Cannot access `Person` with a key of type `str`. Only string literals are allowed as keys on TypedDicts."
    person[str_key] = None

    # error: [invalid-key] "Cannot access `Person` with a key of type `LiteralString`. Only string literals are allowed as keys on TypedDicts."
    person[literalstr_key] = None

def _(person: Person, unknown_key: Any):
    # No error here:
    person[unknown_key] = "Eve"
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

`TypedDict` types do not act like normal classes. For example, calling `type(..)` on an inhabitant
of a `TypedDict` type will return `dict`:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

def _(p: Person) -> None:
    reveal_type(type(p))  # revealed: <class 'dict[str, object]'>

    reveal_type(p.__class__)  # revealed: <class 'dict[str, object]'>
```

Also, the "attributes" on the class definition can not be accessed. Neither on the class itself, nor
on inhabitants of the type defined by the class:

```py
# error: [unresolved-attribute] "Type `<class 'Person'>` has no attribute `name`"
Person.name

def _(P: type[Person]):
    # error: [unresolved-attribute] "Type `type[Person]` has no attribute `name`"
    P.name

def _(p: Person) -> None:
    # error: [unresolved-attribute] "Type `Person` has no attribute `name`"
    p.name

    type(p).name  # error: [unresolved-attribute] "Type `<class 'dict[str, object]'>` has no attribute `name`"
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

These attributes can not be accessed on inhabitants:

```py
def _(person: Person) -> None:
    person.__total__  # error: [unresolved-attribute]
    person.__required_keys__  # error: [unresolved-attribute]
    person.__optional_keys__  # error: [unresolved-attribute]
```

Also, they can not be accessed on `type(person)`, as that would be `dict` at runtime:

```py
def _(person: Person) -> None:
    type(person).__total__  # error: [unresolved-attribute]
    type(person).__required_keys__  # error: [unresolved-attribute]
    type(person).__optional_keys__  # error: [unresolved-attribute]
```

But they *can* be accessed on `type[Person]`, because this function would accept the class object
`Person` as an argument:

```py
def accepts_typed_dict_class(t_person: type[Person]) -> None:
    reveal_type(t_person.__total__)  # revealed: bool
    reveal_type(t_person.__required_keys__)  # revealed: frozenset[str]
    reveal_type(t_person.__optional_keys__)  # revealed: frozenset[str]

accepts_typed_dict_class(Person)
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

## Error cases

### `typing.TypedDict` is not allowed in type expressions

```py
from typing import TypedDict

# error: [invalid-type-form] "The special form `typing.TypedDict` is not allowed in type expressions."
x: TypedDict = {"name": "Alice"}
```

## Diagnostics

<!-- snapshot-diagnostics -->

Snapshot tests for diagnostic messages including suggestions:

```py
from typing import TypedDict, Final

class Person(TypedDict):
    name: str
    age: int | None

def access_invalid_literal_string_key(person: Person):
    person["naem"]  # error: [invalid-key]

NAME_KEY: Final = "naem"

def access_invalid_key(person: Person):
    person[NAME_KEY]  # error: [invalid-key]

def access_with_str_key(person: Person, str_key: str):
    person[str_key]  # error: [invalid-key]

def write_to_key_with_wrong_type(person: Person):
    person["age"] = "42"  # error: [invalid-assignment]

def write_to_non_existing_key(person: Person):
    person["naem"] = "Alice"  # error: [invalid-key]

def write_to_non_literal_string_key(person: Person, str_key: str):
    person[str_key] = "Alice"  # error: [invalid-key]
```

[`typeddict`]: https://typing.python.org/en/latest/spec/typeddict.html
