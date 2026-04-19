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

reveal_type(alice["name"])  # revealed: Literal["Alice"]
reveal_type(alice["age"])  # revealed: Literal[30]

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

Functional `TypedDict`s with non-identifier keys should synthesize `__init__` without turning those
keys into invalid named parameters:

```py
from typing import TypedDict

Config = TypedDict("Config", {"in": int, "x-y": str, "ok": int})
# revealed: Overload[(self: Config, map: Config, /, *, ok: int = ..., **kwargs) -> None, (self: Config, /, *, ok: int, **kwargs) -> None]
reveal_type(Config.__init__)
```

If a dict literal is inferred against a union containing both a `TypedDict` and a plain `dict`,
extra keys accepted by the non-`TypedDict` arm should not trigger eager `TypedDict` diagnostics:

```py
from typing import Any, TypedDict

class FormatterConfig(TypedDict, total=False):
    format: str

def takes_formatter(config: FormatterConfig | dict[str, Any]) -> None: ...

takes_formatter({"format": "%(message)s"})
takes_formatter({"factory": object(), "facility": "local0"})
```

Methods that are available on `dict`s are also available on `TypedDict`s:

```py
bob.update(age=26)
bob.update({"age": 27})

class NamePatch(TypedDict, total=False):
    name: str

name_update: NamePatch = {"name": "Bobby"}
string_key_updates: list[tuple[str, str]] = [("name", "Bobby")]
bad_key_updates: list[tuple[int, str]] = [(1, "Bobby")]

bob.update(name_update)
bob.update({"name": "Robert"})
bob.update([("name", "Bobby")])
bob.update([("age", 27)])
bob.update(name_update, age=26)
bob.update([("name", "Bobby")], age=26)

# error: [invalid-argument-type]
bob.update(age="bad")

# error: [unknown-argument]
bob.update(other=1)

# error: [invalid-argument-type]
bob.update(name_update, age="bad")

# error: [unknown-argument]
bob.update(name_update, other=1)

# error: [invalid-argument-type]
# error: [invalid-key]
bob.update({"other": 1})

# error: [invalid-argument-type]
# error: [invalid-argument-type]
bob.update({"age": "bad"})

bob.update([("other", 1)])

bob.update([("age", "bad")])

bob.update(string_key_updates)

# error: [invalid-argument-type]
bob.update(bad_key_updates)

Require = TypedDict(
    "Require",
    {"source-path": str, "compiled-module-path": str},
    total=False,
)

requirement: Require = {}
requirement.update({"source-path": "src", "compiled-module-path": "build"})
```

`update()` treats the patch operand as partial even when the target `TypedDict` uses `Required` and
`NotRequired`:

```py
from typing_extensions import NotRequired, Required

class Movie(TypedDict, total=False):
    title: Required[str]
    year: int
    director: NotRequired[str]

class MissingRequiredTitle(TypedDict, total=False):
    year: int

movie: Movie = {"title": "Alien"}
missing_required_title: MissingRequiredTitle = {"year": 1986}

movie.update(year=1986)
movie.update(director="Cameron")
movie.update({"title": "Aliens"})
movie.update({"director": "Cameron"})
movie.update(missing_required_title)

# error: [invalid-argument-type]
movie.update(title=1986)

# error: [invalid-argument-type]
# error: [invalid-argument-type]
movie.update({"director": 1986})
```

PEP 584-style immutable updates preserve the `TypedDict` type when the other operand is compatible:

```py
reveal_type(bob | {"age": 27})  # revealed: Person
reveal_type({"age": 27} | bob)  # revealed: Person

carol_update = Person(name="Carol", age=31)
reveal_type(bob | carol_update)  # revealed: Person
```

Compatible `TypedDict` subset updates are also accepted for `|=`:

```py
class NameOnly(TypedDict, closed=True):
    name: str

name_update: NameOnly = {"name": "Bobby"}

bob |= {"age": 27}
bob |= name_update
```

TODO: protocol matching for synthesized `TypedDict.__or__` should also accept these cases:

```py
from typing import Callable, Protocol

class PersonOrNameOnly(Protocol):
    def __or__(self, other: NameOnly) -> Person: ...

class PersonOrNameOnlyAttr(Protocol):
    __or__: Callable[[NameOnly], Person]

def takes_person_or_name_only(x: PersonOrNameOnly) -> None: ...
def takes_person_or_name_only_attr(x: PersonOrNameOnlyAttr) -> None: ...

# TODO: this should pass
# error: [invalid-argument-type] "Argument to function `takes_person_or_name_only` is incorrect: Expected `PersonOrNameOnly`, found `Person`"
takes_person_or_name_only(bob)
# TODO: this should pass
# error: [invalid-argument-type] "Argument to function `takes_person_or_name_only_attr` is incorrect: Expected `PersonOrNameOnlyAttr`, found `Person`"
takes_person_or_name_only_attr(bob)
```

When the other operand is not compatible with the `TypedDict`, the result falls back to the normal
`dict.__or__` return type:

```py
# Incompatible value type for a key
reveal_type(bob | {"name": 42})  # revealed: dict[str, object]
reveal_type({"name": 42} | bob)  # revealed: dict[str, object]

# Key not present in the TypedDict
reveal_type(bob | {"unknown_key": 1})  # revealed: dict[str, object]
reveal_type({"unknown_key": 1} | bob)  # revealed: dict[str, object]

# error: [unsupported-operator] "Operator `|=` is not supported between objects of type `Person` and `dict[str, int]`"
bob |= {"unknown_key": 1}
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

Constructor calls with multiple positional arguments should be rejected, including for empty
`TypedDict`s:

```py
class Empty(TypedDict):
    pass

# error: [too-many-positional-arguments] "Too many positional arguments to TypedDict `Empty` constructor: expected 1, got 2"
Empty({}, {})

# error: [too-many-positional-arguments] "Too many positional arguments to TypedDict `Person` constructor: expected 1, got 2"
Person({}, {})
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
reveal_type(plot3["x"])  # revealed: list[int | None]

plot3["y"] = homogeneous_list(1, 2, 3)
reveal_type(plot3["y"])  # revealed: list[int | None]

reveal_type(plot1 | {"y": homogeneous_list(1, 2, 3)})  # revealed: Plot
reveal_type({"y": homogeneous_list(1, 2, 3)} | plot1)  # revealed: Plot

Y = "y"
X = "x"

plot4: Plot = {Y: [1, 2, 3], X: None}
plot5: Plot = {Y: homogeneous_list(1, 2, 3), X: None}

reveal_type(plot1 | {Y: homogeneous_list(1, 2, 3)})  # revealed: Plot
reveal_type({Y: homogeneous_list(1, 2, 3)} | plot1)  # revealed: Plot

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

reveal_type(alice["inner"]["name"])  # revealed: Literal["Alice"]
reveal_type(alice["inner"]["age"])  # revealed: Literal[30]

# error: [invalid-key] "Unknown key "non_existing" for TypedDict `Inner`"
reveal_type(alice["inner"]["non_existing"])  # revealed: Unknown

# error: [invalid-key] "Unknown key "extra" for TypedDict `Inner`"
alice: Person = {"inner": {"name": "Alice", "age": 30, "extra": 1}}

class Box(TypedDict):
    inner: Inner

box: Box = {"inner": {"name": "Alice", "age": 30}}
reveal_type(box | {"inner": {"age": 31, "name": "Alice"}})  # revealed: Box
reveal_type({"inner": {"age": 31, "name": "Alice"}} | box)  # revealed: Box
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

TypedDict constructor validation should not duplicate diagnostics emitted by argument inference:

```py
from typing import TypedDict

class TD(TypedDict):
    x: int

# error: [unresolved-reference] "Name `missing` used when not defined"
TD(x=missing)
```

TypedDict constructor validation should respect string-valued constants used as keys in positional
dict literals:

```py
from typing import Final, TypedDict

VALUE_KEY: Final = "value"

class Record(TypedDict):
    value: str

Record({VALUE_KEY: "x"})
```

TypedDict constructor validation should combine positional dict literals with keyword arguments:

```py
from typing import TypedDict

class TD(TypedDict):
    x: int
    y: str

# error: [invalid-argument-type] "Invalid argument to key "x" with declared type `int` on TypedDict `TD`: value of type `Literal["foo"]`"
TD({"x": "foo"}, y="bar")
```

TypedDict constructor validation should preserve string-valued constant keys in mixed calls:

```py
from typing import Final, TypedDict

VALUE_KEY: Final = "value"

class Record(TypedDict):
    value: str
    count: int

Record({VALUE_KEY: "x"}, count=1)

# error: [invalid-argument-type] "Invalid argument to key "value" with declared type `str` on TypedDict `Record`: value of type `Literal[1]`"
Record({VALUE_KEY: 1}, count=1)
```

Keyword arguments should override a positional mapping, and `TypedDict` constructor inputs should
preserve shared required keys:

```py
from typing import TypedDict

class ChildWithOptionalCount(TypedDict, total=False):
    count: int

ChildWithOptionalCount({"count": "wrong"}, count=1)

class Base(TypedDict):
    name: str

class ChildKwargs(TypedDict):
    name: str
    count: int

class MaybeName(TypedDict, total=False):
    name: str

def _(
    base: Base,
    maybe_name: MaybeName,
):
    ChildKwargs(base, count=1)
    ChildKwargs(**base, count=1)

    # error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `ChildKwargs` constructor"
    ChildKwargs(**maybe_name, count=1)
```

TypedDict positional arguments in mixed constructors should validate their declared keys:

```py
from typing import TypedDict

class Target(TypedDict):
    a: int
    b: int

class Source(TypedDict):
    a: int

class BadSource(TypedDict):
    a: str

class MaybeSource(TypedDict, total=False):
    a: int

class WiderSource(TypedDict):
    a: int
    extra: str

class WiderBadSource(TypedDict):
    a: str
    extra: str

def _(
    source: Source,
    bad: BadSource,
    maybe: MaybeSource,
    wide: WiderSource,
    wide_bad: WiderBadSource,
    cond: bool,
):
    Target(source, b=2)
    Target(source if cond else {"a": 1}, b=2)
    Target(source if cond else {"a": 1, "b": 0}, b=2)
    Target(source if cond else {"a": 1, "b": "shadowed"}, b=2)
    Target(wide, b=2)

    # error: [invalid-argument-type] "Invalid argument to key "a" with declared type `int` on TypedDict `Target`: value of type `str`"
    Target(bad, b=2)

    # error: [invalid-argument-type] "Invalid argument to key "a" with declared type `int` on TypedDict `Target`: value of type `str`"
    Target(wide_bad, b=2)

    # error: [missing-typed-dict-key] "Missing required key 'a' in TypedDict `Target` constructor"
    Target(maybe, b=2)
```

Mixed constructors should stay lenient for non-`TypedDict` positional mappings once the keyword
arguments cover the full schema:

```py
from typing import TypedDict

class FullFromKeywords(TypedDict):
    a: int

def _(mapping: dict[str, str]):
    FullFromKeywords(mapping, a=1)
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

Constructor validation should also run when the call target is a generic alias or a `type[...]`
value:

```py
from typing import Generic, TypeVar, TypedDict

T = TypeVar("T")

class MyGenTD(TypedDict, Generic[T]):
    a: int
    b: T

class MyTD(TypedDict):
    a: int

MyStrTD = MyGenTD[str]

# error: [invalid-argument-type] "Invalid argument to key "a""
x = MyStrTD(a="foo", b="ok")

# No error: `a` is int, `b` is str (matches T=str)
w = MyStrTD(a=1, b="ok")

# error: [invalid-argument-type] "Invalid argument to key "b""
v = MyStrTD(a=1, b=42)

# error: [invalid-argument-type]
y = MyTD(a="foo")

def _(ATD: type[MyTD]):
    # error: [invalid-argument-type]
    z = ATD(a="foo")
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

## Mixed positional and unpacked keyword constructors

These calls mix a positional `TypedDict` argument with unpacked keyword arguments. They should
validate normally and produce ordinary diagnostics:

```py
from typing import Any, TypedDict
from typing_extensions import Never

class MixedTarget(TypedDict):
    x: int
    y: int

class MaybeY(TypedDict, total=False):
    y: int

def _(target: MixedTarget, maybe_y: MaybeY, kwargs: Any, never_kwargs: Never, cond: bool):
    MixedTarget(target, **maybe_y)
    MixedTarget(maybe_y if cond else {}, **kwargs)
    MixedTarget(maybe_y if cond else {}, **never_kwargs)

    # error: [missing-typed-dict-key] "Missing required key 'y' in TypedDict `MixedTarget` constructor"
    MixedTarget({"x": 1}, **maybe_y)

class TD(TypedDict):
    a: int

def _(td: TD):
    # TODO: this should pass like the explicit-keyword and `**TypedDict` cases below.
    # error: [invalid-argument-type] "Invalid argument to key "a" with declared type `int` on TypedDict `TD`: value of type `Literal["foo"]`"
    TD({"a": "foo"}, **{"a": 1})

    TD({"a": "foo"}, a=1)
    TD({"a": "foo"}, **td)

def _(x: Any):
    TD({"a": "foo"}, **x)
```

## Union of `TypedDict`

When assigning to a union of `TypedDict` types, the type will be narrowed based on the dictionary
literal:

