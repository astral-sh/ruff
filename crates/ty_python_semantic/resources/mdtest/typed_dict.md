# `TypedDict`

A [`TypedDict`] type represents dictionary objects with a specific set of string keys, and with
specific value types for each valid key. Each string key can be either required or non-required.

## Basic

```toml
[environment]
python-version = "3.12"
```

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

reveal_type(alice["name"])  # revealed: str
reveal_type(alice["age"])  # revealed: int | None

# error: [invalid-key] "Unknown key "non_existing" for TypedDict `Person`"
reveal_type(alice["non_existing"])  # revealed: Unknown
```

Inhabitants can also be created through a constructor call:

```py
bob = Person(name="Bob", age=25)

reveal_type(bob["name"])  # revealed: str
reveal_type(bob["age"])  # revealed: int | None

# error: [invalid-key] " key "non_existing" for TypedDict `Person`"
reveal_type(bob["non_existing"])  # revealed: Unknown
```

Methods that are available on `dict`s are also available on `TypedDict`s:

```py
bob.update(age=26)
```

`TypedDict` keys do not have to be string literals, as long as they can be statically determined
(inferred to be of type string `Literal`).

```py
from typing import Literal, Final

NAME = "name"
AGE = "age"

def non_literal() -> str:
    return "name"

def name_or_age() -> Literal["name", "age"]:
    return "name"

carol: Person = {NAME: "Carol", AGE: 20}

reveal_type(carol[NAME])  # revealed: str
# error: [invalid-key] "TypedDict `Person` can only be subscripted with a string literal key, got key of type `str`"
reveal_type(carol[non_literal()])  # revealed: Unknown
reveal_type(carol[name_or_age()])  # revealed: str | int | None

FINAL_NAME: Final = "name"
FINAL_AGE: Final = "age"

def _():
    carol: Person = {FINAL_NAME: "Carol", FINAL_AGE: 20}

CAPITALIZED_NAME = "Name"

# error: [invalid-key] "Unknown key "Name" for TypedDict `Person` - did you mean "name"?"
# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `Person` constructor"
dave: Person = {CAPITALIZED_NAME: "Dave", "age": 20}

def age() -> Literal["age"] | None:
    return "age"

eve: Person = {"na" + "me": "Eve", age() or "age": 20}
```

The construction of a `TypedDict` is checked for type correctness:

```py
# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Person`"
eve1a: Person = {"name": b"Eve", "age": None}

# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Person`"
eve1b = Person(name=b"Eve", age=None)

reveal_type(eve1a)  # revealed: Person
reveal_type(eve1b)  # revealed: Person

# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `Person` constructor"
eve2a: Person = {"age": 22}

# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `Person` constructor"
eve2b = Person(age=22)

reveal_type(eve2a)  # revealed: Person
reveal_type(eve2b)  # revealed: Person

# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
eve3a: Person = {"name": "Eve", "age": 25, "extra": True}

# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
eve3b = Person(name="Eve", age=25, extra=True)

reveal_type(eve3a)  # revealed: Person
reveal_type(eve3b)  # revealed: Person
```

Also, the value types ​​declared in a `TypedDict` affect generic call inference:

```py
class Plot(TypedDict):
    y: list[int | None]
    x: list[int | None] | None

plot1: Plot = {"y": [1, 2, 3], "x": None}

def homogeneous_list[T](*args: T) -> list[T]:
    return list(args)

reveal_type(homogeneous_list(1, 2, 3))  # revealed: list[int]
plot2: Plot = {"y": homogeneous_list(1, 2, 3), "x": None}
reveal_type(plot2["y"])  # revealed: list[int | None]

plot3: Plot = {"y": homogeneous_list(1, 2, 3), "x": homogeneous_list(1, 2, 3)}
reveal_type(plot3["y"])  # revealed: list[int | None]
reveal_type(plot3["x"])  # revealed: list[int | None] | None

Y = "y"
X = "x"

plot4: Plot = {Y: [1, 2, 3], X: None}
plot5: Plot = {Y: homogeneous_list(1, 2, 3), X: None}

class Items(TypedDict):
    items: list[int | str]

items1: Items = {"items": homogeneous_list(1, 2, 3)}
ITEMS = "items"
items2: Items = {ITEMS: homogeneous_list(1, 2, 3)}
```

Assignments to keys are also validated:

```py
# error: [invalid-assignment] "Invalid assignment to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
alice["name"] = None

# error: [invalid-assignment] "Invalid assignment to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
bob["name"] = None
```

Assignments to non-existing keys are disallowed:

```py
# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
alice["extra"] = True

# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
bob["extra"] = True
```

## Nested `TypedDict`

Nested `TypedDict` fields are also supported.

```py
from typing import TypedDict

class Inner(TypedDict):
    name: str
    age: int | None

class Person(TypedDict):
    inner: Inner
```

```py
alice: Person = {"inner": {"name": "Alice", "age": 30}}

reveal_type(alice["inner"]["name"])  # revealed: str
reveal_type(alice["inner"]["age"])  # revealed: int | None

# error: [invalid-key] "Unknown key "non_existing" for TypedDict `Inner`"
reveal_type(alice["inner"]["non_existing"])  # revealed: Unknown

# error: [invalid-key] "Unknown key "extra" for TypedDict `Inner`"
alice: Person = {"inner": {"name": "Alice", "age": 30, "extra": 1}}
```

## Validation of `TypedDict` construction

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

class House:
    owner: Person

house = House()

def accepts_person(p: Person) -> None:
    pass
```

The following constructions of `Person` are all valid:

```py
alice1: Person = {"name": "Alice", "age": 30}
Person(name="Alice", age=30)
Person({"name": "Alice", "age": 30})

accepts_person({"name": "Alice", "age": 30})
house.owner = {"name": "Alice", "age": 30}
```

All of these are missing the required `age` field:

```py
# error: [missing-typed-dict-key] "Missing required key 'age' in TypedDict `Person` constructor"
alice2: Person = {"name": "Alice"}

# error: [missing-typed-dict-key] "Missing required key 'age' in TypedDict `Person` constructor"
Person(name="Alice")

# error: [missing-typed-dict-key] "Missing required key 'age' in TypedDict `Person` constructor"
Person({"name": "Alice"})

# error: [missing-typed-dict-key] "Missing required key 'age' in TypedDict `Person` constructor"
# error: [invalid-argument-type]
accepts_person({"name": "Alice"})

# TODO: this should be an invalid-key error, similar to the above
# error: [invalid-assignment]
house.owner = {"name": "Alice"}

a_person: Person
# error: [missing-typed-dict-key] "Missing required key 'age' in TypedDict `Person` constructor"
a_person = {"name": "Alice"}
```

All of these have an invalid type for the `name` field:

```py
# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
alice3: Person = {"name": None, "age": 30}

# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
Person(name=None, age=30)

# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
Person({"name": None, "age": 30})

# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
# error: [invalid-argument-type]
accepts_person({"name": None, "age": 30})

# TODO: this should be an invalid-key error
# error: [invalid-assignment]
house.owner = {"name": None, "age": 30}

a_person: Person
# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
a_person = {"name": None, "age": 30}

# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
(a_person := {"name": None, "age": 30})
```

All of these have an extra field that is not defined in the `TypedDict`:

```py
# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
alice4: Person = {"name": "Alice", "age": 30, "extra": True}

# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
Person(name="Alice", age=30, extra=True)

# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
Person({"name": "Alice", "age": 30, "extra": True})

# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
# error: [invalid-argument-type]
accepts_person({"name": "Alice", "age": 30, "extra": True})

# TODO: this should be an invalid-key error
# error: [invalid-assignment]
house.owner = {"name": "Alice", "age": 30, "extra": True}

a_person: Person
# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
a_person = {"name": "Alice", "age": 30, "extra": True}

# error: [invalid-key] "Unknown key "extra" for TypedDict `Person`"
(a_person := {"name": "Alice", "age": 30, "extra": True})
```

## Type ignore compatibility issues

Users should be able to ignore TypedDict validation errors with `# type: ignore`

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int

alice_bad: Person = {"name": None}  # type: ignore
Person(name=None, age=30)  # type: ignore
Person(name="Alice", age=30, extra=True)  # type: ignore
```

## Positional dictionary constructor pattern

The positional dictionary constructor pattern (used by libraries like strawberry) should work
correctly:

```py
from typing import TypedDict

class User(TypedDict):
    name: str
    age: int

# Valid usage - all required fields provided
user1 = User({"name": "Alice", "age": 30})

# error: [missing-typed-dict-key] "Missing required key 'age' in TypedDict `User` constructor"
user2 = User({"name": "Bob"})

# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `User`: value of type `None`"
user3 = User({"name": None, "age": 25})