```py
from typing import TypedDict
from typing_extensions import NotRequired

class Foo(TypedDict):
    foo: int

x1: Foo | None = {"foo": 1}
reveal_type(x1)  # revealed: Foo

# A union with no dict-compatible fallback should still validate eagerly against the
# TypedDict arm.
# error: [missing-typed-dict-key] "Missing required key 'foo' in TypedDict `Foo` constructor"
# error: [invalid-key] "Unknown key "bar" for TypedDict `Foo`"
x1_bad: Foo | None = {"bar": 1}
reveal_type(x1_bad)  # revealed: Foo | None

class Bar(TypedDict):
    bar: int

x2: Foo | Bar = {"foo": 1}
reveal_type(x2)  # revealed: Foo

x3: Foo | Bar = {"bar": 1}
reveal_type(x3)  # revealed: Bar

x4: Foo | Bar | None = {"bar": 1}
reveal_type(x4)  # revealed: Bar

# error: [invalid-assignment]
x5: Foo | Bar = {"baz": 1}
reveal_type(x5)  # revealed: Foo | Bar

class FooBar1(TypedDict):
    foo: int
    bar: int

class FooBar2(TypedDict):
    foo: int
    bar: int

class FooBar3(TypedDict):
    foo: int
    bar: int
    baz: NotRequired[int]

x6: FooBar1 | FooBar2 = {"foo": 1, "bar": 1}
reveal_type(x6)  # revealed: FooBar1 | FooBar2

x7: FooBar1 | FooBar3 = {"foo": 1, "bar": 1}
reveal_type(x7)  # revealed: FooBar1 | FooBar3

x8: FooBar1 | FooBar2 | FooBar3 | None = {"foo": 1, "bar": 1}
reveal_type(x8)  # revealed: FooBar1 | FooBar2 | FooBar3
```

In doing so, may have to infer the same type with multiple distinct type contexts:

```py
from typing import TypedDict

class NestedFoo(TypedDict):
    foo: list[FooBar1]

class NestedBar(TypedDict):
    foo: list[FooBar2]

x1: NestedFoo | NestedBar = {"foo": [{"foo": 1, "bar": 1}]}
reveal_type(x1)  # revealed: NestedFoo | NestedBar
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

class NamedPerson(TypedDict):
    name: str

class IgnoredNamedPerson(NamedPerson):
    name: int  # type: ignore

class SpecificallyIgnoredNamedPerson(NamedPerson):
    name: int  # type: ignore[ty:invalid-typed-dict-field]
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

## Constructing TypedDict from existing TypedDict

A `TypedDict` can be constructed from an existing `TypedDict` of the same type using either
positional argument passing or keyword unpacking:

```py
from typing import TypedDict

class Data(TypedDict):
    id: int
    name: str
    value: float

def process_data_positional(data: Data) -> Data:
    return Data(data)

def process_data_unpacking(data: Data) -> Data:
    return Data(**data)
```

Constructing from a compatible TypedDict (with same fields) works:

```py
from typing import TypedDict

class PersonBase(TypedDict):
    name: str
    age: int

class PersonAlias(TypedDict):
    name: str
    age: int

def copy_person(p: PersonBase) -> PersonAlias:
    return PersonAlias(**p)

def copy_person_positional(p: PersonBase) -> PersonAlias:
    return PersonAlias(p)
```

Optional source keys should not satisfy required constructor keys when unpacking:

```py
from typing import TypedDict

class MaybeName(TypedDict, total=False):
    name: str

class NeedsName(TypedDict):
    name: str

def f(maybe: MaybeName) -> NeedsName:
    # error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `NeedsName` constructor"
    return NeedsName(**maybe)
```

Guaranteed duplicate keys from unpacking should be rejected, matching runtime `TypeError`s:

```py
from typing import TypedDict

class DuplicateHasName(TypedDict):
    name: str

class DuplicateNeedsName(TypedDict):
    name: str

def duplicate_name_keys(
    left: DuplicateHasName,
    right: DuplicateHasName,
) -> DuplicateNeedsName:
    # error: [parameter-already-assigned]
    DuplicateNeedsName(**left, name="x")

    # error: [parameter-already-assigned]
    return DuplicateNeedsName(**left, **right)
```

Unpacking a TypedDict with extra keys flags the extra keys as errors, for consistency with the
behavior when passing all keys as explicit keyword arguments:

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int

class Employee(Person):
    employee_id: int

def get_person_from_employee(emp: Employee) -> Person:
    # error: [invalid-key] "Unknown key "employee_id" for TypedDict `Person`"
    return Person(**emp)
```

However, the positional form allows extra keys, by analogy with the fact that assignment
`p: Person = emp` is allowed (structural subtyping). It's not consistent that `Person(emp)` is more
lenient than `Person(**emp)`; ultimately this is because extra keys _should_ be always allowed for a
non-closed `TypedDict`, but we want to disallow explicit extra keys in order to catch typos, and so
we have to bite the inconsistency bullet somewhere.

```py
def get_person_from_employee_positional(emp: Employee) -> Person:
    return Person(emp)
```

Type mismatches in unpacked TypedDict fields should be detected:

```py
from typing import TypedDict

class Source(TypedDict):
    name: int  # Note: int, not str
    age: int

class Target(TypedDict):
    name: str
    age: int

def convert(src: Source) -> Target:
    # error: [invalid-argument-type]
    return Target(**src)

def convert_positional(src: Source) -> Target:
    # error: [invalid-argument-type]
    return Target(src)
```

Unpacking `Never` or a dynamic type (`Any`, `Unknown`) passes unconditionally, since these types can
have any keys:

```py
from typing import Any, TypedDict, Never

class Info(TypedDict):
    name: str
    value: int

def unpack_never(data: Never) -> Info:
    return Info(**data)

def unpack_any(data: Any) -> Info:
    return Info(**data)
```

PEP 695 type aliases to TypedDict types are also supported:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import TypedDict

class Record(TypedDict):
    id: int
    name: str

type RecordAlias = Record

def process_aliased(data: RecordAlias) -> Record:
    return Record(data)

def process_aliased_unpacking(data: RecordAlias) -> Record:
    return Record(**data)
```

Intersection types containing a TypedDict (e.g., from truthiness narrowing) are also supported. With
`total=False`, TypedDicts can be empty (falsy), so truthiness narrowing creates an intersection:

```py
from typing import TypedDict

class OptionalInfo(TypedDict, total=False):
    id: int
    name: str

def process_truthy(data: OptionalInfo) -> OptionalInfo:
    if data:
        reveal_type(data)  # revealed: OptionalInfo & ~AlwaysFalsy
        # Here data is `OptionalInfo & ~AlwaysFalsy`, but we can still construct OptionalInfo from it
        return OptionalInfo(data)
    return {}

def process_truthy_unpacking(data: OptionalInfo) -> OptionalInfo:
    if data:
        return OptionalInfo(**data)
    return {}
```

When we have an intersection of multiple TypedDict types, we extract ALL keys from ALL TypedDicts
(union of keys), because a value of an intersection type must satisfy all TypedDicts and therefore
has all their keys. For keys that appear in multiple TypedDicts, the types are intersected:

```py
from typing import TypedDict
from ty_extensions import Intersection

class TdA(TypedDict):
    name: str
    a_only: int

class TdB(TypedDict):
    name: str
    b_only: int

class NameOnly(TypedDict):
    name: str

# Positional form allows extra keys (like assignment)
def construct_from_intersection(data: Intersection[TdA, TdB]) -> NameOnly:
    return NameOnly(data)

# Unpacking form flags extra keys as errors
def construct_from_intersection_unpacking(data: Intersection[TdA, TdB]) -> NameOnly:
    # error: [invalid-key] "Unknown key "a_only" for TypedDict `NameOnly`"
    # error: [invalid-key] "Unknown key "b_only" for TypedDict `NameOnly`"
    return NameOnly(**data)
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
from typing_extensions import ReadOnly, TypedDict, Unpack

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

They _cannot_ be assigned to `dict[str, object]`, as that would allow them to be mutated in unsafe
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

reveal_type(alice["name"])  # revealed: Literal["Alice"]
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

If the additional, optional item in the target is read-only, the requirements are _somewhat_
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
    assert_type(var, Foo | Bar | Baz | int)
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
    next: "Node1 | None"

class Node2(TypedDict):
    value: int
    next: "Node2 | None"

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

RecursiveKey = list["RecursiveKey | None"]

class Person(TypedDict):
    name: str
    age: int | None
    leg: str

class Animal(TypedDict):
    name: str
    log: str

class Movie(TypedDict):
    name: str

NAME_FINAL: Final = "name"
AGE_FINAL: Final[Literal["age"]] = "age"