# error: [invalid-key] "Unknown key "extra" for TypedDict `User`"
user4 = User({"name": "Charlie", "age": 30, "extra": True})
```

## Optional fields with `total=False`

By default, all fields in a `TypedDict` are required (`total=True`). You can make all fields
optional by setting `total=False`:

```py
from typing import TypedDict

class OptionalPerson(TypedDict, total=False):
    name: str
    age: int | None

# All fields are optional with total=False
charlie = OptionalPerson()
david = OptionalPerson(name="David")
emily = OptionalPerson(age=30)
frank = OptionalPerson(name="Frank", age=25)

# TODO: we could emit an error here, because these fields are not guaranteed to exist
reveal_type(charlie["name"])  # revealed: str
reveal_type(david["age"])  # revealed: int | None
```

Type validation still applies to provided fields:

```py
# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `OptionalPerson`"
invalid = OptionalPerson(name=123)
```

Extra fields are still not allowed, even with `total=False`:

```py
# error: [invalid-key] "Unknown key "extra" for TypedDict `OptionalPerson`"
invalid_extra = OptionalPerson(name="George", extra=True)
```

## `Required` and `NotRequired`

You can have fine-grained control over keys using `Required` and `NotRequired` qualifiers. These
qualifiers override the class-level `total` setting, which sets the default (`total=True` means that
all keys are required by default, `total=False` means that all keys are non-required by default):

```py
from typing_extensions import TypedDict, Required, NotRequired, Final

# total=False by default, but id is explicitly Required
class Message(TypedDict, total=False):
    id: Required[int]  # Always required, even though total=False
    content: str  # Optional due to total=False
    timestamp: NotRequired[str]  # Explicitly optional (redundant here)

# total=True by default, but content is explicitly NotRequired
class User(TypedDict):
    name: str  # Required due to total=True (default)
    email: Required[str]  # Explicitly required (redundant here)
    bio: NotRequired[str]  # Optional despite total=True

ID: Final = "id"

# Valid Message constructions
msg1 = Message(id=1)  # id required, content optional
msg2 = Message(id=2, content="Hello")  # both provided
msg3 = Message(id=3, timestamp="2024-01-01")  # id required, timestamp optional
msg4: Message = {"id": 4}  # id required, content optional
msg5: Message = {ID: 5}  # id required, content optional

def msg() -> Message:
    return {ID: 1}

# Valid User constructions
user1 = User(name="Alice", email="alice@example.com")  # required fields
user2 = User(name="Bob", email="bob@example.com", bio="Developer")  # with optional bio

reveal_type(msg1["id"])  # revealed: int
reveal_type(msg1["content"])  # revealed: str
reveal_type(user1["name"])  # revealed: str
reveal_type(user1["bio"])  # revealed: str
```

Constructor validation respects `Required`/`NotRequired` overrides:

```py
# error: [missing-typed-dict-key] "Missing required key 'id' in TypedDict `Message` constructor"
invalid_msg = Message(content="Hello")  # Missing required id

# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `User` constructor"
# error: [missing-typed-dict-key] "Missing required key 'email' in TypedDict `User` constructor"
invalid_user = User(bio="No name provided")  # Missing required name and email
```

Type validation still applies to all fields when provided:

```py
# error: [invalid-argument-type] "Invalid argument to key "id" with declared type `int` on TypedDict `Message`"
invalid_type = Message(id="not-an-int", content="Hello")
```

## Structural assignability

Assignability between `TypedDict` types is structural, that is, it is based on the presence of keys
and their types, rather than the class hierarchy:

```py
from typing import TypedDict
from typing_extensions import ReadOnly
from ty_extensions import static_assert, is_assignable_to, is_subtype_of

class Person(TypedDict):
    name: str

class Employee(TypedDict):
    name: str
    employee_id: int

class Robot(TypedDict):
    name: int

static_assert(is_assignable_to(Employee, Person))

static_assert(not is_assignable_to(Person, Employee))
static_assert(not is_assignable_to(Robot, Person))
static_assert(not is_assignable_to(Person, Robot))
```

In order for one `TypedDict` `B` to be assignable to another `TypedDict` `A`, all required keys in
`A`'s schema must be required in `B`'s schema. If a key is not-required and also mutable in `A`,
then it must be not-required in `B` (because `A` allows the caller to `del` that key). These rules
cover keys that are explicitly marked `NotRequired`, and also all the keys in a `TypedDict` with
`total=False`.

```py
from typing_extensions import NotRequired

class Spy1(TypedDict):
    name: NotRequired[str]

class Spy2(TypedDict, total=False):
    name: str

# invalid because `Spy1` and `Spy2` might be missing `name`
static_assert(not is_assignable_to(Spy1, Person))
static_assert(not is_assignable_to(Spy2, Person))

# invalid because `Spy1` and `Spy2` are allowed to delete `name`, while `Person` is not
static_assert(not is_assignable_to(Person, Spy1))
static_assert(not is_assignable_to(Person, Spy2))

class Amnesiac1(TypedDict):
    name: NotRequired[ReadOnly[str]]

class Amnesiac2(TypedDict, total=False):
    name: ReadOnly[str]

# invalid because `Amnesiac1` and `Amnesiac2` might be missing `name`
static_assert(not is_assignable_to(Amnesiac1, Person))
static_assert(not is_assignable_to(Amnesiac2, Person))

# Allowed. Neither `Amnesiac1` nor `Amnesiac2` can delete `name`, because it's read-only.
static_assert(is_assignable_to(Person, Amnesiac1))
static_assert(is_assignable_to(Person, Amnesiac2))
```

If an item in `A` (the destination `TypedDict` type) is read-only, then the corresponding item in
`B` can have any assignable type. But if the item in `A` is mutable, the item type in `B` must be
"consistent", i.e. both assignable-to and assignable-from. (For fully-static types, consistent is
the same as equivalent.) The required and not-required cases are different codepaths, so we need
test all the permutations:

```py
from typing import Any
from typing_extensions import ReadOnly

class RequiredMutableInt(TypedDict):
    x: int

class RequiredReadOnlyInt(TypedDict):
    x: ReadOnly[int]

class NotRequiredMutableInt(TypedDict):
    x: NotRequired[int]

class NotRequiredReadOnlyInt(TypedDict):
    x: NotRequired[ReadOnly[int]]

class RequiredMutableBool(TypedDict):
    x: bool

class RequiredReadOnlyBool(TypedDict):
    x: ReadOnly[bool]

class NotRequiredMutableBool(TypedDict):
    x: NotRequired[bool]

class NotRequiredReadOnlyBool(TypedDict):
    x: NotRequired[ReadOnly[bool]]

class RequiredMutableAny(TypedDict):
    x: Any

class RequiredReadOnlyAny(TypedDict):
    x: ReadOnly[Any]

class NotRequiredMutableAny(TypedDict):
    x: NotRequired[Any]

class NotRequiredReadOnlyAny(TypedDict):
    x: NotRequired[ReadOnly[Any]]

# fmt: off
static_assert(    is_assignable_to( RequiredMutableInt,      RequiredMutableInt))
static_assert(       is_subtype_of( RequiredMutableInt,      RequiredMutableInt))
static_assert(not is_assignable_to( RequiredReadOnlyInt,     RequiredMutableInt))
static_assert(not    is_subtype_of( RequiredReadOnlyInt,     RequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredMutableInt,   RequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredMutableInt,   RequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyInt,  RequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyInt,  RequiredMutableInt))
static_assert(not is_assignable_to( RequiredMutableBool,     RequiredMutableInt))
static_assert(not    is_subtype_of( RequiredMutableBool,     RequiredMutableInt))
static_assert(not is_assignable_to( RequiredReadOnlyBool,    RequiredMutableInt))
static_assert(not    is_subtype_of( RequiredReadOnlyBool,    RequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredMutableBool,  RequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredMutableBool,  RequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyBool, RequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyBool, RequiredMutableInt))
static_assert(    is_assignable_to( RequiredMutableAny,      RequiredMutableInt))
static_assert(not    is_subtype_of( RequiredMutableAny,      RequiredMutableInt))
static_assert(not is_assignable_to( RequiredReadOnlyAny,     RequiredMutableInt))
static_assert(not    is_subtype_of( RequiredReadOnlyAny,     RequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredMutableAny,   RequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredMutableAny,   RequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyAny,  RequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyAny,  RequiredMutableInt))