def _(
    recursive_key: RecursiveKey,
    movie: Movie,
    person: Person,
    animal: Animal,
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

    reveal_type(movie[recursive_key[0]])  # revealed: Unknown

    # error: [invalid-key] "Unknown key "anything" for TypedDict `Animal`"
    reveal_type(animal["anything"])  # revealed: Unknown

    reveal_type(being["name"])  # revealed: str

    # error: [invalid-key] "Unknown key "age" for TypedDict `Animal`"
    reveal_type(being["age"])  # revealed: int | None | Unknown

    # error: [invalid-key]
    # error: [invalid-key]
    reveal_type(being["legs"])  # revealed: Unknown
```

### Writing

```py
from typing_extensions import TypedDict, Final, Literal, LiteralString, Any
from ty_extensions import Intersection

class Person(TypedDict):
    name: str
    surname: str
    age: int | None
    leg: str

class Animal(TypedDict):
    name: str
    legs: int

NAME_FINAL: Final = "name"
AGE_FINAL: Final[Literal["age"]] = "age"

def _(person: Person):
    person["name"] = "Alice"
    person["age"] = 30

    # error: [invalid-key] "Unknown key "nane" for TypedDict `Person` - did you mean "name"?"
    person["nane"] = "Alice"

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

    # error: [invalid-key] "Unknown key "leg" for TypedDict `Animal`"
    being["leg"] = "unknown"

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

## `update()` with `ReadOnly` items

`update()` also cannot write to `ReadOnly` items, unless the source key is bottom-typed and
therefore cannot be present:

```py
from typing_extensions import Never, NotRequired, ReadOnly, TypedDict

class ReadOnlyPerson(TypedDict):
    id: ReadOnly[int]
    age: int

class AgePatch(TypedDict, total=False):
    age: int

class IdPatch(TypedDict, total=False):
    id: int

class ImpossibleIdPatch(TypedDict, total=False):
    id: NotRequired[Never]

person: ReadOnlyPerson = {"id": 1, "age": 30}
age_patch: AgePatch = {"age": 31}
id_patch: IdPatch = {"id": 2}
impossible_id_patch: ImpossibleIdPatch = {}

person.update(age_patch)

# error: [invalid-argument-type]
person.update(id_patch)

# error: [invalid-argument-type]
# error: [invalid-argument-type]
person.update({"id": 2})

# error: [invalid-argument-type]
person.update(id=2)

person.update(impossible_id_patch)
```

## Methods on `TypedDict`

```py
from typing import TypedDict
from typing_extensions import NotRequired

class Inner(TypedDict):
    inner: int

class Person(TypedDict):
    name: str
    age: int | None
    extra: NotRequired[str]
    inner: NotRequired[Inner]

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

    # Even another typed dict:
    reveal_type(p.get("inner", {"inner": 0}))  # revealed: Inner

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

Known-key `get()` calls also use the field type as bidirectional context when that produces a valid
default:

```py
from typing import TypedDict

class ResolvedData(TypedDict, total=False):
    x: int

class Payload(TypedDict, total=False):
    resolved: ResolvedData

class Payload2(TypedDict, total=False):
    resolved: ResolvedData

def takes_resolved(value: ResolvedData) -> None: ...
def _(payload: Payload) -> None:
    reveal_type(payload.get("resolved", {}))  # revealed: ResolvedData
    takes_resolved(payload.get("resolved", {}))

def _(payload: Payload | Payload2) -> None:
    reveal_type(payload.get("resolved", {}))  # revealed: ResolvedData
    takes_resolved(payload.get("resolved", {}))
```

With a gradual default, the specialized known-key overload and generic default overload both match,
so we currently fall back to `Unknown`:

```py
from typing import Any, TypedDict

class GradualDefault(TypedDict, total=False):
    x: int

def _(td: GradualDefault, default: Any) -> None:
    reveal_type(td.get("x", default))  # revealed: Unknown
```

Synthesized `get()` on unions falls back to generic resolution when a key is missing from one arm:

```py
class HasX(TypedDict):
    x: int

class NoX(TypedDict):
    y: str

class OptX(TypedDict):
    x: NotRequired[int]

def _(u: HasX | NoX) -> None:
    # Key "x" is missing from `NoX`, so specialization does not apply.
    reveal_type(u.get("x"))  # revealed: int | Unknown | None

def union_get(u: HasX | OptX) -> None:
    # `HasX.x` is required (returns `int`), `OptX.x` is not (returns `int | None`).
    reveal_type(u.get("x"))  # revealed: int | None
```

`pop()` also uses the field type as bidirectional context for the default argument:

```py
class Config(TypedDict, total=False):
    data: dict[str, int]

def _(c: Config) -> None:
    reveal_type(c.pop("data", {}))  # revealed: dict[str, int]
```

Synthesized `pop()` overloads on `TypedDict` unions correctly handle per-arm requiredness:

```py
class OptionalX(TypedDict):
    x: NotRequired[int]

class RequiredX(TypedDict):
    x: int

class OptStrX(TypedDict):
    x: NotRequired[str]

def _(v: OptionalX | RequiredX) -> None:
    # TODO: it's correct that we emit an error,
    # but this is a terrible error message:
    #
    # error: [call-non-callable] "Object of type `Overload[]` is not callable"
    reveal_type(v.pop("x"))  # revealed: Unknown

def union_pop_with_default(u: OptionalX | OptStrX) -> None:
    # `Literal[0]` is assignable to `int`, so `OptionalX` arm returns `int`; `OptStrX` arm
    # returns `str | Literal[0]`.
    reveal_type(u.pop("x", 0))  # revealed: int | str
```

Synthesized `setdefault()` overloads on `TypedDict` unions:

```py
class IntX(TypedDict):
    x: int

class StrX(TypedDict):
    x: str

def _(u: IntX | StrX) -> None:
    # error: [invalid-argument-type]
    reveal_type(u.setdefault("x", 1))  # revealed: int | str
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

But they _can_ be accessed on `type[Person]`, because this function would accept the class object
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

    # `Employee` is assignable to `Person`, so the result is `Person` in both directions.
    # The result dict will also contain the `employee_id` key at runtime, but that's
    # compatible with `Person` (which simply doesn't require it).
    reveal_type(p | e)  # revealed: Person
    reveal_type(e | p)  # revealed: Person
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

## Class-based inheritance from functional `TypedDict`

Class-based TypedDicts can inherit from functional TypedDicts:

```py
from typing import TypedDict

Base = TypedDict("Base", {"a": int}, total=False)

class Child(Base):
    b: str
    c: list[int]

child1 = Child(b="hello", c=[1, 2, 3])
child2 = Child(a=1, b="world", c=[])

reveal_type(child1["a"])  # revealed: int
reveal_type(child1["b"])  # revealed: str
reveal_type(child1["c"])  # revealed: list[int]

# error: [missing-typed-dict-key] "Missing required key 'b' in TypedDict `Child` constructor"
bad_child1 = Child(c=[1])

# error: [missing-typed-dict-key] "Missing required key 'c' in TypedDict `Child` constructor"
bad_child2 = Child(b="test")
```

## Incompatible field overrides

Overriding an inherited `TypedDict` field must preserve the compatibility rules from the typing
spec. We reject both direct overwrites and incompatible merges from multiple bases.

Mutable fields are invariant, so they cannot be overwritten with a different type, even if the new
type is a subtype of the old one:

```py
from typing import TypedDict
from typing_extensions import NotRequired, ReadOnly, Required

class Base(TypedDict):
    value: int

class BadSubtype(Base):
    # error: [invalid-typed-dict-field] "Inherited mutable field type `int` is incompatible with `bool`"
    value: bool

FunctionalBase = TypedDict("FunctionalBase", {"value": int})

class BadFunctionalSubtype(FunctionalBase):
    # error: [invalid-typed-dict-field] "Inherited mutable field type `int` is incompatible with `bool`"
    value: bool

class L(TypedDict):
    value: int

class R(TypedDict):
    value: bool

class BadMerge(L, R):  # error: [invalid-typed-dict-field] "Inherited mutable field type `bool` is incompatible with `int`"
    pass

class R2(TypedDict):
    value: int
    other: str

class GoodMerge(L, R2):
    pass
```

Read-only fields, on the other hand, can be overwritten with a compatible read-only type (a
subtype):

```py
class ReadOnlyBase(TypedDict):
    value: ReadOnly[int]

class ReadOnlySubtype(ReadOnlyBase):
    value: ReadOnly[bool]

class BadReadOnlySubtype(ReadOnlyBase):
    # error: [invalid-typed-dict-field] "Inherited read-only field type `int` is not assignable from `object`"
    value: ReadOnly[object]
```

Read-only fields can be made mutable in a subtype, but not the other way around:

```py
named_dict: ReadOnlyBase = {"value": 1}
named_dict["value"] = 2  # error: [invalid-assignment]

class MutableSubtype(ReadOnlyBase):
    value: int

album: MutableSubtype = {"value": 1}
album["value"] = 2  # no error here

class MutableBase(TypedDict):
    value: int

class BadReadOnlySubtype(MutableBase):
    # error: [invalid-typed-dict-field] "Mutable inherited fields cannot be redeclared as read-only"
    value: ReadOnly[int]
```

Read-only, non-required fields can be made required in a subtype, but not the other way around:

```py
class OptionalName(TypedDict):
    name: ReadOnly[NotRequired[str]]

optional_name: OptionalName = {}

class RequiredName(OptionalName):
    name: ReadOnly[Required[str]]

required_name: RequiredName = {"name": "Flood"}
bad_required_name: RequiredName = {}  # error: [missing-typed-dict-key]

class RequiredName(TypedDict):
    name: ReadOnly[Required[str]]

class BadOptionalName(RequiredName):
    # error: [invalid-typed-dict-field] "Required inherited fields cannot be redeclared as `NotRequired`"
    name: ReadOnly[NotRequired[str]]
```

This is not allowed for mutable fields, however (in either direction):

```py
class MutableNotRequired(TypedDict):
    value: NotRequired[int]

class BadNonRequiredSubtype(MutableNotRequired):
    # error: [invalid-typed-dict-field] "Mutable inherited `NotRequired` fields cannot be redeclared as required"
    value: Required[int]

class MutableRequired(TypedDict):
    value: Required[int]

class BadRequiredSubtype(MutableRequired):
    # error: [invalid-typed-dict-field] "Required inherited fields cannot be redeclared as `NotRequired`"
    value: NotRequired[int]
```

Inconsistencies are reported only once per field, even if they occur multiple times in the
hierarchy:

```py
class P1(TypedDict):
    value: str

class P2(TypedDict):
    value: str

class P3(TypedDict):
    value: str

class Child(P1, P2, P3):
    value: bytes  # error: [invalid-typed-dict-field]
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

### Validation of generic `TypedDict`s

```toml
[environment]
python-version = "3.12"
```

```py
from typing import TypedDict

class L[T](TypedDict):
    value: T

class R[T](TypedDict):
    value: T

class Merge(L[int], R[int]): ...
class MergeGeneric[T](L[T], R[T]): ...

# error: [invalid-typed-dict-field] "Inherited mutable field type `str` is incompatible with `int`"
class BadMerge(L[int], R[str]): ...

# error: [invalid-typed-dict-field] "Inherited mutable field type `T@BadMergeGeneric` is incompatible with `int`"
class BadMergeGeneric[T](L[int], R[T]): ...
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

TypedDict constructor calls should also use field type context when inferring nested values:

```py
from typing import TypedDict

class Comparison(TypedDict):
    field: str
    value: object

class Logical(TypedDict):
    primary: Comparison
    conditions: list[Comparison]

logical_from_literal = Logical(
    primary=Comparison(field="a", value="b"),
    conditions=[Comparison(field="c", value="d")],
)
logical_from_dict_call = Logical(dict(primary=dict(field="a", value="b"), conditions=[dict(field="c", value="d")]))

# error: [missing-typed-dict-key]
missing_primary_from_dict_call = Logical(primary=dict(field="a"), conditions=[dict(field="c", value="d")])

# error: [missing-typed-dict-key]
missing_primary_from_literal = Logical(primary={"field": "a"}, conditions=[dict(field="c", value="d")])
```

## Function/assignment syntax

TypedDicts can be created using the functional syntax:

```py
from typing_extensions import TypedDict
from ty_extensions import reveal_mro

Movie = TypedDict("Movie", {"name": str, "year": int})

reveal_type(Movie)  # revealed: <class 'Movie'>
reveal_mro(Movie)  # revealed: (<class 'Movie'>, typing.TypedDict, <class 'object'>)

movie = Movie(name="The Matrix", year=1999)

reveal_type(movie)  # revealed: Movie
reveal_type(movie["name"])  # revealed: str
reveal_type(movie["year"])  # revealed: int
```

An empty functional `TypedDict` should pass an empty dict for the `fields` argument:

```py
from typing_extensions import TypedDict

Empty = TypedDict("Empty", {})
empty = Empty()

reveal_type(Empty)  # revealed: <class 'Empty'>
reveal_type(empty)  # revealed: Empty

EmptyPartial = TypedDict("EmptyPartial", {}, total=False)
reveal_type(EmptyPartial())  # revealed: EmptyPartial
```

Omitting the `fields` argument entirely is an error:

```py
from typing_extensions import TypedDict

# error: [missing-argument] "No argument provided for required parameter `fields` of function `TypedDict`"
Empty = TypedDict("Empty")
reveal_type(Empty)  # revealed: type[Mapping[str, object]] & Unknown
```

Constructor validation also works with dict literals:

```py
from typing_extensions import TypedDict

Film = TypedDict("Film", {"title": str, "year": int})

# Valid usage
film1 = Film({"title": "The Matrix", "year": 1999})
film2 = Film(title="Inception", year=2010)

reveal_type(film1)  # revealed: Film
reveal_type(film2)  # revealed: Film

# error: [invalid-argument-type] "Invalid argument to key "year" with declared type `int` on TypedDict `Film`: value of type `Literal["not a year"]`"
invalid_type = Film({"title": "Bad", "year": "not a year"})

# error: [missing-typed-dict-key] "Missing required key 'year' in TypedDict `Film` constructor"
missing_key = Film({"title": "Incomplete"})

# error: [invalid-key] "Unknown key "director" for TypedDict `Film`"
extra_key = Film({"title": "Extra", "year": 2020, "director": "Someone"})
```

Inline functional `TypedDict`s preserve their field types too:

```py
from typing_extensions import TypedDict

inline = TypedDict("Inline", {"x": int})(x=1)
reveal_type(inline["x"])  # revealed: int

# error: [invalid-argument-type] "Invalid argument to key "x" with declared type `int` on TypedDict `InlineBad`: value of type `Literal["bad"]`"
inline_bad = TypedDict("InlineBad", {"x": int})(x="bad")
```

Inline functional `TypedDict`s preserve `ReadOnly` qualifiers:

```py
from typing_extensions import TypedDict, ReadOnly

inline_readonly = TypedDict("InlineReadOnly", {"id": ReadOnly[int]})(id=1)

# error: [invalid-assignment] "Cannot assign to key "id" on TypedDict `InlineReadOnly`: key is marked read-only"
inline_readonly["id"] = 2
```

Inline functional `TypedDict`s resolve string forward references to existing names:

```py
from typing_extensions import TypedDict

class Director:
    pass

inline_ref = TypedDict("InlineRef", {"director": "Director"})(director=Director())
reveal_type(inline_ref["director"])  # revealed: Director
```

## Function syntax with `total=False`

The `total=False` keyword makes all fields optional by default:

```py
from typing_extensions import TypedDict

# With total=False, all fields are optional by default
PartialMovie = TypedDict("PartialMovie", {"name": str, "year": int}, total=False)

# All fields are optional
partial = PartialMovie()
partial_with_name = PartialMovie(name="The Matrix")

# Non-bool arguments are rejected:
# error: [invalid-argument-type] "Invalid argument to parameter `total` of `TypedDict()`"
TotalNone = TypedDict("TotalNone", {"id": int}, total=None)

# Non-literal bool arguments are also rejected per the spec:
def f(total: bool) -> None:
    # error: [invalid-argument-type] "Invalid argument to parameter `total` of `TypedDict()`"
    TotalDynamic = TypedDict("TotalDynamic", {"id": int}, total=total)
```

## Function syntax with `Required` and `NotRequired`

The `Required` and `NotRequired` wrappers can be used to override the default requiredness:

```py
from typing_extensions import TypedDict, Required, NotRequired

# With total=True (default), all fields are required unless wrapped in NotRequired
MovieWithOptional = TypedDict("MovieWithOptional", {"name": str, "year": NotRequired[int]})

# name is required, year is optional
# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `MovieWithOptional` constructor"
empty_movie = MovieWithOptional()
movie_no_year = MovieWithOptional(name="The Matrix")
reveal_type(movie_no_year)  # revealed: MovieWithOptional
reveal_type(movie_no_year["name"])  # revealed: str
reveal_type(movie_no_year["year"])  # revealed: int
```

```py
from typing_extensions import TypedDict, Required, NotRequired

# With total=False, all fields are optional unless wrapped in Required
PartialWithRequired = TypedDict("PartialWithRequired", {"name": Required[str], "year": int}, total=False)

# name is required, year is optional
# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `PartialWithRequired` constructor"
empty_partial = PartialWithRequired()
partial_no_year = PartialWithRequired(name="The Matrix")
reveal_type(partial_no_year)  # revealed: PartialWithRequired
```

## Function syntax with invalid qualifiers

All type qualifiers except for `ReadOnly`, `Required` and `NotRequired` are rejected:

```py
from typing_extensions import ClassVar, Final, TypedDict
from dataclasses import InitVar

TD1 = TypedDict("TD1", {"x": ClassVar[int]})  # error: [invalid-type-form]
TD2 = TypedDict("TD2", {"x": Final[int]})  # error: [invalid-type-form]
TD3 = TypedDict("TD3", {"x": InitVar[int]})  # error: [invalid-type-form]

class TD4(TypedDict("TD4", {"x": ClassVar[int]})): ...  # error: [invalid-type-form]
class TD5(TypedDict("TD5", {"x": Final[int]})): ...  # error: [invalid-type-form]
class TD6(TypedDict("TD6", {"x": InitVar[int]})): ...  # error: [invalid-type-form]
```

## Function syntax with `closed`

The `closed` keyword is accepted but not yet fully supported:

```py
from typing_extensions import TypedDict

# closed is accepted (no error)
OtherMessage = TypedDict("OtherMessage", {"id": int, "content": str}, closed=True)

# Non-bool arguments are rejected:
# error: [invalid-argument-type] "Invalid argument to parameter `closed` of `TypedDict()`"
ClosedNone = TypedDict("ClosedNone", {"id": int}, closed=None)

# Non-literal bool arguments are also rejected per the spec:
def f(closed: bool) -> None:
    # error: [invalid-argument-type] "Invalid argument to parameter `closed` of `TypedDict()`"
    ClosedDynamic = TypedDict("ClosedDynamic", {"id": int}, closed=closed)
```

## Function syntax with `extra_items`

The `extra_items` keyword is accepted and validated as an annotation expression:

```py
from typing_extensions import ReadOnly, TypedDict, NotRequired, Required, ClassVar, Final
from dataclasses import InitVar

# extra_items is accepted (no error)
MovieWithExtras = TypedDict("MovieWithExtras", {"name": str}, extra_items=bool)

# Invalid type expressions are rejected:
# error: [invalid-syntax-in-forward-annotation] "Syntax error in forward annotation: Unexpected token at the end of an expression"
BadExtras = TypedDict("BadExtras", {"name": str}, extra_items="not a type expression")

# Forward references in extra_items are supported:
TD = TypedDict("TD", {}, extra_items="TD | None")
reveal_type(TD)  # revealed: <class 'TD'>

class Foo(TypedDict("T", {}, extra_items="Foo | None")): ...

reveal_type(Foo)  # revealed: <class 'Foo'>

# The `ReadOnly` type qualifier is valid in `extra_items` (annotation expression, not type expression):
TD2 = TypedDict("TD2", {}, extra_items=ReadOnly[int])

class Bar(TypedDict("TD3", {}, extra_items=ReadOnly[int])): ...

# But all other qualifiers are rejected:

TD4 = TypedDict("TD4", {}, extra_items=Required[int])  # error: [invalid-type-form]
TD5 = TypedDict("TD5", {}, extra_items=NotRequired[int])  # error: [invalid-type-form]
TD6 = TypedDict("TD6", {}, extra_items=ClassVar[int])  # error: [invalid-type-form]
TD7 = TypedDict("TD7", {}, extra_items=InitVar[int])  # error: [invalid-type-form]
TD8 = TypedDict("TD8", {}, extra_items=Final[int])  # error: [invalid-type-form]

class TD9(TypedDict("TD9", {}, extra_items=Required[int])): ...  # error: [invalid-type-form]
class TD10(TypedDict("TD10", {}, extra_items=NotRequired[int])): ...  # error: [invalid-type-form]
class TD11(TypedDict("TD11", {}, extra_items=ClassVar[int])): ...  # error: [invalid-type-form]
class TD12(TypedDict("TD12", {}, extra_items=InitVar[int])): ...  # error: [invalid-type-form]
class TD13(TypedDict("TD13", {}, extra_items=Final[int])): ...  # error: [invalid-type-form]
```

## Function syntax with forward references

Functional TypedDict supports forward references (string annotations):

```py
from typing_extensions import TypedDict, NotRequired

# Forward reference to a class defined below
MovieWithDirector = TypedDict("MovieWithDirector", {"title": str, "director": "Director"})

class Director:
    name: str

movie: MovieWithDirector = {"title": "The Matrix", "director": Director()}
reveal_type(movie)  # revealed: MovieWithDirector

# Forward reference to a class defined above
MovieWithDirector2 = TypedDict("MovieWithDirector2", {"title": str, "director": NotRequired["Director"]})

movie2: MovieWithDirector2 = {"title": "The Matrix"}
reveal_type(movie2)  # revealed: MovieWithDirector2
```

String annotations can also wrap the entire `Required` or `NotRequired` qualifier:

```py
from typing_extensions import TypedDict, Required, NotRequired

# NotRequired as a string annotation
TD = TypedDict("TD", {"required": str, "optional": "NotRequired[int]"})

# 'required' is required, 'optional' is not required
td1: TD = {"required": "hello"}  # Valid - optional is not required
td2: TD = {"required": "hello", "optional": 42}  # Valid - all keys provided
reveal_type(td1)  # revealed: TD
reveal_type(td1["required"])  # revealed: Literal["hello"]
reveal_type(td1["optional"])  # revealed: int

# error: [missing-typed-dict-key] "Missing required key 'required' in TypedDict `TD` constructor"
bad_td: TD = {"optional": 42}

# Also works with Required in total=False TypedDicts
TD2 = TypedDict("TD2", {"required": "Required[str]", "optional": int}, total=False)

# 'required' is required, 'optional' is not required
td3: TD2 = {"required": "hello"}  # Valid
# error: [missing-typed-dict-key] "Missing required key 'required' in TypedDict `TD2` constructor"
bad_td2: TD2 = {"optional": 42}
```

## `Unpack[TypedDict]` in `**kwargs`

Using `Unpack[TypedDict]` on a `**kwargs` parameter should expose named keyword parameters to
callers while preserving the original `TypedDict` shape inside the function body.

### Inside the function body

Inside the function, `kwargs` should still behave like the original `TypedDict`, including
flow-sensitive access to optional keys.

```py
from typing_extensions import NotRequired, Required, TypedDict, Unpack

class TD1(TypedDict):
    v1: Required[int]
    v2: NotRequired[str]

class TD2(TD1):
    v3: Required[str]

def func(**kwargs: Unpack[TD2]) -> None:
    reveal_type(kwargs)  # revealed: TD2
    reveal_type(kwargs["v1"])  # revealed: int
    if "v2" in kwargs:
        reveal_type(kwargs["v2"])  # revealed: str
    reveal_type(kwargs["v3"])  # revealed: str
```

### Calling the function

At the call site, required keys must be provided, unknown keys must be rejected, and `**kwargs`
unpacking should be validated against the `TypedDict` shape.

```py
from typing_extensions import NotRequired, Required, TypedDict, Unpack

class TD1(TypedDict):
    v1: Required[int]
    v2: NotRequired[str]

class TD2(TD1):
    v3: Required[str]

def func(**kwargs: Unpack[TD2]) -> None:
    pass

# error: [missing-argument]
func()
func(v1=1, v3="ok")
func(v1=1, v2="optional", v3="ok")

# error: [unknown-argument]
func(v1=1, v3="ok", v4=1)
```

### Assignability to explicit keyword-only signatures

A callable using `**kwargs: Unpack[TD2]` should line up with equivalent explicit keyword-only
signatures, and the assignability should work in both directions.

```py
from typing import Protocol
from typing_extensions import NotRequired, Required, TypedDict, Unpack

class TD1(TypedDict):
    v1: Required[int]
    v2: NotRequired[str]

class TD2(TD1):
    v3: Required[str]

def func(**kwargs: Unpack[TD2]) -> None:
    pass

class ExplicitKwargs(Protocol):
    def __call__(self, *, v1: int, v3: str, v2: str = "") -> None: ...

class TypedDictKwargs(Protocol):
    def __call__(self, **kwargs: Unpack[TD2]) -> None: ...

explicit_ok: ExplicitKwargs = func
typed_dict_ok: TypedDictKwargs = func

def _(explicit: ExplicitKwargs, typed_dict: TypedDictKwargs) -> None:
    typed_dict_2: TypedDictKwargs = explicit
    explicit_2: ExplicitKwargs = typed_dict

def func7(*, v1: int, v3: str, v2: str = "") -> None:
    pass

typed_dict_from_explicit: TypedDictKwargs = func7
```

### Missing required keys remain incompatible

A callable that does not accept all required unpacked keys should not be assignable to the unpacked
form.

```py
from typing import Protocol
from typing_extensions import NotRequired, Required, TypedDict, Unpack

class TD1(TypedDict):
    v1: Required[int]
    v2: NotRequired[str]

class TD2(TD1):
    v3: Required[str]

def func(**kwargs: Unpack[TD2]) -> None:
    pass

class MissingRequiredKwarg(Protocol):
    def __call__(self, *, v1: int) -> None: ...

# error: [invalid-assignment]
missing_required: MissingRequiredKwarg = func
```

### Optional-only unpacked kwargs are not a single keyword parameter

An unpacked all-optional `TypedDict` still describes named keyword arguments rather than a single
keyword whose value has that `TypedDict` type.

```py
from typing import Protocol
from typing_extensions import TypedDict, Unpack

class OptionalOnlyKwargs(TypedDict, total=False):
    a: int

def accepts_optional_kwargs(**kwargs: Unpack[OptionalOnlyKwargs]) -> None:
    pass

class WantsB(Protocol):
    def __call__(self, *, b: OptionalOnlyKwargs) -> None: ...

# error: [invalid-assignment]
wants_b: WantsB = accepts_optional_kwargs
```

### Invalid `Unpack` signatures

These signatures should be rejected. Some of them use a well-formed `Unpack[...]` expression, but
the overall `**kwargs` signature is still invalid: mixing explicit parameters with conflicting
unpacked names, using a type variable, or using a union instead of a concrete `TypedDict`.

```py
from typing import TypeVar, Union
from typing_extensions import NotRequired, Required, TypedDict, Unpack

class TD1(TypedDict):
    v1: Required[int]
    v2: NotRequired[str]

class TD2(TD1):
    v3: Required[str]

def func5(v1: int, **kwargs: Unpack[TD2]) -> None:  # error: [invalid-type-form]
    pass

T = TypeVar("T", bound=TD2)

def func6(**kwargs: Unpack[T]) -> None:  # error: [invalid-type-form]
    pass

TDUnion = Union[TD1, TD2]

def func_union(**kwargs: Unpack[TDUnion]) -> None:  # error: [invalid-type-form]
    pass
```

### Aliases are followed

Type aliases to a `TypedDict` should still be accepted in `Unpack`.

```py
from typing_extensions import NotRequired, Required, TypedDict, Unpack

class TD1(TypedDict):
    v1: Required[int]
    v2: NotRequired[str]

class TD2(TD1):
    v3: Required[str]

TD2Alias = TD2

def func_alias(**kwargs: Unpack[TD2Alias]) -> None:
    reveal_type(kwargs)  # revealed: TD2
```

### Stringified annotations are followed

Quoted annotations should behave the same way as unquoted `Unpack[TypedDict]` annotations.

```py
from typing_extensions import TypedDict, Unpack

class StringifiedTD(TypedDict):
    a: int

def stringified(**kwargs: "Unpack[StringifiedTD]") -> None:
    reveal_type(kwargs)  # revealed: StringifiedTD

stringified(a=1)
```

## Bare `TypedDict` annotations in `**kwargs`

A bare `TypedDict` annotation on `**kwargs` still means “arbitrary keyword names whose values have
this `TypedDict` type”. Only `Unpack[TypedDict]` should expose named keyword parameters.

```py
from typing import Protocol
from typing_extensions import TypedDict

class BareKwargs(TypedDict):
    a: int

def plain(**kwargs: BareKwargs) -> None:
    reveal_type(kwargs)  # revealed: dict[str, BareKwargs]

plain(a=BareKwargs(a=1))

# error: [invalid-argument-type]
plain(a=1)

class BareKwargsProtocol(Protocol):
    def __call__(self, **kwargs: BareKwargs) -> None: ...

class ExplicitAProtocol(Protocol):
    def __call__(self, *, a: int) -> None: ...

bare_kwargs_ok: BareKwargsProtocol = plain

# error: [invalid-assignment]
explicit_a_bad: ExplicitAProtocol = plain

def unrelated_named_parameter(x: int, **kwargs: BareKwargs) -> None:
    reveal_type(kwargs)  # revealed: dict[str, BareKwargs]
```

## Recursive functional `TypedDict` (unstringified forward reference)

Forward references in functional `TypedDict` calls must be stringified, since the field types are
evaluated at runtime. An unstringified self-reference is an error:

```py
from typing import TypedDict

# error: [unresolved-reference] "Name `T` used when not defined"
T = TypedDict("T", {"x": T | None})
```

## Recursive functional `TypedDict`

Functional `TypedDict`s can also be recursive, referencing themselves in field types:

```py
from __future__ import annotations
from typing_extensions import TypedDict

# Self-referencing TypedDict using functional syntax
TreeNode = TypedDict("TreeNode", {"value": int, "left": "TreeNode | None", "right": "TreeNode | None"})

reveal_type(TreeNode)  # revealed: <class 'TreeNode'>

leaf: TreeNode = {"value": 1, "left": None, "right": None}
reveal_type(leaf["value"])  # revealed: Literal[1]
reveal_type(leaf["left"])  # revealed: None

tree: TreeNode = {
    "value": 10,
    "left": {"value": 5, "left": None, "right": None},
    "right": {"value": 15, "left": None, "right": None},
}

# error: [invalid-argument-type]
bad_tree: TreeNode = {"value": 1, "left": "not a node", "right": None}
```

## Deprecated keyword-argument syntax

The deprecated keyword-argument syntax (fields as keyword arguments instead of a dict) is rejected.
This syntax is deprecated since Python 3.11, and raises an exception on Python 3.13+:

```py
from typing_extensions import TypedDict

# error: [unknown-argument] "Argument `name` does not match any known parameter of function `TypedDict`"
# error: [unknown-argument] "Argument `year` does not match any known parameter of function `TypedDict`"
# error: [missing-argument] "No argument provided for required parameter `fields` of function `TypedDict`"
Movie2 = TypedDict("Movie2", name=str, year=int)
```

## Function syntax with invalid arguments

<!-- snapshot-diagnostics -->

```py
from typing_extensions import TypedDict

# error: [too-many-positional-arguments] "Too many positional arguments to function `TypedDict`: expected 2, got 3"
TypedDict("Foo", {}, {})
# error: [missing-argument] "No arguments provided for required parameters `typename` and `fields` of function `TypedDict`"
TypedDict()
# error: [missing-argument] "No argument provided for required parameter `fields` of function `TypedDict`"
TypedDict("Foo")

# error: [invalid-argument-type] "Invalid argument to parameter `typename` of `TypedDict()`: Expected `str`, found `Literal[123]`"
Bad1 = TypedDict(123, {"name": str})

# error: [mismatched-type-name] "The name passed to `TypedDict` must match the variable it is assigned to: Expected "BadTypedDict3", got "WrongName""
BadTypedDict3 = TypedDict("WrongName", {"name": str})
reveal_type(BadTypedDict3)  # revealed: <class 'WrongName'>

def f(x: str) -> None:
    # error: [mismatched-type-name] "The name passed to `TypedDict` must match the variable it is assigned to: Expected "Y", got variable of type `str`"
    Y = TypedDict(x, {})

def g(x: str) -> None:
    TypedDict(x, {})  # fine

name = "GoodTypedDict"
GoodTypedDict = TypedDict(name, {"name": str})

# error: [invalid-argument-type] "Expected a dict literal for parameter `fields` of `TypedDict()`"
Bad2 = TypedDict("Bad2", "not a dict")
# error: [invalid-argument-type] "Expected a dict literal for parameter `fields` of `TypedDict()`"
TypedDict("Bad2", "not a dict")

def get_fields() -> dict[str, object]:
    return {"name": str}

# error: [invalid-argument-type] "Expected a dict literal for parameter `fields` of `TypedDict()`"
Bad2b = TypedDict("Bad2b", get_fields())

# error: [invalid-argument-type] "Invalid argument to parameter `total` of `TypedDict()`"
Bad3 = TypedDict("Bad3", {"name": str}, total="not a bool")

# error: [invalid-argument-type] "Invalid argument to parameter `closed` of `TypedDict()`"
Bad4 = TypedDict("Bad4", {"name": str}, closed=123)

tup = ("foo", "bar")
kw = {"name": str}

# error: [invalid-argument-type] "Variadic positional arguments are not supported in `TypedDict()` calls"
Bad5 = TypedDict(*tup)

# error: [invalid-argument-type] "Variadic keyword arguments are not supported in `TypedDict()` calls"
Bad6 = TypedDict("Bad6", {"name": str}, **kw)

# error: [invalid-argument-type] "Variadic positional and keyword arguments are not supported in `TypedDict()` calls"
Bad7 = TypedDict(*tup, "foo", "bar", **kw)

# error: [invalid-argument-type] "Variadic keyword arguments are not supported in `TypedDict()` calls"
# error: [unknown-argument] "Argument `random_other_arg` does not match any known parameter of function `TypedDict`"
Bad7b = TypedDict("Bad7b", **kw, random_other_arg=56)

kwargs = {"x": int}

# error: [invalid-argument-type] "Keyword splats are not allowed in the `fields` parameter to `TypedDict()`"
Bad8 = TypedDict("Bad8", {**kwargs})
# error: [invalid-argument-type] "Keyword splats are not allowed in the `fields` parameter to `TypedDict()`"
TypedDict("Bad8", {**kwargs})
# error: [invalid-argument-type] "Keyword splats are not allowed in the `fields` parameter to `TypedDict()`"
# error: [invalid-argument-type] "Keyword splats are not allowed in the `fields` parameter to `TypedDict()`"
Bad81 = TypedDict("Bad81", {**kwargs, **kwargs})
# error: [invalid-argument-type] "Keyword splats are not allowed in the `fields` parameter to `TypedDict()`"
# error: [invalid-argument-type] "Keyword splats are not allowed in the `fields` parameter to `TypedDict()`"
TypedDict("Bad81", {**kwargs, **kwargs})
# error: [invalid-argument-type] "Keyword splats are not allowed in the `fields` parameter to `TypedDict()`"
# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
Bad82 = TypedDict("Bad82", {**kwargs, "foo": []})
# error: [invalid-argument-type] "Keyword splats are not allowed in the `fields` parameter to `TypedDict()`"
# error: [invalid-type-form] "List literals are not allowed in this context in a type expression"
TypedDict("Bad82", {**kwargs, "foo": []})

def get_name() -> str:
    return "x"

name = get_name()

# error: [invalid-argument-type] "Expected a string-literal key in the `fields` dict of `TypedDict()`"
Bad9 = TypedDict("Bad9", {name: int})

# error: [invalid-argument-type] "Expected a string-literal key in the `fields` dict of `TypedDict()`"
# error: [invalid-type-form]
Bad10 = TypedDict("Bad10", {name: 42})

# error: [invalid-argument-type] "Expected a string-literal key in the `fields` dict of `TypedDict()`"
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
class Bad11(TypedDict("Bad11", {name: 42})): ...

# error: [invalid-argument-type] "Invalid argument to parameter `typename` of `TypedDict()`: Expected `str`, found `Literal[123]`"
class Bad12(TypedDict(123, {"field": int})): ...
```

## Functional `TypedDict` with unknown fields

When a functional `TypedDict` has unparseable fields (e.g., non-literal keys), the resulting type
behaves like a `TypedDict` with no known fields. This is consistent with pyright and mypy:

```py
from typing import TypedDict

def get_name() -> str:
    return "x"

key = get_name()

# error: [invalid-argument-type] "Expected a string-literal key in the `fields` dict of `TypedDict()`"
Bad = TypedDict("Bad", {key: int})

# No known fields, so keyword arguments are rejected
# error: [invalid-key]
b = Bad(x=1)
reveal_type(b)  # revealed: Bad

# Field access reports unknown keys
# error: [invalid-key]
reveal_type(b["x"])  # revealed: Unknown
```

## Equivalence between functional and class-based `TypedDict`

Functional and class-based `TypedDict`s with the same fields are structurally equivalent:

```py
from typing import TypedDict
from typing_extensions import assert_type
from ty_extensions import is_assignable_to, is_equivalent_to, static_assert

class ClassBased(TypedDict):
    name: str
    age: int

Functional = TypedDict("Functional", {"name": str, "age": int})

static_assert(is_equivalent_to(ClassBased, Functional))
static_assert(is_assignable_to(ClassBased, Functional))
static_assert(is_assignable_to(Functional, ClassBased))

cb: ClassBased = {"name": "Alice", "age": 30}
assert_type(cb, Functional)

fn: Functional = {"name": "Bob", "age": 25}
assert_type(fn, ClassBased)
```

## Subtyping between functional and class-based `TypedDict`

A functional `TypedDict` is not a subtype of a class-based one when the field types differ:

```py
from typing import TypedDict
from ty_extensions import is_assignable_to, static_assert

class StrFields(TypedDict):
    x: str

IntFields = TypedDict("IntFields", {"x": int})

static_assert(not is_assignable_to(IntFields, StrFields))
static_assert(not is_assignable_to(StrFields, IntFields))
```

## Methods on functional `TypedDict`

Functional `TypedDict`s support the same synthesized methods as class-based ones:

```py
from typing import TypedDict

Person = TypedDict("Person", {"name": str, "age": int})

def _(p: Person) -> None:
    # __getitem__
    reveal_type(p["name"])  # revealed: str
    reveal_type(p["age"])  # revealed: int

    # get()
    reveal_type(p.get("name"))  # revealed: str
    reveal_type(p.get("name", "default"))  # revealed: str
    reveal_type(p.get("unknown"))  # revealed: Unknown | None

    # setdefault()
    reveal_type(p.setdefault("name", "Alice"))  # revealed: str

    # __contains__
    reveal_type("name" in p)  # revealed: bool

    # __setitem__
    p["name"] = "Alice"
    # error: [invalid-assignment]
    p["name"] = 42

    # __delitem__ on required fields is an error
    # error: [invalid-argument-type]
    del p["name"]
```

Functional `TypedDict`s with `total=False` have optional fields that support `pop` and `del`:

```py
from typing import TypedDict

Partial = TypedDict("Partial", {"name": str, "extra": int}, total=False)

def _(p: Partial) -> None:
    reveal_type(p.get("name"))  # revealed: str | None
    reveal_type(p.get("name", "default"))  # revealed: str
    reveal_type(p.pop("name"))  # revealed: str
    reveal_type(p.pop("name", "fallback"))  # revealed: str
    reveal_type(p.copy())  # revealed: Partial
    del p["extra"]
```

## Merge operators on functional `TypedDict`

```py
from typing import TypedDict

Foo = TypedDict("Foo", {"x": int, "y": str})

def _(a: Foo, b: Foo) -> None:
    reveal_type(a | b)  # revealed: Foo
    reveal_type(a | {"x": 1})  # revealed: Foo
    reveal_type(a | {"x": 1, "y": "a", "z": True})  # revealed: dict[str, object]
```

## Error cases

### `typing.TypedDict` is not allowed in type expressions

<!-- snapshot-diagnostics -->

```py
from typing import TypedDict

# error: [invalid-type-form] "The special form `typing.TypedDict` is not allowed in type expressions"
x: TypedDict = {"name": "Alice"}
```

### `ReadOnly`, `Required` and `NotRequired` not allowed in parameter annotations or return annotations

```pyi
from typing_extensions import Required, NotRequired, ReadOnly

def bad(
    # error: [invalid-type-form] "Type qualifier `typing.Required` is not allowed in parameter annotations"
    a: Required[int],
    # error: [invalid-type-form] "Type qualifier `typing.NotRequired` is not allowed in parameter annotations"
    b: NotRequired[int],
    # error: [invalid-type-form] "Type qualifier `typing.ReadOnly` is not allowed in parameter annotations"
    c: ReadOnly[int],
): ...

# error: [invalid-type-form] "Type qualifier `typing.Required` is not allowed in return type annotations"
def bad2() -> Required[int]: ...

# error: [invalid-type-form] "Type qualifier `typing.NotRequired` is not allowed in return type annotations"
def bad2() -> NotRequired[int]: ...

# error: [invalid-type-form] "Type qualifier `typing.ReadOnly` is not allowed in return type annotations"
def bad2() -> ReadOnly[int]: ...
```

### `Required`, `NotRequired` and `ReadOnly` require exactly one argument

```py
from typing_extensions import TypedDict, ReadOnly, Required, NotRequired

class Foo(TypedDict):
    a: Required  # error: [invalid-type-form] "`Required` may not be used without a type argument"
    b: Required[()]  # error: [invalid-type-form] "Type qualifier `typing.Required` expected exactly 1 argument, got 0"
    c: Required[int, str]  # error: [invalid-type-form] "Type qualifier `typing.Required` expected exactly 1 argument, got 2"
    d: NotRequired  # error: [invalid-type-form] "`NotRequired` may not be used without a type argument"
    e: NotRequired[()]  # error: [invalid-type-form] "Type qualifier `typing.NotRequired` expected exactly 1 argument, got 0"
    # error: [invalid-type-form] "Type qualifier `typing.NotRequired` expected exactly 1 argument, got 2"
    f: NotRequired[int, str]
    g: ReadOnly  # error: [invalid-type-form] "`ReadOnly` may not be used without a type argument"
    h: ReadOnly[()]  # error: [invalid-type-form] "Type qualifier `typing.ReadOnly` expected exactly 1 argument, got 0"
    i: ReadOnly[int, str]  # error: [invalid-type-form] "Type qualifier `typing.ReadOnly` expected exactly 1 argument, got 2"
```

### `Required`, `NotRequired` and `ReadOnly` are not allowed outside `TypedDict`

```py
from typing_extensions import Required, NotRequired, TypedDict, ReadOnly

# error: [invalid-type-form] "`Required` is only allowed in TypedDict fields"
x: Required[int]
# error: [invalid-type-form] "`NotRequired` is only allowed in TypedDict fields"
y: NotRequired[str]
# error: [invalid-type-form] "`ReadOnly` is only allowed in TypedDict fields"
z: ReadOnly[str]

class MyClass:
    # error: [invalid-type-form] "`Required` is only allowed in TypedDict fields"
    x: Required[int]
    # error: [invalid-type-form] "`NotRequired` is only allowed in TypedDict fields"
    y: NotRequired[str]
    # error: [invalid-type-form] "`ReadOnly` is only allowed in TypedDict fields"
    z: ReadOnly[str]

def f():
    # error: [invalid-type-form] "`Required` is only allowed in TypedDict fields"
    x: Required[int] = 1
    # error: [invalid-type-form] "`NotRequired` is only allowed in TypedDict fields"
    y: NotRequired[str] = ""
    # error: [invalid-type-form] "`ReadOnly` is only allowed in TypedDict fields"
    z: ReadOnly[str]

# fine
MyFunctionalTypedDict = TypedDict("MyFunctionalTypedDict", {"not-an-identifier": Required[int]})

class FunctionalTypedDictSubclass(MyFunctionalTypedDict):
    y: NotRequired[int]  # fine
```

### Nested `Required` and `NotRequired`

`Required` and `NotRequired` cannot be nested inside each other:

```py
from typing_extensions import TypedDict, Required, NotRequired

class TD(TypedDict):
    # error: [invalid-type-form] "`typing.Required` cannot be nested inside `Required` or `NotRequired`"
    a: Required[Required[int]]
    # error: [invalid-type-form] "`typing.NotRequired` cannot be nested inside `Required` or `NotRequired`"
    b: NotRequired[NotRequired[int]]
    # error: [invalid-type-form] "`typing.Required` cannot be nested inside `Required` or `NotRequired`"
    c: Required[NotRequired[int]]
    # error: [invalid-type-form] "`typing.NotRequired` cannot be nested inside `Required` or `NotRequired`"
    d: NotRequired[Required[int]]
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

### Cannot be used in `isinstance` tests or `issubclass` tests

```py
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

def _(obj: object, obj2: type):
    # error: [isinstance-against-typed-dict] "`TypedDict` class `Person` cannot be used as the second argument to `isinstance`"
    isinstance(obj, Person)
    # error: [isinstance-against-typed-dict] "`TypedDict` class `Person` cannot be used as the second argument to `issubclass`"
    issubclass(obj2, Person)
```

The same applies when a `TypedDict` class appears inside a tuple, including non-literal tuples:

```py
def _(obj: object, obj2: type):
    isinstance(obj, (int, Person))  # error: [isinstance-against-typed-dict]
    issubclass(obj2, (int, Person))  # error: [isinstance-against-typed-dict]
    isinstance(obj, (int, (str, Person)))  # error: [isinstance-against-typed-dict]

classes = (int, Person)

def _(obj: object):
    isinstance(obj, classes)  # error: [isinstance-against-typed-dict]
```

They also cannot be used in class patterns for `match` statements:

```py
def f(x: object):
    match x:
        # error: [isinstance-against-typed-dict] "`TypedDict` class `Person` cannot be used in a class pattern"
        case Person():
            pass
        # error: [isinstance-against-typed-dict] "`TypedDict` class `Person` cannot be used in a class pattern"
        case object(parent=Person()):
            pass
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
    person["nane"]  # error: [invalid-key]

NAME_KEY: Final = "nane"

def access_invalid_key(person: Person):
    person[NAME_KEY]  # error: [invalid-key]

def access_with_str_key(person: Person, str_key: str):
    person[str_key]  # error: [invalid-key]

def write_to_key_with_wrong_type(person: Person):
    person["age"] = "42"  # error: [invalid-assignment]

def write_to_non_existing_key(person: Person):
    person["nane"] = "Alice"  # error: [invalid-key]

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
    person['nane'] = "Alice"  # fmt: skip
```

Field override diagnostics should point at the incompatible child declaration and show inherited
declarations as separate notes:

```py
class MovieBase(TypedDict):
    name: str

class BadMovie(MovieBase):
    name: int  # error: [invalid-typed-dict-field]

class LeftBase(TypedDict):
    value: int

class RightBase(TypedDict):
    value: str

class BadMerge(LeftBase, RightBase):  # error: [invalid-typed-dict-field]
    pass
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

However, note that most pairs of non-final classes are _not_ disjoint from each other, even if
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
common need to have _compatible_ types (in the fully-static case, equivalent types):

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
`Required` field is _not_ necessarily assignable to its `NotRequired` counterpart. In particular, if
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
    elif 42 == u["tag"]:
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

Intersections containing a TypedDict with literal fields can be narrowed with equality checks. Since
`Foo` requires `tag == "foo"`, the else branch is `Never`:

```py
from ty_extensions import Intersection
from typing import Any

def _(x: Intersection[Foo, Any]):
    if x["tag"] == "foo":
        reveal_type(x)  # revealed: Foo & Any
    else:
        reveal_type(x)  # revealed: Never
```

But intersections with non-literal fields cannot be narrowed:

```py
from ty_extensions import Intersection
from typing import Any

def _(x: Intersection[NonLiteralTD, Any]):
    if x["tag"] == 42:
        reveal_type(x)  # revealed: NonLiteralTD & Any
    else:
        reveal_type(x)  # revealed: NonLiteralTD & Any
```

This is especially important when the field type is disjoint from the comparison literal. Even
though `str` and `int` are disjoint, we can't narrow here because a `str` subclass could override
`__eq__` to return `True`. Without proper handling, this would wrongly narrow to `Never`:

```py
from ty_extensions import Intersection
from typing import Any

class StrTagTD(TypedDict):
    tag: str

def _(x: Intersection[StrTagTD, Any]):
    if x["tag"] == 42:
        reveal_type(x)  # revealed: StrTagTD & Any
    else:
        reveal_type(x)  # revealed: StrTagTD & Any
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
field, it could be _assigned to_ with another `TypedDict` that does:

```py
from typing_extensions import Literal

class Foo(TypedDict):
    foo: int

class Bar(TypedDict):
    bar: int

def disappointment(u: Foo | Bar, v: Literal["foo"]):
    if "foo" in u:
        # We can't narrow the union here...
        reveal_type(u)  # revealed: Foo | Bar
    else:
        # ...(even though we *can* narrow it here)...
        reveal_type(u)  # revealed: Bar

    if v in u:
        reveal_type(u)  # revealed: Foo | Bar
    else:
        reveal_type(u)  # revealed: Bar

# ...because `u` could turn out to be one of these.
class FooBar(TypedDict):
    foo: int
    bar: int

static_assert(is_assignable_to(FooBar, Foo))
static_assert(is_assignable_to(FooBar, Bar))
```

`not in` works in the opposite way to `in`: we can narrow in the positive case, but we cannot narrow
in the negative case. The following snippet also tests our narrowing behaviour for intersections
that contain `TypedDict`s, and unions that contain intersections that contain `TypedDict`s:

```py
from typing_extensions import Literal, Any
from ty_extensions import Intersection, is_assignable_to, static_assert

def _(t: Bar, u: Foo | Intersection[Bar, Any], v: Intersection[Bar, Any], w: Literal["bar"]):
    reveal_type(u)  # revealed: Foo | (Bar & Any)
    reveal_type(v)  # revealed: Bar & Any

    if "bar" not in t:
        reveal_type(t)  # revealed: Never
    else:
        reveal_type(t)  # revealed: Bar

    if "bar" not in u:
        reveal_type(u)  # revealed: Foo
    else:
        reveal_type(u)  # revealed: Foo | (Bar & Any)

    if "bar" not in v:
        reveal_type(v)  # revealed: Never
    else:
        reveal_type(v)  # revealed: Bar & Any

    if w not in u:
        reveal_type(u)  # revealed: Foo
    else:
        reveal_type(u)  # revealed: Foo | (Bar & Any)
```

With `closed=True`, the narrowing that we couldn't do above becomes possible, because a [closed]
TypedDict is guaranteed not to have extra keys:

```py
from typing_extensions import Literal, TypedDict

class ClosedFoo(TypedDict, closed=True):
    foo: int

class ClosedBar(TypedDict, closed=True):
    bar: int

def _(u: ClosedFoo | ClosedBar, v: Literal["foo"]):
    if "foo" in u:
        # TODO: should be `ClosedFoo`
        reveal_type(u)  # revealed: ClosedFoo | ClosedBar
    else:
        reveal_type(u)  # revealed: ClosedBar

    if v in u:
        # TODO: should be `ClosedFoo`
        reveal_type(u)  # revealed: ClosedFoo | ClosedBar
    else:
        reveal_type(u)  # revealed: ClosedBar
```

Similarly, for `not in`, we can now also narrow in the negative case:

```py
from typing_extensions import Literal, Any
from ty_extensions import Intersection

def _(
    t: ClosedBar,
    u: ClosedFoo | Intersection[ClosedBar, Any],
    v: Intersection[ClosedBar, Any],
    w: Literal["bar"],
):
    if "bar" not in u:
        reveal_type(u)  # revealed: ClosedFoo
    else:
        # TODO: should be `ClosedBar & Any`
        reveal_type(u)  # revealed: ClosedFoo | (ClosedBar & Any)

    if "bar" not in v:
        reveal_type(v)  # revealed: Never
    else:
        reveal_type(v)  # revealed: ClosedBar & Any

    if w not in u:
        reveal_type(u)  # revealed: ClosedFoo
    else:
        # TODO: should be `ClosedBar & Any`
        reveal_type(u)  # revealed: ClosedFoo | (ClosedBar & Any)
```

## Narrowing tagged unions of `TypedDict`s with `match` statements

Just like with `if` statements, we can narrow tagged unions of `TypedDict`s in `match` statements:

```toml
[environment]
python-version = "3.10"
```

```py
from typing import TypedDict, Literal

class Foo(TypedDict):
    tag: Literal["foo"]

class Bar(TypedDict):
    tag: Literal[42]

class Baz(TypedDict):
    tag: Literal[b"baz"]

class Bing(TypedDict):
    tag: Literal["bing"]

def match_statements(u: Foo | Bar | Baz | Bing):
    match u["tag"]:
        case "foo":
            reveal_type(u)  # revealed: Foo
        case 42:
            reveal_type(u)  # revealed: Bar
        case b"baz":
            reveal_type(u)  # revealed: Baz
        case _:
            reveal_type(u)  # revealed: Bing
```

We can also narrow a single `TypedDict` type to `Never`:

```py
def match_single(u: Foo):
    match u["tag"]:
        case "foo":
            reveal_type(u)  # revealed: Foo
        case _:
            reveal_type(u)  # revealed: Never
```

Narrowing is restricted to `Literal` tags:

```py
from ty_extensions import is_assignable_to, static_assert

class NonLiteralTD(TypedDict):
    tag: int

def match_non_literal(u: Foo | NonLiteralTD):
    match u["tag"]:
        case "foo":
            # We can't narrow the union here...
            reveal_type(u)  # revealed: Foo | NonLiteralTD
        case _:
            # ...(but we *can* narrow here)...
            reveal_type(u)  # revealed: NonLiteralTD
```

and it is also restricted to `match` patterns that solely consist of value patterns:

```py
class Config:
    MODE: str = "default"

class Foo(TypedDict):
    tag: Literal["foo"]
    data: int

class Bar(TypedDict):
    tag: Literal["bar"]
    data: str

def test_or_pattern_with_non_literal(u: Foo | Bar):
    match u["tag"]:
        case Config.MODE | "foo":
            # Config.mode has type `str` (not a literal), which could match
            # any string value at runtime. We cannot narrow based on "foo" alone
            # because the actual match might have been against Config.mode.
            reveal_type(u)  # revealed: Foo | Bar
        case "bar":
            # Since the previous case could match any string, this case can
            # still narrow to `Bar` when tag equals "bar".
            reveal_type(u)  # revealed: Bar
```

We can still narrow `Literal` tags even when non-`TypedDict` types are present in the union:

```py
def match_with_dict(u: Foo | Bar | dict):
    match u["tag"]:
        case "foo":
            # TODO: `dict & ~<TypedDict ...>` should simplify to `dict` here, but that's currently a
            # false negative in `is_disjoint_impl`.
            reveal_type(u)  # revealed: Foo | (dict[Unknown, Unknown] & ~<TypedDict with items 'tag'>)
```

## Narrowing tagged unions of `TypedDict`s from PEP 695 type aliases

PEP 695 type aliases are transparently resolved when narrowing tagged unions:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import TypedDict, Literal

class Foo(TypedDict):
    tag: Literal["foo"]

class Bar(TypedDict):
    tag: Literal["bar"]

type Thing = Foo | Bar

def test_if(x: Thing):
    if x["tag"] == "foo":
        reveal_type(x)  # revealed: Foo
    else:
        reveal_type(x)  # revealed: Bar
```

PEP 695 type aliases also work in `match` statements:

```py
def test_match(x: Thing):
    match x["tag"]:
        case "foo":
            reveal_type(x)  # revealed: Foo
        case "bar":
            reveal_type(x)  # revealed: Bar
```

PEP 695 type aliases also work with `in`/`not in` narrowing:

```py
class Baz(TypedDict):
    baz: int

type ThingWithBaz = Foo | Baz

def test_in(x: ThingWithBaz):
    if "baz" not in x:
        reveal_type(x)  # revealed: Foo
    else:
        reveal_type(x)  # revealed: Foo | Baz
```

Nested PEP 695 type aliases (an alias referring to another alias) also work:

```py
type Inner = Foo | Bar
type Outer = Inner

def test_nested_if(x: Outer):
    if x["tag"] == "foo":
        reveal_type(x)  # revealed: Foo
    else:
        reveal_type(x)  # revealed: Bar

def test_nested_match(x: Outer):
    match x["tag"]:
        case "foo":
            reveal_type(x)  # revealed: Foo
        case "bar":
            reveal_type(x)  # revealed: Bar

type InnerWithBaz = Foo | Baz
type OuterWithBaz = InnerWithBaz

def test_nested_in(x: OuterWithBaz):
    if "baz" not in x:
        reveal_type(x)  # revealed: Foo
    else:
        reveal_type(x)  # revealed: Foo | Baz
```

## Only annotated declarations are allowed in the class body

<!-- snapshot-diagnostics -->

`TypedDict` class bodies are very restricted in what kinds of statements they can contain. Besides
annotated items, the only allowed statements are docstrings and `pass`. Annotated items are are also
not allowed to have a value.

```py
from typing import TypedDict

class Foo(TypedDict):
    """docstring"""

    annotated_item: int
    """attribute docstring"""

    pass

    # As a non-standard but common extension, we interpret `...` as equivalent to `pass`.
    ...

class Bar(TypedDict):
    a: int
    # error: [invalid-typed-dict-statement] "invalid statement in TypedDict class body"
    42
    # error: [invalid-typed-dict-statement] "TypedDict item cannot have a value"
    b: str = "hello"
    # error: [invalid-typed-dict-statement] "TypedDict class cannot have methods"
    def bar(self): ...
```

These rules are also enforced for `TypedDict` classes that don't directly inherit from `TypedDict`:

```py
class Baz(Bar):
    # error: [invalid-typed-dict-statement]
    def baz(self):
        pass
```

## Conditional fields in class body

Conditional branches in a `TypedDict` body can declare fields. Static reachability determines
whether those fields are part of the schema.

### Python 3.12 or later

```toml
[environment]
python-version = "3.12"
```

```py
import sys
from typing import TypedDict

class ConditionalField(TypedDict):
    x: int
    if sys.version_info >= (3, 12):
        y: str

ConditionalField(x=1, y="hello")
```

### Python before 3.12

```toml
[environment]
python-version = "3.11"
```

```py
import sys
from typing import TypedDict

class ConditionalField(TypedDict):
    x: int
    if sys.version_info >= (3, 12):
        y: str

# error: [invalid-key] "Unknown key "y" for TypedDict `ConditionalField`"
ConditionalField(x=1, y="hello")
```

## `TypedDict` with `@dataclass` decorator

Applying `@dataclass` to a `TypedDict` class is conceptually incoherent: `TypedDict` defines
abstract structural types where "instantiating" always gives you a plain `dict` at runtime, whereas
`@dataclass` is a tool for customising the creation of new nominal types. An exception may be raised
when instantiating the class at runtime:

```py
from dataclasses import dataclass
from typing import TypedDict

@dataclass
# error: [invalid-dataclass] "`TypedDict` class `Foo` cannot be decorated with `@dataclass`"
class Foo(TypedDict):
    x: int
    y: str
```

The same error occurs with `dataclasses.dataclass` used with parentheses:

```py
from dataclasses import dataclass
from typing import TypedDict

@dataclass()
# error: [invalid-dataclass]
class Bar(TypedDict):
    x: int
```

It also applies when using `frozen=True` or other dataclass parameters:

```py
from dataclasses import dataclass
from typing import TypedDict

@dataclass(frozen=True)
# error: [invalid-dataclass]
class Baz(TypedDict):
    x: int
```

Classes that inherit from a `TypedDict` subclass (indirectly inheriting from `TypedDict`) are also
TypedDict classes and cannot be decorated with `@dataclass`:

```py
from dataclasses import dataclass
from typing import TypedDict

class Base(TypedDict):
    x: int

@dataclass
# error: [invalid-dataclass]
class Child(Base):
    y: str
```

The functional `TypedDict` syntax also triggers this error:

```py
from dataclasses import dataclass
from typing import TypedDict

@dataclass
# error: [invalid-dataclass]
class Foo(TypedDict("Foo", {"x": int, "y": str})):
    pass
```

## Class header validation

<!-- snapshot-diagnostics -->

A `TypedDict` may not inherit from a non-`TypedDict`:

```py
from typing import TypedDict

class Foo(TypedDict, int): ...  # error: [invalid-typed-dict-header]

# This even fails at runtime!
class Foo2(TypedDict, object): ...  # error: [invalid-typed-dict-header]
```

It is invalid to pass non-`bool`s to the `total` and `closed` keyword arguments:

```py
class Bar(TypedDict, total=42): ...  # error: [invalid-argument-type]
class Baz(TypedDict, closed=None): ...  # error: [invalid-argument-type]
```

And it's also invalid to pass an object of type `bool` -- according to the spec:

> The value must be exactly `True` or `False`; other expressions are not allowed.

```py
def f(is_total: bool):
    class VeryDynamic(TypedDict, total=is_total): ...  # error: [invalid-argument-type]
```

Unknown keyword arguments are detected:

```py
class Bazzzz(TypedDict, weird=56): ...  # error: [unknown-argument]
```

Specifying a custom metaclass is not permitted:

```py
from abc import ABCMeta

class Spam(TypedDict, metaclass=ABCMeta): ...  # error: [invalid-typed-dict-header]

# This one works at runtime, but the metaclass is still `typing._TypedDictMeta`,
# so there doesn't seem to be any reason why you'd want to do this
class Ham(TypedDict, metaclass=type): ...  # error: [invalid-typed-dict-header]
```

And variadic keywords are also banned:

```py
def f(kwargs: dict):
    class Eggs(TypedDict, **kwargs): ...  # error: [invalid-typed-dict-header]
```

## PEP 728 (`closed` and `extra_items`)

### Iterating keys, values and items of a `closed=True` `TypedDict`

Iterating over the keys produces a `Literal` type; iterating the values produces a union of all the
value types.

```py
from typing_extensions import TypedDict

class Closed(TypedDict, closed=True):
    name: str
    age: int

def _(closed: Closed) -> None:
    # TODO: should be `dict_keys[Literal["name", "age"], str | int]`
    reveal_type(closed.keys())  # revealed: dict_keys[str, object]

    # TODO: should be `dict_values[Literal["name", "age"], str | int]`
    reveal_type(closed.values())  # revealed: dict_values[str, object]

    # TODO: should be `dict_items[Literal["name", "age"], str | int]`
    reveal_type(closed.items())  # revealed: dict_items[str, object]

    # iterating over the keys gives `Literal` types
    for key in closed:
        # TODO: should be `Literal["name", "age"]`
        reveal_type(key)  # revealed: str

    for key in closed.keys():
        # TODO: should be `Literal["name", "age"]`
        reveal_type(key)  # revealed: str

    for value in closed.values():
        # TODO: should be `str | int
        reveal_type(value)  # revealed: object

    for item in closed.items():
        # TODO: should be `tuple[Literal["name"], str] | tuple[Literal["age"], int]`
        reveal_type(item)  # revealed: tuple[str, object]
```

### Iterating keys, values and items of an extra-items TypedDict

For an extra-items `TypedDict`, iteraitng over the keys only gives you a `str`, because there may be
arbitrary additional keys in the mapping. Iterating over the values gives you a union of all known
value types and the `extra_items` type.

```py
from typing_extensions import TypedDict

class Extra(TypedDict, extra_items=int):
    name: str

def _(extra: Extra) -> None:
    # TODO: should be `dict_keys[str, str | int]`
    reveal_type(extra.keys())  # revealed: dict_keys[str, object]

    # TODO: should be `dict_values[str, str | int]`
    reveal_type(extra.values())  # revealed: dict_values[str, object]

    # TODO: should be `dict_items[str, str | int]`
    reveal_type(extra.items())  # revealed: dict_items[str, object]

    # iterating over the keys gives `str` types
    for key in extra:
        reveal_type(key)  # revealed: str

    for key in extra.keys():
        reveal_type(key)  # revealed: str

    for value in extra.values():
        # TODO: should be `str | int
        reveal_type(value)  # revealed: object

    for item in extra.items():
        # TODO: should be `tuple[str, str | int]`
        reveal_type(item)  # revealed: tuple[str, object]
```

### A closed `TypedDict` is equivalent to `extra_items=Never`

```py
from typing_extensions import TypedDict, Never
from ty_extensions import static_assert, is_equivalent_to, is_subtype_of

class Extra(TypedDict, extra_items=Never):
    x: int

class Closed(TypedDict, closed=True):
    x: int

static_assert(is_equivalent_to(Extra, Closed))
static_assert(is_subtype_of(Extra, Closed))
static_assert(is_subtype_of(Closed, Extra))
```

### Empty closed TypedDict is known to be falsy

An empty `closed=True` TypedDict cannot contain any keys, so it is always empty and always falsy.

```py
from typing_extensions import TypedDict

class Empty(TypedDict, closed=True): ...

def _(empty: Empty) -> None:
    # TODO: should be `Literal[False]`
    reveal_type(bool(empty))  # revealed: bool
```

### Closed TypedDict is structurally final but not nominally final

A [closed] TypedDict can be subclassed. However, subclasses cannot add new keys, so any subclasses
are equivalent in terms of the type they define. `closed=True` TypedDicts can therefore be thought
of as "structurally final" even if they are not nominally final.

```py
from typing_extensions import TypedDict
from ty_extensions import static_assert, is_equivalent_to

class Closed(TypedDict, closed=True):
    name: str

# OK: no new keys
class ClosedChild(Closed): ...

static_assert(is_equivalent_to(ClosedChild, Closed))

# TODO: should be error: [invalid-typed-dict-header] "Cannot add new items to a closed TypedDict"
class BadChild(Closed):
    age: int
```

### Indexing into extra-items TypedDict with `str` key is allowed and returns extra-items type

For an open (non-closed, non-extra-items) TypedDict, indexing with a non-literal `str` is an error.
But for extra-items TypedDicts, the value type is known for arbitrary string keys.

```py
from typing_extensions import TypedDict

class Extra(TypedDict, extra_items=int):
    name: str

def _(extra: Extra, key: str) -> None:
    reveal_type(extra["name"])  # revealed: str
    # TODO: should be `int` (the extra_items type) with no error
    # error: [invalid-key]
    reveal_type(extra["anything"])  # revealed: Unknown
    # TODO: should be `str | int` with no error
    # error: [invalid-key]
    reveal_type(extra[key])  # revealed: Unknown
```

For closed TypedDicts, indexing into the dictionary with a non-literal `str` is an error, just like
with an open TypedDict. Nonetheless, unlike with an open TypedDict, the revealed type can safely be
inferred as the union of all the value types:

```py
class Closed(TypedDict, closed=True):
    name: str
    age: int

def _(td: Closed, key: str) -> None:
    # TODO: the error is correct, but this could validly be `str | int`
    # error: [invalid-key]
    reveal_type(td[key])  # revealed: Unknown
```

### Subclass of extra-items TypedDict has the same extra-items type as its base

```py
from typing_extensions import TypedDict, NotRequired

class Base(TypedDict, extra_items=int):
    name: str

class Child(Base):
    age: NotRequired[int]

# Child inherits extra_items=int from Base
def _(child: Child) -> None:
    reveal_type(child["name"])  # revealed: str
    reveal_type(child["age"])  # revealed: int
    # TODO: should be `int` (inherited extra_items) with no error
    # error: [invalid-key]
    reveal_type(child["other"])  # revealed: Unknown
```

### `closed=False` TypedDict cannot inherit from an `extra_items` TypedDict

Explicitly setting `closed=False` on a subclass of an `extra_items` TypedDict (or a `closed=True`
TypedDict) is an error:

```py
from typing_extensions import TypedDict

class ExtraBase(TypedDict, extra_items=int):
    name: str

# TODO: should be error: [invalid-typed-dict-header]
class BadChild1(ExtraBase, closed=False): ...

class ClosedBase(TypedDict, closed=True):
    name: str

# TODO: should be error: [invalid-typed-dict-header]
class BadChild2(ClosedBase, closed=False): ...
```

But `closed=False` on a subclass of an open TypedDict is fine (it's the default):

```py
class OpenBase(TypedDict, closed=False):
    name: str

class OkChild(OpenBase):
    age: int

class ExplicitOkChild(OpenBase, closed=False):
    age: int
```

and `closed=True` on a subclass of an open TypedDict is also fine:

```py
class ClosedChild(OpenBase, closed=True): ...
class ClosedChild2(ExplicitOkChild, closed=True): ...
```

### Extra-items TypedDict can be initialized with additional keys (via literal or constructor), but values must be of the correct type

```py
from typing_extensions import TypedDict

class Movie(TypedDict, extra_items=bool):
    name: str

# TODO: should be OK (extra key with correct type), no errors
a: Movie = {"name": "Blade Runner", "novel_adaptation": True}  # error: [invalid-key]
Movie(name="Blade Runner", novel_adaptation=True)  # error: [invalid-key]

# TODO: should be error: [invalid-argument-type] (wrong type for extra key), not [invalid-key]
b: Movie = {"name": "Blade Runner", "year": 1982}  # error: [invalid-key]

# TODO: should be error: [invalid-argument-type], not [invalid-key]
Movie(name="Blade Runner", year=1982)  # error: [invalid-key]

# Closed TypedDicts reject extra keys entirely
class ClosedMovie(TypedDict, closed=True):
    name: str

# error: [invalid-key] "Unknown key "year" for TypedDict `ClosedMovie`"
c: ClosedMovie = {"name": "Blade Runner", "year": 1982}

# error: [invalid-key]
ClosedMovie(name="Blade Runner", year=1982)
```

The functional syntax also supports `extra_items`:

```py
MovieFunctional = TypedDict("MovieFunctional", {"name": str}, extra_items=bool)

# TODO: should be OK (extra key with correct type), no errors
d: MovieFunctional = {"name": "Blade Runner", "novel_adaptation": True}  # error: [invalid-key]

# TODO: should be error: [invalid-argument-type] (wrong type for extra key), not [invalid-key]
e: MovieFunctional = {"name": "Blade Runner", "year": 1982}  # error: [invalid-key]
```

### `extra_items` parameter must be a valid annotation expression; the only legal type qualifier is `ReadOnly`

`Required` and `NotRequired` are not valid qualifiers for `extra_items`, since extra items are
always implicitly non-required.

```py
from typing_extensions import TypedDict, ReadOnly, Required, NotRequired, ClassVar, Final
from dataclasses import InitVar

# OK
class A(TypedDict, extra_items=int):
    name: str

# OK: ReadOnly is allowed
class B(TypedDict, extra_items=ReadOnly[int]):
    name: str

# error: [invalid-type-form] "Type qualifier `typing.Required` is not valid in a TypedDict `extra_items` argument"
class C(TypedDict, extra_items=Required[int]):
    name: str

# error: [invalid-type-form] "Type qualifier `typing.NotRequired` is not valid in a TypedDict `extra_items` argument"
class D(TypedDict, extra_items=NotRequired[int]):
    name: str

# error: [invalid-type-form] "Type qualifier `typing.ClassVar` is not valid in a TypedDict `extra_items` argument"
class D(TypedDict, extra_items=ClassVar[int]):
    name: str

# error: [invalid-type-form] "Type qualifier `typing.Final` is not valid in a TypedDict `extra_items` argument"
class D(TypedDict, extra_items=Final[int]):
    name: str

# error: [invalid-type-form] "Type qualifier `dataclasses.InitVar` is not valid in a TypedDict `extra_items` argument"
class D(TypedDict, extra_items=InitVar[int]):
    name: str
```

It is an error to specify both `closed` and `extra_items`:

```py
# TODO: should be error: [invalid-typed-dict-header]
class E(TypedDict, closed=True, extra_items=int):
    name: str
```

### Forward references in `extra_items`

Stringified forward references are understood:

`a.py`:

```py
from typing import TypedDict

class F(TypedDict, extra_items="F | None"): ...
```

While invalid syntax in forward annotations is rejected:

`b.py`:

```py
from typing import TypedDict

# error: [invalid-syntax-in-forward-annotation]
class G(TypedDict, extra_items="not a type expression"): ...
```

In non-stub files, forward references in `extra_items` must be stringified:

`c.py`:

```py
from typing import TypedDict

# error: [unresolved-reference] "Name `H` used when not defined"
class H(TypedDict, extra_items=H | None): ...
```

but stringification is unnecessary in stubs:

`stub.pyi`:

```pyi
from typing import TypedDict

class I(TypedDict, extra_items=I | None): ...
```

The `extra_items` keyword is not parsed as an annotation expression for non-TypedDict classes:

`d.py`:

```py
class TypedDict:  # not typing.TypedDict!
    def __init_subclass__(cls, extra_items: int): ...

class Foo(TypedDict, extra_items=42): ...  # fine
class Bar(TypedDict, extra_items=int): ...  # error: [invalid-argument-type]
```

### Writing to an undeclared literal key of an `extra_items` TypedDict is allowed, if the type is assignable

```py
from typing_extensions import TypedDict

class Extra(TypedDict, extra_items=int):
    name: str

def _(extra: Extra) -> None:
    # TODO: should be OK (int is assignable to extra_items=int), no error
    extra["year"] = 1982  # error: [invalid-key]
    extra["name"] = "Alien"  # OK: str is assignable to str

    # TODO: should be error: [invalid-assignment], not [invalid-key]
    extra["year"] = "not an int"  # error: [invalid-key]
```

### If `extra_items` is `ReadOnly`, you can't write to an undeclared literal string key

```py
from typing_extensions import TypedDict, ReadOnly

class ReadOnlyExtra(TypedDict, extra_items=ReadOnly[int]):
    name: str

def _(read_only_extra: ReadOnlyExtra) -> None:
    read_only_extra["name"] = "Alien"  # OK: name is a declared mutable field

    # TODO: should be error: [invalid-assignment] "Cannot assign to key "year" on TypedDict `ReadOnlyExtra`: key is marked read-only"
    read_only_extra["year"] = 1982  # error: [invalid-key]
```

### Writing to a `str` key on an `extra_items` TypedDict is only allowed if the type is assignable to all TypedDict items

```py
from typing_extensions import TypedDict

class Super: ...
class Sub(Super): ...

class Extra1(TypedDict, extra_items=Sub):
    field: Super

class Extra2(TypedDict, extra_items=Super):
    field: Sub

def _(extra1: Extra1, extra2: Extra2, key: str) -> None:
    # TODO: the error message is wrong: `Super` is assignable to the value-type of `field`, but not to `extra_items`
    #
    # error: [invalid-key] "TypedDict `Extra1` can only be subscripted with a string literal key, got key of type `str`."
    extra1[key] = Super()

    # TODO: the error message is wrong: `Super` is assignable to `extra_items`, but not to the value type of `field`
    #
    # error: [invalid-key] "TypedDict `Extra2` can only be subscripted with a string literal key, got key of type `str`."
    extra2[key] = Super()

    # TODO: these should be fine
    extra1[key] = Sub()  # error: [invalid-key]
    extra2[key] = Sub()  # error: [invalid-key]
```

### If `extra_items` is `ReadOnly`, subclasses can override the type covariantly, and/or have mutable `extra_items`

When a base has `ReadOnly` extra items, a subclass may narrow the extra-items type covariantly or
switch to mutable extra items (as long as the mutable type is assignable to the base's read-only
type).

```py
from typing_extensions import TypedDict, ReadOnly

class ReadOnlyBase(TypedDict, extra_items=ReadOnly[int | str]):
    name: str

# OK: narrow ReadOnly extra_items covariantly
class NarrowerChild(ReadOnlyBase, extra_items=ReadOnly[int]): ...

# OK: switch from read-only to mutable, with assignable type
class MutableChild(ReadOnlyBase, extra_items=int): ...

# OK: close the subclass (only allowed when base extra_items is read-only)
class ClosedChild(ReadOnlyBase, closed=True): ...

# TODO: should be error: [invalid-typed-dict-header] "'list[str]' is not assignable to 'int | str'"
class BadChild(ReadOnlyBase, extra_items=list[str]): ...
```

When the base has _mutable_ extra items, the child cannot change the extra-items type:

```py
class MutableBase(TypedDict, extra_items=int):
    name: str

# TODO: should be error: [invalid-typed-dict-header]
class BadNarrow(MutableBase, extra_items=bool): ...

# TODO: should be error: [invalid-typed-dict-header]
class BadClose(MutableBase, closed=True): ...
```

### A subclass of a TypedDict with mutable `extra_items: T` may only add non-required items consistent with `T`

```py
from typing_extensions import TypedDict, NotRequired

class Base(TypedDict, extra_items=int | None):
    name: str

# OK: non-required, type consistent with int | None
class GoodChild(Base):
    year: NotRequired[int | None]

# TODO: should be error: [invalid-typed-dict-header] "Required key 'year' is not allowed"
class ChildWithBadRequiredItem(Base):
    year: int | None

# TODO: should be error: [invalid-typed-dict-header] "Type 'int' is not consistent with 'int | None'"
class ChildWithBadValueType(Base):
    year: NotRequired[int]
```

### A subclass of a TypedDict with read-only `extra_items: T` may add required or non-required items assignable to `T`

```py
from typing_extensions import TypedDict, ReadOnly, NotRequired

class Base(TypedDict, extra_items=ReadOnly[int | str]):
    name: str

# OK: required key with type assignable to int | str
class WithYear(Base):
    year: int

# OK: non-required key with type assignable to int | str
class WithTag(Base):
    tag: NotRequired[str]

# TODO: should be error: [invalid-typed-dict-header] "'list[str]' is not assignable to 'int | str'"
class BadChild(Base):
    tags: list[str]
```

### Deleting extra items is permitted

Extra items are implicitly non-required, so deletion is allowed for unknown keys if they have
literal types. Deletion of declared required keys and keys of type `str` is still an error.

```py
from typing_extensions import TypedDict

class Extra(TypedDict, extra_items=int):
    name: str

def _(extra: Extra, key: str) -> None:
    # TODO: should be OK (extra items are non-required)
    del extra["year"]  # error: [invalid-argument-type]

    # error: [invalid-argument-type] "Cannot delete required key "name" from TypedDict `Extra`"
    del extra["name"]

    # TODO: not the best error message...
    #
    # error: [invalid-argument-type] "Method `__delitem__` of type `(key: Never, /) -> None` cannot be called with key of type `str` on object of type `Extra`"
    del extra[key]
```

### Assignability between TypedDicts accounts for the type of extra items

```py
from typing_extensions import TypedDict, ReadOnly, NotRequired
from ty_extensions import static_assert, is_assignable_to, is_subtype_of

class ExtraInt(TypedDict, extra_items=int):
    name: str

class ExtraStr(TypedDict, extra_items=str):
    name: str

# Mutable extra items must be equivalent, not just assignable
#
# TODO: these should pass
static_assert(not is_assignable_to(ExtraInt, ExtraStr))  # error: [static-assert-error]
static_assert(not is_assignable_to(ExtraStr, ExtraInt))  # error: [static-assert-error]

class ReadOnlyExtraInt(TypedDict, extra_items=ReadOnly[int]):
    name: str

class ReadOnlyExtraIntStr(TypedDict, extra_items=ReadOnly[int | str]):
    name: str

# Read-only extra items: covariant, so narrower is assignable to wider
static_assert(is_subtype_of(ReadOnlyExtraInt, ReadOnlyExtraIntStr))
# TODO: should pass
static_assert(not is_assignable_to(ReadOnlyExtraIntStr, ReadOnlyExtraInt))  # error: [static-assert-error]

# A closed TypedDict is assignable to an open one (open implicitly has ReadOnly[object] extras)
class Closed(TypedDict, closed=True):
    name: str

class Open(TypedDict):
    name: str

static_assert(is_assignable_to(Closed, Open))

# An open TypedDict is not assignable to a closed one (might have extra keys)
#
# TODO: should pass
static_assert(not is_assignable_to(Open, Closed))  # error: [static-assert-error]

# An extra-items TypedDict is assignable to an open one
static_assert(is_assignable_to(ExtraInt, Open))

# But not vice versa
#
# TODO: should pass
static_assert(not is_assignable_to(Open, ExtraInt))  # error: [static-assert-error]
```

Non-required items in the target that are absent in the source must be accounted for by the source's
extra-items type:

```py
class Target(TypedDict):
    name: str
    age: NotRequired[ReadOnly[int]]

class SourceWithIntExtra(TypedDict, extra_items=int):
    name: str

# SourceExtra can satisfy `Target`'s non-required `ReadOnly` `age` via its `extra_items=int`
#
# TODO: should pass
static_assert(is_assignable_to(SourceWithIntExtra, Target))  # error: [static-assert-error]

class SourceWithStrExtra(TypedDict, extra_items=str):
    name: str

# `str` extra items can't satisfy `age: int`
static_assert(not is_assignable_to(SourceWithStrExtra, Target))
```

### A `TypedDict` with `extra_items: T` is a subtype of `Mapping[str, T1]`, where `T1` is the union of `T` and all declared item types

```py
from collections.abc import Mapping
from typing_extensions import TypedDict
from ty_extensions import static_assert, is_assignable_to

class ExtraStr(TypedDict, extra_items=str):
    name: str

# All value types (str, str) are subtypes of str
#
# TODO: should pass
static_assert(is_assignable_to(ExtraStr, Mapping[str, str]))  # error: [static-assert-error]

class ExtraInt(TypedDict, extra_items=int):
    name: str

# Value types are str | int, so it's assignable to Mapping[str, str | int] but not Mapping[str, int]
#
# TODO: should pass
static_assert(is_assignable_to(ExtraInt, Mapping[str, str | int]))  # error: [static-assert-error]
static_assert(not is_assignable_to(ExtraInt, Mapping[str, int]))

# Closed TypedDicts also have a known set of value types
class Closed(TypedDict, closed=True):
    name: str
    age: int

# TODO: should pass
static_assert(is_assignable_to(Closed, Mapping[str, str | int]))  # error: [static-assert-error]
static_assert(not is_assignable_to(Closed, Mapping[str, str]))
```

### A `TypedDict` with all not-required and not-readonly items is a subtype of `dict[str, VT]` if all keys are equivalent to `VT`

A call to the `.clear()` method is allowed on such a `TypedDict` type, as is arbitrary deletion of
keys. The reverse is not true, however. `dict[str, VT]` is not assignable to such a `TypedDict`
type, as an inhabitant of this type might be an instance of a subclass of `dict`.

```py
from typing_extensions import TypedDict, NotRequired
from ty_extensions import static_assert, is_subtype_of, is_assignable_to, is_equivalent_to

class IntDict(TypedDict, extra_items=int): ...

class IntDictWithNum(IntDict):
    num: NotRequired[int]

# All items non-required + mutable + extra_items=int → assignable to dict[str, int]
#
# TODO: these should pass
static_assert(is_subtype_of(IntDict, dict[str, int]))  # error: [static-assert-error]
static_assert(is_subtype_of(IntDictWithNum, dict[str, int]))  # error: [static-assert-error]

# But dict[str, int] is not assignable to the TypedDict (could be a dict subclass)
static_assert(not is_assignable_to(dict[str, int], IntDict))
static_assert(not is_equivalent_to(dict[str, int], IntDict))

def _(int_dict_with_num: IntDictWithNum, key: str) -> None:
    # TODO: no errors should be reported here
    v: dict[str, int] = int_dict_with_num  # error: [invalid-assignment]
    int_dict_with_num.clear()  # error: [unresolved-attribute]
    # error: [unresolved-attribute]
    reveal_type(int_dict_with_num.popitem())  # revealed: Unknown
    int_dict_with_num[key] = 42  # error: [invalid-key]
    del int_dict_with_num[key]  # error: [invalid-argument-type]

class BoolDictWithNum(IntDict, extra_items=int):
    condition: NotRequired[bool]

# All keys must be equivalent to the value-type of the dict in order for
# assignability to hold:
static_assert(not is_assignable_to(BoolDictWithNum, dict[str, int]))
static_assert(not is_subtype_of(BoolDictWithNum, dict[str, int]))
static_assert(not is_equivalent_to(BoolDictWithNum, dict[str, int]))
```

A TypedDict with a required key is not assignable to `dict[str, VT]`:

```py
class HasRequired(TypedDict, extra_items=int):
    name: str

static_assert(not is_assignable_to(HasRequired, dict[str, int]))
static_assert(not is_assignable_to(HasRequired, dict[str, str | int]))
```

A TypedDict with a read-only item is not assignable to `dict[str, VT]`:

```py
from typing_extensions import ReadOnly

class HasReadOnly(TypedDict, extra_items=int):
    x: NotRequired[ReadOnly[int]]

static_assert(not is_assignable_to(HasReadOnly, dict[str, int]))
```

[closed]: https://peps.python.org/pep-0728/#disallowing-extra-items-explicitly
[subtyping section]: https://typing.python.org/en/latest/spec/typeddict.html#subtyping-between-typeddict-types
[`typeddict`]: https://typing.python.org/en/latest/spec/typeddict.html