static_assert(    is_assignable_to( RequiredMutableInt,      RequiredReadOnlyInt))
static_assert(       is_subtype_of( RequiredMutableInt,      RequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredReadOnlyInt,     RequiredReadOnlyInt))
static_assert(       is_subtype_of( RequiredReadOnlyInt,     RequiredReadOnlyInt))
static_assert(not is_assignable_to( NotRequiredMutableInt,   RequiredReadOnlyInt))
static_assert(not    is_subtype_of( NotRequiredMutableInt,   RequiredReadOnlyInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyInt,  RequiredReadOnlyInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyInt,  RequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredMutableBool,     RequiredReadOnlyInt))
static_assert(       is_subtype_of( RequiredMutableBool,     RequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredReadOnlyBool,    RequiredReadOnlyInt))
static_assert(       is_subtype_of( RequiredReadOnlyBool,    RequiredReadOnlyInt))
static_assert(not is_assignable_to( NotRequiredMutableBool,  RequiredReadOnlyInt))
static_assert(not    is_subtype_of( NotRequiredMutableBool,  RequiredReadOnlyInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyBool, RequiredReadOnlyInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyBool, RequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredMutableAny,      RequiredReadOnlyInt))
static_assert(not    is_subtype_of( RequiredMutableAny,      RequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredReadOnlyAny,     RequiredReadOnlyInt))
static_assert(not    is_subtype_of( RequiredReadOnlyAny,     RequiredReadOnlyInt))
static_assert(not is_assignable_to( NotRequiredMutableAny,   RequiredReadOnlyInt))
static_assert(not    is_subtype_of( NotRequiredMutableAny,   RequiredReadOnlyInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyAny,  RequiredReadOnlyInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyAny,  RequiredReadOnlyInt))

static_assert(not is_assignable_to( RequiredMutableInt,      NotRequiredMutableInt))
static_assert(not    is_subtype_of( RequiredMutableInt,      NotRequiredMutableInt))
static_assert(not is_assignable_to( RequiredReadOnlyInt,     NotRequiredMutableInt))
static_assert(not    is_subtype_of( RequiredReadOnlyInt,     NotRequiredMutableInt))
static_assert(    is_assignable_to( NotRequiredMutableInt,   NotRequiredMutableInt))
static_assert(       is_subtype_of( NotRequiredMutableInt,   NotRequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyInt,  NotRequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyInt,  NotRequiredMutableInt))
static_assert(not is_assignable_to( RequiredMutableBool,     NotRequiredMutableInt))
static_assert(not    is_subtype_of( RequiredMutableBool,     NotRequiredMutableInt))
static_assert(not is_assignable_to( RequiredReadOnlyBool,    NotRequiredMutableInt))
static_assert(not    is_subtype_of( RequiredReadOnlyBool,    NotRequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredMutableBool,  NotRequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredMutableBool,  NotRequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyBool, NotRequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyBool, NotRequiredMutableInt))
static_assert(not is_assignable_to( RequiredMutableAny,      NotRequiredMutableInt))
static_assert(not    is_subtype_of( RequiredMutableAny,      NotRequiredMutableInt))
static_assert(not is_assignable_to( RequiredReadOnlyAny,     NotRequiredMutableInt))
static_assert(not    is_subtype_of( RequiredReadOnlyAny,     NotRequiredMutableInt))
static_assert(    is_assignable_to( NotRequiredMutableAny,   NotRequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredMutableAny,   NotRequiredMutableInt))
static_assert(not is_assignable_to( NotRequiredReadOnlyAny,  NotRequiredMutableInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyAny,  NotRequiredMutableInt))

static_assert(    is_assignable_to( RequiredMutableInt,      NotRequiredReadOnlyInt))
static_assert(       is_subtype_of( RequiredMutableInt,      NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredReadOnlyInt,     NotRequiredReadOnlyInt))
static_assert(       is_subtype_of( RequiredReadOnlyInt,     NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( NotRequiredMutableInt,   NotRequiredReadOnlyInt))
static_assert(       is_subtype_of( NotRequiredMutableInt,   NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( NotRequiredReadOnlyInt,  NotRequiredReadOnlyInt))
static_assert(       is_subtype_of( NotRequiredReadOnlyInt,  NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredMutableBool,     NotRequiredReadOnlyInt))
static_assert(       is_subtype_of( RequiredMutableBool,     NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredReadOnlyBool,    NotRequiredReadOnlyInt))
static_assert(       is_subtype_of( RequiredReadOnlyBool,    NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( NotRequiredMutableBool,  NotRequiredReadOnlyInt))
static_assert(       is_subtype_of( NotRequiredMutableBool,  NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( NotRequiredReadOnlyBool, NotRequiredReadOnlyInt))
static_assert(       is_subtype_of( NotRequiredReadOnlyBool, NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredMutableAny,      NotRequiredReadOnlyInt))
static_assert(not    is_subtype_of( RequiredMutableAny,      NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( RequiredReadOnlyAny,     NotRequiredReadOnlyInt))
static_assert(not    is_subtype_of( RequiredReadOnlyAny,     NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( NotRequiredMutableAny,   NotRequiredReadOnlyInt))
static_assert(not    is_subtype_of( NotRequiredMutableAny,   NotRequiredReadOnlyInt))
static_assert(    is_assignable_to( NotRequiredReadOnlyAny,  NotRequiredReadOnlyInt))
static_assert(not    is_subtype_of( NotRequiredReadOnlyAny,  NotRequiredReadOnlyInt))
# fmt: on
```

All typed dictionaries can be assigned to `Mapping[str, object]`:

```py
from typing import Mapping, TypedDict

class Person(TypedDict):
    name: str
    age: int | None

alice = Person(name="Alice", age=30)
# Always assignable.
_: Mapping[str, object] = alice
# Follows from above.
_: Mapping[str, Any] = alice
# Also follows from above, because `update` accepts the `SupportsKeysAndGetItem` protocol.
{}.update(alice)
# Not assignable.
# error: [invalid-assignment] "Object of type `Person` is not assignable to `Mapping[str, int]`"
_: Mapping[str, int] = alice
# `Person` does not have `closed=True` or `extra_items`, so it may have additional keys with values
# of unknown type, therefore it can't be assigned to a `Mapping` with value type smaller than `object`.
# error: [invalid-assignment]
_: Mapping[str, str | int | None] = alice
```

They *cannot* be assigned to `dict[str, object]`, as that would allow them to be mutated in unsafe
ways:

```py
from typing import TypedDict

def dangerous(d: dict[str, object]) -> None:
    d["name"] = 1

class Person(TypedDict):
    name: str

alice: Person = {"name": "Alice"}

# error: [invalid-argument-type] "Argument to function `dangerous` is incorrect: Expected `dict[str, object]`, found `Person`"
dangerous(alice)

reveal_type(alice["name"])  # revealed: str
```

Likewise, `dict`s are not assignable to typed dictionaries:

```py
alice: dict[str, str] = {"name": "Alice"}

# error: [invalid-assignment] "Object of type `dict[str, str]` is not assignable to `Person`"
alice: Person = alice
```

## A subtle interaction between two structural assignability rules prevents unsoundness

> For the purposes of these conditions, an open `TypedDict` is treated as if it had **read-only**
> extra items of type `object`.

That language is at the top of [subtyping section of the `TypedDict` spec][subtyping section]. It
sounds like an obscure technicality, especially since `extra_items` is still TODO, but it has an
important interaction with another rule:

> For each item in [the destination type]...If it is non-required...If it is mutable...If \[the
> source type does not have an item with the same key and also\] has extra items, the extra items
> type **must not be read-only**...

In other words, by default (`closed=False`) a `TypedDict` cannot be assigned to a different
`TypedDict` that has an additional, optional, mutable item. That implicit rule turns out to be the
only thing standing in the way of this unsound example:

```py
from typing_extensions import TypedDict, NotRequired

class C(TypedDict):
    x: int
    y: str

class B(TypedDict):
    x: int

class A(TypedDict):
    x: int
    y: NotRequired[object]  # incompatible with both C and (surprisingly!) B

def b_from_c(c: C) -> B:
    return c  # allowed

def a_from_b(b: B) -> A:
    # error: [invalid-return-type] "Return type does not match returned value: expected `A`, found `B`"
    return b

# The [invalid-return-type] error above is the only thing that keeps us from corrupting the type of c['y'].
c: C = {"x": 1, "y": "hello"}
a: A = a_from_b(b_from_c(c))
a["y"] = 42
```

If the additional, optional item in the target is read-only, the requirements are *somewhat*
relaxed. In this case, because the source might contain have undeclared extra items of any type, the
target item must be assignable from `object`:

```py
from typing_extensions import ReadOnly

class A2(TypedDict):
    x: int
    y: NotRequired[ReadOnly[object]]

def a2_from_b(b: B) -> A2:
    return b  # allowed

class A3(TypedDict):
    x: int
    y: NotRequired[ReadOnly[int]]  # not assignable from `object`

def a3_from_b(b: B) -> A3:
    return b  # error: [invalid-return-type]
```

## Structural assignability supports `TypedDict`s that contain other `TypedDict`s

```py
from typing_extensions import TypedDict, ReadOnly, NotRequired
from ty_extensions import static_assert, is_assignable_to, is_subtype_of

class Inner1(TypedDict):
    name: str

class Inner2(TypedDict):
    name: str

class Outer1(TypedDict):
    a: Inner1
    b: ReadOnly[Inner1]
    c: NotRequired[Inner1]
    d: ReadOnly[NotRequired[Inner1]]

class Outer2(TypedDict):
    a: Inner2
    b: ReadOnly[Inner2]
    c: NotRequired[Inner2]
    d: ReadOnly[NotRequired[Inner2]]

def _(o1: Outer1, o2: Outer2):
    static_assert(is_assignable_to(Outer1, Outer2))
    static_assert(is_subtype_of(Outer1, Outer2))
    static_assert(is_assignable_to(Outer2, Outer1))
    static_assert(is_subtype_of(Outer2, Outer1))
```

This also extends to gradual types:

```py
from typing import Any

class Inner3(TypedDict):
    name: Any

class Outer3(TypedDict):
    a: Inner3
    b: ReadOnly[Inner3]
    c: NotRequired[Inner3]
    d: ReadOnly[NotRequired[Inner3]]

class Outer4(TypedDict):
    a: Any
    b: ReadOnly[Any]
    c: NotRequired[Any]
    d: ReadOnly[NotRequired[Any]]

def _(o1: Outer1, o2: Outer2, o3: Outer3, o4: Outer4):
    static_assert(is_assignable_to(Outer3, Outer1))
    static_assert(not is_subtype_of(Outer3, Outer1))
    static_assert(is_assignable_to(Outer4, Outer1))
    static_assert(not is_subtype_of(Outer4, Outer1))

    static_assert(is_assignable_to(Outer3, Outer2))
    static_assert(not is_subtype_of(Outer3, Outer2))
    static_assert(is_assignable_to(Outer4, Outer2))
    static_assert(not is_subtype_of(Outer4, Outer2))

    static_assert(is_assignable_to(Outer1, Outer3))
    static_assert(not is_subtype_of(Outer1, Outer3))
    static_assert(is_assignable_to(Outer2, Outer3))
    static_assert(not is_subtype_of(Outer2, Outer3))
    static_assert(is_assignable_to(Outer3, Outer3))
    static_assert(is_subtype_of(Outer3, Outer3))
    static_assert(is_assignable_to(Outer4, Outer3))
    static_assert(not is_subtype_of(Outer4, Outer3))

    static_assert(is_assignable_to(Outer1, Outer4))
    static_assert(not is_subtype_of(Outer1, Outer4))
    static_assert(is_assignable_to(Outer2, Outer4))
    static_assert(not is_subtype_of(Outer2, Outer4))
    static_assert(is_assignable_to(Outer3, Outer4))
    static_assert(not is_subtype_of(Outer3, Outer4))
    static_assert(is_assignable_to(Outer4, Outer4))
    static_assert(is_subtype_of(Outer4, Outer4))
```

## Structural equivalence

Two `TypedDict`s with equivalent fields are equivalent types. This includes fields with gradual
types:

```py
from typing_extensions import Any, TypedDict, ReadOnly, assert_type
from ty_extensions import is_assignable_to, is_equivalent_to, static_assert

class Foo(TypedDict):
    x: int
    y: Any

# exactly the same fields
class Bar(TypedDict):
    x: int
    y: Any

# the same fields but in a different order
class Baz(TypedDict):
    y: Any
    x: int

static_assert(is_assignable_to(Foo, Bar))
static_assert(is_equivalent_to(Foo, Bar))
static_assert(is_assignable_to(Foo, Baz))
static_assert(is_equivalent_to(Foo, Baz))

foo: Foo = {"x": 1, "y": "hello"}
assert_type(foo, Foo)
assert_type(foo, Bar)
assert_type(foo, Baz)
```

Equivalent `TypedDict`s within unions can also produce equivalent unions, which currently relies on
"normalization" machinery:

```py
def f(var: Foo | int):
    assert_type(var, Foo | int)
    assert_type(var, Bar | int)
    assert_type(var, Baz | int)
    # TODO: Union simplification compares `TypedDict`s by name/identity to avoid cycles. This assert
    # should also pass once that's fixed.
    assert_type(var, Foo | Bar | Baz | int)  # error: [type-assertion-failure]
```

Here are several cases that are not equivalent. In particular, assignability does not imply
equivalence:

```py
class FewerFields(TypedDict):
    x: int

static_assert(is_assignable_to(Foo, FewerFields))
static_assert(not is_equivalent_to(Foo, FewerFields))

class DifferentMutability(TypedDict):
    x: int
    y: ReadOnly[Any]

static_assert(is_assignable_to(Foo, DifferentMutability))
static_assert(not is_equivalent_to(Foo, DifferentMutability))

class MoreFields(TypedDict):
    x: int
    y: Any
    z: str

static_assert(not is_assignable_to(Foo, MoreFields))
static_assert(not is_equivalent_to(Foo, MoreFields))

class DifferentFieldStaticType(TypedDict):
    x: str
    y: Any

static_assert(not is_assignable_to(Foo, DifferentFieldStaticType))
static_assert(not is_equivalent_to(Foo, DifferentFieldStaticType))

class DifferentFieldGradualType(TypedDict):
    x: int
    y: Any | str

static_assert(is_assignable_to(Foo, DifferentFieldGradualType))
static_assert(not is_equivalent_to(Foo, DifferentFieldGradualType))
```

## Structural equivalence understands the interaction between `Required`/`NotRequired` and `total`

```py
from ty_extensions import static_assert, is_equivalent_to
from typing_extensions import TypedDict, Required, NotRequired

class Foo1(TypedDict, total=False):
    x: int
    y: str

class Foo2(TypedDict):
    y: NotRequired[str]
    x: NotRequired[int]

static_assert(is_equivalent_to(Foo1, Foo2))
static_assert(is_equivalent_to(Foo1 | int, int | Foo2))

class Bar1(TypedDict, total=False):
    x: int
    y: Required[str]

class Bar2(TypedDict):
    y: str
    x: NotRequired[int]

static_assert(is_equivalent_to(Bar1, Bar2))
static_assert(is_equivalent_to(Bar1 | int, int | Bar2))
```

## Assignability and equivalence work with recursive `TypedDict`s

```py
from typing_extensions import TypedDict
from ty_extensions import static_assert, is_assignable_to, is_equivalent_to

class Node1(TypedDict):
    value: int
    next: "Node1" | None

class Node2(TypedDict):
    value: int
    next: "Node2" | None

static_assert(is_assignable_to(Node1, Node2))
static_assert(is_equivalent_to(Node1, Node2))

class Person1(TypedDict):
    name: str
    friends: list["Person1"]

class Person2(TypedDict):
    name: str
    friends: list["Person2"]

static_assert(is_assignable_to(Person1, Person2))
static_assert(is_equivalent_to(Person1, Person2))
```

## Redundant cast warnings

<!-- snapshot-diagnostics -->

Casting between equivalent types produces a redundant cast warning. When the types have different
names, the warning makes that clear:

```py
from typing import TypedDict, cast

class Foo2(TypedDict):
    x: int

class Bar2(TypedDict):
    x: int

foo: Foo2 = {"x": 1}
_ = cast(Foo2, foo)  # error: [redundant-cast]
_ = cast(Bar2, foo)  # error: [redundant-cast]
```

## Key-based access

### Reading

```py
from typing import TypedDict, Final, Literal, Any

class Person(TypedDict):
    name: str
    age: int | None

class Animal(TypedDict):
    name: str

NAME_FINAL: Final = "name"
AGE_FINAL: Final[Literal["age"]] = "age"

def _(
    person: Person,
    being: Person | Animal,
    literal_key: Literal["age"],
    union_of_keys: Literal["age", "name"],
    str_key: str,
    unknown_key: Any,
) -> None:
    reveal_type(person["name"])  # revealed: str
    reveal_type(person["age"])  # revealed: int | None

    reveal_type(person[NAME_FINAL])  # revealed: str
    reveal_type(person[AGE_FINAL])  # revealed: int | None

    reveal_type(person[literal_key])  # revealed: int | None

    reveal_type(person[union_of_keys])  # revealed: int | None | str

    # error: [invalid-key] "Unknown key "non_existing" for TypedDict `Person`"
    reveal_type(person["non_existing"])  # revealed: Unknown

    # error: [invalid-key] "TypedDict `Person` can only be subscripted with a string literal key, got key of type `str`"
    reveal_type(person[str_key])  # revealed: Unknown

    # No error here:
    reveal_type(person[unknown_key])  # revealed: Unknown

    reveal_type(being["name"])  # revealed: str

    # TODO: A type of `int | None | Unknown` might be better here. The `str` is mixed in
    # because `Animal.__getitem__` can only return `str`.
    # error: [invalid-key] "Unknown key "age" for TypedDict `Animal`"
    reveal_type(being["age"])  # revealed: int | None | str
```

### Writing

```py
from typing_extensions import TypedDict, Final, Literal, LiteralString, Any
from ty_extensions import Intersection

class Person(TypedDict):
    name: str
    surname: str
    age: int | None

class Animal(TypedDict):
    name: str
    legs: int

NAME_FINAL: Final = "name"
AGE_FINAL: Final[Literal["age"]] = "age"

def _(person: Person):
    person["name"] = "Alice"
    person["age"] = 30

    # error: [invalid-key] "Unknown key "naem" for TypedDict `Person` - did you mean "name"?"
    person["naem"] = "Alice"

def _(person: Person):
    person[NAME_FINAL] = "Alice"
    person[AGE_FINAL] = 30

def _(person: Person, literal_key: Literal["age"]):
    person[literal_key] = 22

def _(person: Person, union_of_keys: Literal["name", "surname"]):
    person[union_of_keys] = "unknown"

    # error: [invalid-assignment] "Invalid assignment to key "name" with declared type `str` on TypedDict `Person`: value of type `Literal[1]`"
    # error: [invalid-assignment] "Invalid assignment to key "surname" with declared type `str` on TypedDict `Person`: value of type `Literal[1]`"
    person[union_of_keys] = 1

def _(being: Person | Animal):
    being["name"] = "Being"

    # error: [invalid-assignment] "Invalid assignment to key "name" with declared type `str` on TypedDict `Person`: value of type `Literal[1]`"
    # error: [invalid-assignment] "Invalid assignment to key "name" with declared type `str` on TypedDict `Animal`: value of type `Literal[1]`"
    being["name"] = 1

    # error: [invalid-key] "Unknown key "surname" for TypedDict `Animal` - did you mean "name"?"
    being["surname"] = "unknown"

def _(centaur: Intersection[Person, Animal]):
    centaur["name"] = "Chiron"
    centaur["age"] = 100
    centaur["legs"] = 4

    # error: [invalid-key] "Unknown key "unknown" for TypedDict `Person`"
    centaur["unknown"] = "value"

def _(person: Person, union_of_keys: Literal["name", "age"], unknown_value: Any):
    person[union_of_keys] = unknown_value

    # error: [invalid-assignment] "Invalid assignment to key "name" with declared type `str` on TypedDict `Person`: value of type `None`"
    person[union_of_keys] = None

def _(person: Person, str_key: str, literalstr_key: LiteralString):
    # error: [invalid-key] "TypedDict `Person` can only be subscripted with a string literal key, got key of type `str`."
    person[str_key] = None

    # error: [invalid-key] "TypedDict `Person` can only be subscripted with a string literal key, got key of type `LiteralString`."
    person[literalstr_key] = None

def _(person: Person, unknown_key: Any):
    # No error here:
    person[unknown_key] = "Eve"
```

## `ReadOnly`

Assignments to keys that are marked `ReadOnly` will produce an error:

```py
from typing_extensions import TypedDict, ReadOnly, Required

class Person(TypedDict, total=False):
    id: ReadOnly[Required[int]]
    name: str
    age: int | None

alice: Person = {"id": 1, "name": "Alice", "age": 30}
alice["age"] = 31  # okay

# error: [invalid-assignment] "Cannot assign to key "id" on TypedDict `Person`: key is marked read-only"
alice["id"] = 2
```

This also works if all fields on a `TypedDict` are `ReadOnly`, in which case we synthesize a
`__setitem__` method with a `key` type of `Never`:

```py
class Config(TypedDict):
    host: ReadOnly[str]
    port: ReadOnly[int]

config: Config = {"host": "localhost", "port": 8080}

# error: [invalid-assignment] "Cannot assign to key "host" on TypedDict `Config`: key is marked read-only"
config["host"] = "127.0.0.1"
# error: [invalid-assignment] "Cannot assign to key "port" on TypedDict `Config`: key is marked read-only"
config["port"] = 80
```

## Methods on `TypedDict`

```py
from typing import TypedDict
from typing_extensions import NotRequired

class Person(TypedDict):
    name: str
    age: int | None
    extra: NotRequired[str]

def _(p: Person) -> None:
    reveal_type(p.keys())  # revealed: dict_keys[str, object]
    reveal_type(p.values())  # revealed: dict_values[str, object]

    # `get()` returns the field type for required keys (no None union)
    reveal_type(p.get("name"))  # revealed: str
    reveal_type(p.get("age"))  # revealed: int | None

    # It doesn't matter if a default is specified:
    reveal_type(p.get("name", "default"))  # revealed: str
    reveal_type(p.get("age", 999))  # revealed: int | None

    # `get()` can return `None` for non-required keys
    reveal_type(p.get("extra"))  # revealed: str | None
    reveal_type(p.get("extra", "default"))  # revealed: str

    # The type of the default parameter can be anything:
    reveal_type(p.get("extra", 0))  # revealed: str | Literal[0]

    # We allow access to unknown keys (they could be set for a subtype of Person)
    reveal_type(p.get("unknown"))  # revealed: Unknown | None
    reveal_type(p.get("unknown", "default"))  # revealed: Unknown | Literal["default"]

    # `pop()` only works on non-required fields
    reveal_type(p.pop("extra"))  # revealed: str
    reveal_type(p.pop("extra", "fallback"))  # revealed: str
    # error: [invalid-argument-type] "Cannot pop required field 'name' from TypedDict `Person`"
    reveal_type(p.pop("name"))  # revealed: Unknown

    # Similar to above, the default parameter can be of any type:
    reveal_type(p.pop("extra", 0))  # revealed: str | Literal[0]

    # `setdefault()` always returns the field type
    reveal_type(p.setdefault("name", "Alice"))  # revealed: str
    reveal_type(p.setdefault("extra", "default"))  # revealed: str

    # error: [invalid-key] "Unknown key "extraz" for TypedDict `Person` - did you mean "extra"?"
    reveal_type(p.setdefault("extraz", "value"))  # revealed: Unknown
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

Also, the "attributes" on the class definition cannot be accessed. Neither on the class itself, nor
on inhabitants of the type defined by the class:

```py
# error: [unresolved-attribute] "Class `Person` has no attribute `name`"
Person.name

def _(P: type[Person]):
    # error: [unresolved-attribute] "Object of type `type[Person]` has no attribute `name`"
    P.name

def _(p: Person) -> None:
    # error: [unresolved-attribute] "Object of type `Person` has no attribute `name`"
    p.name

    type(p).name  # error: [unresolved-attribute] "Class `dict[str, object]` has no attribute `name`"
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

These attributes cannot be accessed on inhabitants:

```py
def _(person: Person) -> None:
    person.__total__  # error: [unresolved-attribute]
    person.__required_keys__  # error: [unresolved-attribute]
    person.__optional_keys__  # error: [unresolved-attribute]
```

Also, they cannot be accessed on `type(person)`, as that would be `dict` at runtime:

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

# error: [missing-typed-dict-key] "Missing required key 'employee_id' in TypedDict `Employee` constructor"
eve: Employee = {"name": "Eve"}

def combine(p: Person, e: Employee):
    reveal_type(p.copy())  # revealed: Person
    reveal_type(e.copy())  # revealed: Employee

    reveal_type(p | p)  # revealed: Person
    reveal_type(e | e)  # revealed: Employee

    # TODO: Should be `Person`; simplifying TypedDicts in Unions is pending better cycle handling
    reveal_type(p | e)  # revealed: Person | Employee
```

When inheriting from a `TypedDict` with a different `total` setting, inherited fields maintain their
original requirement status, while new fields follow the child class's `total` setting:

```py
from typing import TypedDict

# Case 1: total=True parent, total=False child
class PersonBase(TypedDict):
    id: int  # required (from total=True)
    name: str  # required (from total=True)

class PersonOptional(PersonBase, total=False):
    age: int  # optional (from total=False)
    email: str  # optional (from total=False)

# Inherited fields keep their original requirement status
person1 = PersonOptional(id=1, name="Alice")  # Valid - id/name still required
person2 = PersonOptional(id=1, name="Alice", age=25)  # Valid - age optional
person3 = PersonOptional(id=1, name="Alice", email="alice@test.com")  # Valid

# These should be errors - missing required inherited fields
# error: [missing-typed-dict-key] "Missing required key 'id' in TypedDict `PersonOptional` constructor"
person_invalid1 = PersonOptional(name="Bob")

# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `PersonOptional` constructor"
person_invalid2 = PersonOptional(id=2)

# Case 2: total=False parent, total=True child
class PersonBaseOptional(TypedDict, total=False):
    id: int  # optional (from total=False)
    name: str  # optional (from total=False)

class PersonRequired(PersonBaseOptional):  # total=True by default
    age: int  # required (from total=True)

# New fields in child are required, inherited fields stay optional
person4 = PersonRequired(age=30)  # Valid - only age required, id/name optional
person5 = PersonRequired(id=1, name="Charlie", age=35)  # Valid - all provided

# This should be an error - missing required new field
# error: [missing-typed-dict-key] "Missing required key 'age' in TypedDict `PersonRequired` constructor"
person_invalid3 = PersonRequired(id=3, name="David")
```

This also works with `Required` and `NotRequired`:

```py
from typing_extensions import TypedDict, Required, NotRequired

# Case 3: Mixed inheritance with Required/NotRequired
class PersonMixed(TypedDict, total=False):
    id: Required[int]  # required despite total=False
    name: str  # optional due to total=False

class Employee(PersonMixed):  # total=True by default
    department: str  # required due to total=True

# id stays required (Required override), name stays optional, department is required
emp1 = Employee(id=1, department="Engineering")  # Valid
emp2 = Employee(id=2, name="Eve", department="Sales")  # Valid

# Errors for missing required keys
# error: [missing-typed-dict-key] "Missing required key 'id' in TypedDict `Employee` constructor"
emp_invalid1 = Employee(department="HR")

# error: [missing-typed-dict-key] "Missing required key 'department' in TypedDict `Employee` constructor"
emp_invalid2 = Employee(id=3)
```

## Generic `TypedDict`

`TypedDict`s can also be generic.

### Legacy generics

```py
from typing import Generic, TypeVar, TypedDict, Any
from ty_extensions import static_assert, is_assignable_to, is_subtype_of

T = TypeVar("T")

class TaggedData(TypedDict, Generic[T]):
    data: T
    tag: str

p1: TaggedData[int] = {"data": 42, "tag": "number"}
p2: TaggedData[str] = {"data": "Hello", "tag": "text"}

# error: [invalid-argument-type] "Invalid argument to key "data" with declared type `int` on TypedDict `TaggedData[int]`: value of type `Literal["not a number"]`"
p3: TaggedData[int] = {"data": "not a number", "tag": "number"}

class Items(TypedDict, Generic[T]):
    items: list[T]

def homogeneous_list(*args: T) -> list[T]:
    return list(args)

items1: Items[int] = {"items": [1, 2, 3]}
items2: Items[str] = {"items": ["a", "b", "c"]}
items3: Items[int] = {"items": homogeneous_list(1, 2, 3)}
items4: Items[str] = {"items": homogeneous_list("a", "b", "c")}
items5: Items[int | str] = {"items": homogeneous_list(1, 2, 3)}

# structural assignability
static_assert(is_assignable_to(Items[int], Items[int]))
static_assert(is_subtype_of(Items[int], Items[int]))
static_assert(not is_assignable_to(Items[str], Items[int]))
static_assert(not is_subtype_of(Items[str], Items[int]))
static_assert(is_assignable_to(Items[Any], Items[int]))
static_assert(not is_subtype_of(Items[Any], Items[int]))
```

### PEP-695 generics

```toml
[environment]
python-version = "3.12"
```

```py
from typing import TypedDict, Any
from ty_extensions import static_assert, is_assignable_to, is_subtype_of

class TaggedData[T](TypedDict):
    data: T
    tag: str

p1: TaggedData[int] = {"data": 42, "tag": "number"}
p2: TaggedData[str] = {"data": "Hello", "tag": "text"}

# error: [invalid-argument-type] "Invalid argument to key "data" with declared type `int` on TypedDict `TaggedData[int]`: value of type `Literal["not a number"]`"
p3: TaggedData[int] = {"data": "not a number", "tag": "number"}

class Items[T](TypedDict):
    items: list[T]

def homogeneous_list[T](*args: T) -> list[T]:
    return list(args)

items1: Items[int] = {"items": [1, 2, 3]}
items2: Items[str] = {"items": ["a", "b", "c"]}
items3: Items[int] = {"items": homogeneous_list(1, 2, 3)}
items4: Items[str] = {"items": homogeneous_list("a", "b", "c")}
items5: Items[int | str] = {"items": homogeneous_list(1, 2, 3)}

# structural assignability
static_assert(is_assignable_to(Items[int], Items[int]))
static_assert(is_subtype_of(Items[int], Items[int]))
static_assert(not is_assignable_to(Items[str], Items[int]))
static_assert(not is_subtype_of(Items[str], Items[int]))
static_assert(is_assignable_to(Items[Any], Items[int]))
static_assert(not is_subtype_of(Items[Any], Items[int]))
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

# error: [invalid-argument-type] "Invalid argument to key "name" with declared type `str` on TypedDict `Node`: value of type `Literal[3]`"
nested_invalid: Node = {"name": "n1", "parent": {"name": "n2", "parent": {"name": 3, "parent": None}}}
```

Structural assignment works for recursive `TypedDict`s too:

```py
class Person(TypedDict):
    name: str
    parent: Person | None

def _(node: Node, person: Person):
    _: Person = node
    _: Node = person

_: Node = Person(name="Alice", parent=Node(name="Bob", parent=Person(name="Charlie", parent=None)))
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

reveal_type(Message.__required_keys__)  # revealed: @Todo(Support for functional `TypedDict`)

# TODO: this should be an error
msg.content
```

## Error cases

### `typing.TypedDict` is not allowed in type expressions

<!-- snapshot-diagnostics -->

```py
from typing import TypedDict

# error: [invalid-type-form] "The special form `typing.TypedDict` is not allowed in type expressions"
x: TypedDict = {"name": "Alice"}
```

### `dict`-subclass inhabitants

Values that inhabit a `TypedDict` type must be instances of `dict` itself, not a subclass:

```py
from typing import TypedDict

class MyDict(dict):
    pass

class Person(TypedDict):
    name: str
    age: int | None

# error: [invalid-assignment] "Object of type `MyDict` is not assignable to `Person`"
x: Person = MyDict({"name": "Alice", "age": 30})
```

### Cannot be used in `isinstance` tests

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

def _(obj: object) -> bool:
    # TODO: this should be an error
    return isinstance(obj, Person)
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

def create_with_invalid_string_key():
    # error: [invalid-key]
    alice: Person = {"name": "Alice", "age": 30, "unknown": "Foo"}

    # error: [invalid-key]
    bob = Person(name="Bob", age=25, unknown="Bar")
```

Assignment to `ReadOnly` keys:

```py
from typing_extensions import ReadOnly

class Employee(TypedDict):
    id: ReadOnly[int]
    name: str

def write_to_readonly_key(employee: Employee):
    employee["id"] = 42  # error: [invalid-assignment]
```

If the key uses single quotes, the autofix preserves that quoting style:

```py
def write_to_non_existing_key_single_quotes(person: Person):
    # error: [invalid-key]
    person['naem'] = "Alice"  # fmt: skip
```

## Import aliases

`TypedDict` can be imported with aliases and should work correctly:

```py
from typing import TypedDict as TD
from typing_extensions import Required

class UserWithAlias(TD, total=False):
    name: Required[str]
    age: int

user_empty = UserWithAlias(name="Alice")  # name is required
user_partial = UserWithAlias(name="Alice", age=30)

# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `UserWithAlias` constructor"
user_invalid = UserWithAlias(age=30)

reveal_type(user_empty["name"])  # revealed: str
reveal_type(user_partial["age"])  # revealed: int
```

## Shadowing behavior

When a local class shadows the `TypedDict` import, only the actual `TypedDict` import should be
treated as a `TypedDict`:

```py
from typing import TypedDict as TD

class TypedDict:
    def __init__(self):
        pass

class NotActualTypedDict(TypedDict, total=True):
    name: str

class ActualTypedDict(TD, total=True):
    name: str

not_td = NotActualTypedDict()
reveal_type(not_td)  # revealed: NotActualTypedDict

# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `ActualTypedDict` constructor"
actual_td = ActualTypedDict()
actual_td = ActualTypedDict(name="Alice")
reveal_type(actual_td)  # revealed: ActualTypedDict
reveal_type(actual_td["name"])  # revealed: str
```

## Disjointness with other `TypedDict`s

Two `TypedDict` types are disjoint if it's impossible to come up with a third (fully-static)
`TypedDict` that's assignable to both. The simplest way to establish this is if both sides have
fields with the same name but disjoint types:

```py
from typing import TypedDict, final
from typing_extensions import ReadOnly
from ty_extensions import static_assert, is_disjoint_from

# Two simple disjoint types, to avoid relying on `@disjoint_base` special cases for built-ins like
# `int` and `str`.
@final
class Final1: ...

@final
class Final2: ...

static_assert(is_disjoint_from(Final1, Final2))

class DisjointTD1(TypedDict):
    # Make this example `ReadOnly` because that actually ends up checking the field types for
    # disjointness in practice. Mutable fields are stricter. We'll get to that below.
    disjoint: ReadOnly[Final1]
    # While we're here: It doesn't matter how many other compatible fields there are. Just the one
    # incompatible field above establishes disjointness.
    common1: object
    common2: object

class DisjointTD2(TypedDict):
    disjoint: ReadOnly[Final2]
    common1: object
    common2: object

static_assert(is_disjoint_from(DisjointTD1, DisjointTD2))
```

However, note that most pairs of non-final classes are *not* disjoint from each other, even if
neither inherits from the other, because we could define a third class that multiply-inherits from
both. `TypedDict` disjointness takes this into account. For example:

```py
from ty_extensions import is_assignable_to

class NonFinal1: ...
class NonFinal2: ...
class CommonSub(NonFinal1, NonFinal2): ...

static_assert(not is_disjoint_from(NonFinal1, NonFinal2))
static_assert(not is_assignable_to(NonFinal1, NonFinal2))
static_assert(is_assignable_to(CommonSub, NonFinal1))
static_assert(is_assignable_to(CommonSub, NonFinal2))

class NonDisjointTD1(TypedDict):
    non_disjoint: ReadOnly[NonFinal1]
    # While we're here: It doesn't matter how many "extra" fields there are, or what order the
    # fields are in. Only shared field names can establish disjointness.
    extra1: int

class NonDisjointTD2(TypedDict):
    extra2: str
    non_disjoint: ReadOnly[NonFinal2]

class CommonSubTD(TypedDict):
    extra2: str
    extra1: int
    non_disjoint: ReadOnly[CommonSub]

# The first two TDs above are not assignable in either direction...
static_assert(not is_assignable_to(NonDisjointTD1, NonDisjointTD2))
static_assert(not is_assignable_to(NonDisjointTD2, NonDisjointTD1))
# ...but they're still not disjoint...
static_assert(not is_disjoint_from(NonDisjointTD1, NonDisjointTD2))
# ...because the third TD above is assignable to both of them.
static_assert(is_assignable_to(CommonSubTD, NonDisjointTD1))
static_assert(is_assignable_to(CommonSubTD, NonDisjointTD2))
static_assert(not is_disjoint_from(CommonSubTD, NonDisjointTD1))
static_assert(not is_disjoint_from(CommonSubTD, NonDisjointTD2))
```

We made the important fields `ReadOnly` above, because those only establish disjointness when
they're disjoint themselves. However, the rules for mutable fields are stricter. Mutable fields in
common need to have *compatible* types (in the fully-static case, equivalent types):

```py
from typing import Any, Generic, TypeVar

class IntTD(TypedDict):
    x: int

class BoolTD(TypedDict):
    x: bool

# `bool` is assignable to `int`, but `int` is not assignable to `bool`. If `x` was `ReadOnly` (even,
# as we'll see below, only on the `int` side), then these two TDs would not be disjoint, but in this
# mutable case they are.

static_assert(is_disjoint_from(IntTD, BoolTD))
static_assert(is_disjoint_from(BoolTD, IntTD))

# Gradual types: `int` is compatible with `bool | Any`, because that could materialize to
# `bool | int`, which is just `int`. (And `int | Any` and `bool | Any` are compatible with each
# other for the same reason.) However, `bool` is *not* compatible with `int | Any`, because there's
# no materialization that's equivalent to `bool`.

class IntOrAnyTD(TypedDict):
    x: int | Any

class BoolOrAnyTD(TypedDict):
    x: bool | Any

static_assert(not is_disjoint_from(IntTD, IntOrAnyTD))
static_assert(not is_disjoint_from(IntOrAnyTD, IntTD))
static_assert(not is_disjoint_from(IntTD, BoolOrAnyTD))
static_assert(not is_disjoint_from(BoolOrAnyTD, IntTD))

static_assert(not is_disjoint_from(IntOrAnyTD, BoolOrAnyTD))
static_assert(not is_disjoint_from(BoolOrAnyTD, IntOrAnyTD))

static_assert(is_disjoint_from(BoolTD, IntOrAnyTD))
static_assert(is_disjoint_from(IntOrAnyTD, BoolTD))
static_assert(not is_disjoint_from(BoolTD, BoolOrAnyTD))
static_assert(not is_disjoint_from(BoolOrAnyTD, BoolTD))

# `Any` is compatible with everything.

class AnyTD(TypedDict):
    x: Any

static_assert(not is_disjoint_from(IntTD, AnyTD))
static_assert(not is_disjoint_from(AnyTD, IntTD))
static_assert(not is_disjoint_from(BoolTD, AnyTD))
static_assert(not is_disjoint_from(AnyTD, BoolTD))
static_assert(not is_disjoint_from(IntOrAnyTD, AnyTD))
static_assert(not is_disjoint_from(AnyTD, IntOrAnyTD))
static_assert(not is_disjoint_from(BoolOrAnyTD, AnyTD))
static_assert(not is_disjoint_from(AnyTD, BoolOrAnyTD))
static_assert(not is_disjoint_from(AnyTD, AnyTD))

# This works with generic `TypedDict`s too.

class TwoIntsTD(TypedDict):
    x: int
    y: int

class TwoBoolsTD(TypedDict):
    x: bool
    y: bool

class IntBoolTD(TypedDict):
    x: int
    y: bool

T = TypeVar("T")

class TwoGenericTD(TypedDict, Generic[T]):
    x: T
    y: T

static_assert(not is_disjoint_from(TwoGenericTD[Any], TwoIntsTD))
static_assert(not is_disjoint_from(TwoGenericTD[int], TwoIntsTD))
static_assert(is_disjoint_from(TwoGenericTD[bool], TwoIntsTD))
static_assert(not is_disjoint_from(TwoGenericTD[Any], TwoBoolsTD))
static_assert(is_disjoint_from(TwoGenericTD[int], TwoBoolsTD))
static_assert(not is_disjoint_from(TwoGenericTD[bool], TwoBoolsTD))
# TODO: T can't be compatible with both `int` and `bool` at the same time, so these types should be
# disjoint, regardless of the materialization of `T`.
static_assert(not is_disjoint_from(TwoGenericTD[Any], IntBoolTD))
```

If one side is mutable but the other is not, then a "third `TypedDict` that's assignable to both"
would have to have the same type as the mutable side, so we establish disjointness if that type
isn't assignable to the immutable side:

```py
class ReadOnlyIntTD(TypedDict):
    x: ReadOnly[int]

class ReadOnlyBoolTD(TypedDict):
    x: ReadOnly[bool]

static_assert(not is_disjoint_from(ReadOnlyIntTD, ReadOnlyBoolTD))
static_assert(not is_disjoint_from(ReadOnlyBoolTD, ReadOnlyIntTD))
static_assert(not is_disjoint_from(BoolTD, ReadOnlyIntTD))
static_assert(not is_disjoint_from(ReadOnlyIntTD, BoolTD))
static_assert(is_disjoint_from(IntTD, ReadOnlyBoolTD))
static_assert(is_disjoint_from(ReadOnlyBoolTD, IntTD))
```

With mutability above we were able to make the simplifying assumption that the "third `TypedDict`
that's assignable to both" has only mutable fields, because a mutable field is always assignable to
its immutable counterpart. However, `Required` vs `NotRequired` are more complicated, because a a
`Required` field is *not* necessarily assignable to its `NotRequired` counterpart. In particular, if
a `NotRequired` field is also mutable (intuitively, if we're allowed to `del` it), then no
`Required` field is ever assignable to it. So, if either side is `NotRequired` and mutable, and the
other side is `Required` (regardless of mutability), then that's sufficient to establish
disjointness:

```py
from typing_extensions import NotRequired

class NotRequiredIntTD(TypedDict):
    x: NotRequired[int]

class NotRequiredReadOnlyIntTD(TypedDict):
    x: NotRequired[ReadOnly[int]]

static_assert(is_disjoint_from(NotRequiredIntTD, IntTD))
static_assert(is_disjoint_from(IntTD, NotRequiredIntTD))
static_assert(is_disjoint_from(NotRequiredIntTD, ReadOnlyIntTD))
static_assert(is_disjoint_from(ReadOnlyIntTD, NotRequiredIntTD))
static_assert(not is_disjoint_from(NotRequiredIntTD, NotRequiredReadOnlyIntTD))
static_assert(not is_disjoint_from(NotRequiredReadOnlyIntTD, NotRequiredIntTD))
```

All those rules put together give us the "full disjointness table". We've pretty well tested above
that disjointness is symmetrical, so here we won't worry about asserting both directions for each
check:

```py
class NotRequiredBoolTD(TypedDict):
    x: NotRequired[bool]

class NotRequiredReadOnlyBoolTD(TypedDict):
    x: NotRequired[ReadOnly[bool]]

static_assert(not is_disjoint_from(IntTD, IntTD))
static_assert(is_disjoint_from(IntTD, BoolTD))
static_assert(not is_disjoint_from(IntTD, ReadOnlyIntTD))
static_assert(is_disjoint_from(IntTD, ReadOnlyBoolTD))
static_assert(is_disjoint_from(IntTD, NotRequiredIntTD))
static_assert(is_disjoint_from(IntTD, NotRequiredBoolTD))
static_assert(not is_disjoint_from(IntTD, NotRequiredReadOnlyIntTD))
static_assert(is_disjoint_from(IntTD, NotRequiredReadOnlyBoolTD))
static_assert(not is_disjoint_from(ReadOnlyIntTD, BoolTD))
static_assert(not is_disjoint_from(ReadOnlyIntTD, ReadOnlyIntTD))
static_assert(not is_disjoint_from(ReadOnlyIntTD, ReadOnlyBoolTD))
static_assert(is_disjoint_from(ReadOnlyIntTD, NotRequiredIntTD))
static_assert(is_disjoint_from(ReadOnlyIntTD, NotRequiredBoolTD))
static_assert(not is_disjoint_from(ReadOnlyIntTD, NotRequiredReadOnlyIntTD))
static_assert(not is_disjoint_from(ReadOnlyIntTD, NotRequiredReadOnlyBoolTD))
static_assert(is_disjoint_from(NotRequiredIntTD, BoolTD))
static_assert(is_disjoint_from(NotRequiredIntTD, ReadOnlyBoolTD))
static_assert(not is_disjoint_from(NotRequiredIntTD, NotRequiredIntTD))
static_assert(is_disjoint_from(NotRequiredIntTD, NotRequiredBoolTD))
static_assert(not is_disjoint_from(NotRequiredIntTD, NotRequiredReadOnlyIntTD))
static_assert(is_disjoint_from(NotRequiredIntTD, NotRequiredReadOnlyBoolTD))
static_assert(not is_disjoint_from(NotRequiredReadOnlyIntTD, BoolTD))
static_assert(not is_disjoint_from(NotRequiredReadOnlyIntTD, ReadOnlyBoolTD))
static_assert(not is_disjoint_from(NotRequiredReadOnlyIntTD, NotRequiredBoolTD))
static_assert(not is_disjoint_from(NotRequiredReadOnlyIntTD, NotRequiredReadOnlyIntTD))
static_assert(not is_disjoint_from(NotRequiredReadOnlyIntTD, NotRequiredReadOnlyBoolTD))
static_assert(not is_disjoint_from(BoolTD, BoolTD))
static_assert(not is_disjoint_from(BoolTD, ReadOnlyBoolTD))
static_assert(is_disjoint_from(BoolTD, NotRequiredBoolTD))
static_assert(not is_disjoint_from(BoolTD, NotRequiredReadOnlyBoolTD))
static_assert(not is_disjoint_from(ReadOnlyBoolTD, ReadOnlyBoolTD))
static_assert(is_disjoint_from(ReadOnlyBoolTD, NotRequiredBoolTD))
static_assert(not is_disjoint_from(ReadOnlyBoolTD, NotRequiredReadOnlyBoolTD))
static_assert(not is_disjoint_from(NotRequiredBoolTD, NotRequiredBoolTD))
static_assert(not is_disjoint_from(NotRequiredBoolTD, NotRequiredReadOnlyBoolTD))
static_assert(not is_disjoint_from(NotRequiredReadOnlyBoolTD, NotRequiredReadOnlyBoolTD))
```

## Disjointness with other types

```py
from typing import TypedDict, Mapping
from ty_extensions import static_assert, is_disjoint_from

class TD(TypedDict):
    x: int

class RegularNonTD: ...

static_assert(not is_disjoint_from(TD, object))
static_assert(not is_disjoint_from(TD, Mapping[str, object]))
static_assert(is_disjoint_from(TD, Mapping[int, object]))
static_assert(is_disjoint_from(TD, RegularNonTD))

# TODO: We approximate disjointness with other types `T` by asking whether `dict[str, Any]` is
# assignable to `T`. That covers common cases like the ones above, but does it have some false
# negatives with `dict` types. A `TypedDict` is almost never assignable to a `dict` (or vice versa),
# even when all of the `TypedDict`'s field types match the `dict`'s value type (and are mutable).
# The problem is that the `TypedDict` could have been assigned to from *another* `TypedDict` with
# additional fields, and we don't usually know anything about the types or mutability of those. On
# the other hand, the assignment to `dict` can be allowed if the `TypedDict` has mutable
# `extra_items` of a compatible type. See: https://typing.python.org/en/latest/spec/typeddict.html#subtyping-with-dict
static_assert(is_disjoint_from(TD, dict[str, int]))  # error: [static-assert-error]
static_assert(is_disjoint_from(TD, dict[str, str]))  # error: [static-assert-error]
```

## Narrowing tagged unions of `TypedDict`s

In a tagged union of `TypedDict`s, a common field in each member (often `"type"` or `"tag"`) is
given a distinct `Literal` type/value. We can narrow the union by constraining this field:

```py
from typing import TypedDict, Literal

class Foo(TypedDict):
    tag: Literal["foo"]

class Bar(TypedDict):
    tag: Literal[42]

class Baz(TypedDict):
    tag: Literal[b"baz"]  # `BytesLiteral` is supported.

class Bing(TypedDict):
    tag: Literal["bing"]

def _(u: Foo | Bar | Baz | Bing):
    if u["tag"] == "foo":
        reveal_type(u)  # revealed: Foo
    elif u["tag"] == 42:
        reveal_type(u)  # revealed: Bar
    elif u["tag"] == b"baz":
        reveal_type(u)  # revealed: Baz
    else:
        reveal_type(u)  # revealed: Bing
```

We can descend into intersections to discover `TypedDict` types that need narrowing:

```py
from collections.abc import Mapping
from ty_extensions import Intersection

def _(u: Foo | Intersection[Bar, Mapping[str, int]]):
    if u["tag"] == "foo":
        reveal_type(u)  # revealed: Foo
    else:
        reveal_type(u)  # revealed: Bar & Mapping[str, int]
```

We can also narrow a single `TypedDict` type to `Never`:

```py
def _(u: Foo):
    if u["tag"] == "foo":
        reveal_type(u)  # revealed: Foo
    else:
        reveal_type(u)  # revealed: Never
```

Narrowing is restricted to `Literal` tags, though, because `x == "foo"` doesn't generally tell us
anything about the type of `x`. Here's an example where narrowing would be tempting but unsound:

```py
from ty_extensions import is_assignable_to, static_assert

class NonLiteralTD(TypedDict):
    tag: int

def _(u: Foo | NonLiteralTD):
    if u["tag"] == "foo":
        # We can't narrow the union here...
        reveal_type(u)  # revealed: Foo | NonLiteralTD
    else:
        # ...(even though we can here)...
        reveal_type(u)  # revealed: NonLiteralTD

# ...because `NonLiteralTD["tag"]` could be assigned to with one of these, which would make the
# first condition above true at runtime!
class WackyInt(int):
    def __eq__(self, other):
        return True

_: NonLiteralTD = {"tag": WackyInt(99)}  # allowed
```

We can still narrow `Literal` tags even when non-`TypedDict` types are present in the union:

```py
def _(u: Foo | Bar | dict):
    if u["tag"] == "foo":
        # TODO: `dict & ~<TypedDict ...>` should simplify to `dict` here, but that's currently a
        # false negative in `is_disjoint_impl`.
        reveal_type(u)  # revealed: Foo | (dict[Unknown, Unknown] & ~<TypedDict with items 'tag'>)

# The negation(s) will simplify out if we add something to the union that doesn't inherit from
# `dict`. It just needs to support indexing with a string key.
class NotADict:
    def __getitem__(self, key): ...

def _(u: Foo | Bar | NotADict):
    if u["tag"] == 42:
        reveal_type(u)  # revealed: Bar | NotADict
```

It would be nice if we could also narrow `TypedDict` unions by checking whether a key (which only
shows up in a subset of the union members) is present, but that isn't generally correct, because
"extra items" are allowed by default. For example, even though `Bar` here doesn't define a `"foo"`
field, it could be *assigned to* with another `TypedDict` that does:

```py
class Foo(TypedDict):
    foo: int

class Bar(TypedDict):
    bar: int

def disappointment(u: Foo | Bar):
    if "foo" in u:
        # We can't narrow the union here...
        reveal_type(u)  # revealed: Foo | Bar
    else:
        # ...(even though we *can* narrow it here)...
        # TODO: This should narrow to `Bar`, because "foo" is required in `Foo`.
        reveal_type(u)  # revealed: Foo | Bar

# ...because `u` could turn out to be one of these.
class FooBar(TypedDict):
    foo: int
    bar: int

static_assert(is_assignable_to(FooBar, Foo))
static_assert(is_assignable_to(FooBar, Bar))
```

TODO: The narrowing that we didn't do above will become possible when we add support for
`closed=True`. This is [one of the main use cases][closed] that motivated the `closed` feature.

[closed]: https://peps.python.org/pep-0728/#disallowing-extra-items-explicitly
[subtyping section]: https://typing.python.org/en/latest/spec/typeddict.html#subtyping-between-typeddict-types
[`typeddict`]: https://typing.python.org/en/latest/spec/typeddict.html
