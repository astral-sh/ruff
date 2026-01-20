# `NamedTuple`

`NamedTuple` is a type-safe way to define named tuples — a tuple where each field can be accessed by
name, and not just by its numeric position within the tuple:

## `typing.NamedTuple`

### Basics

```py
from typing import NamedTuple, Sequence
from ty_extensions import static_assert, is_subtype_of, is_assignable_to, reveal_mro

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

# revealed: (<class 'Person'>, <class 'tuple[int, str, int | None]'>, <class 'Sequence[int | str | None]'>, <class 'Reversible[int | str | None]'>, <class 'Collection[int | str | None]'>, <class 'Iterable[int | str | None]'>, <class 'Container[int | str | None]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Person)

static_assert(is_subtype_of(Person, tuple[int, str, int | None]))
static_assert(is_subtype_of(Person, tuple[object, ...]))
static_assert(is_subtype_of(Person, Sequence[int | str | None]))
static_assert(is_subtype_of(Person, Sequence[object]))
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

reveal_type(Person.id)  # revealed: property
reveal_type(Person.name)  # revealed: property
reveal_type(Person.age)  # revealed: property

# error: [invalid-assignment] "Cannot assign to read-only property `id` on object of type `Person`"
alice.id = 42
# error: [invalid-assignment]
bob.age = None
```

Alternative functional syntax with a list of tuples:

```py
Person2 = NamedTuple("Person", [("id", int), ("name", str)])
alice2 = Person2(1, "Alice")

# error: [missing-argument]
Person2(1)

reveal_type(alice2.id)  # revealed: int
reveal_type(alice2.name)  # revealed: str
```

Functional syntax with a tuple of tuples:

```py
Person3 = NamedTuple("Person", (("id", int), ("name", str)))
alice3 = Person3(1, "Alice")

reveal_type(alice3.id)  # revealed: int
reveal_type(alice3.name)  # revealed: str
```

Functional syntax with a tuple of lists:

```py
Person4 = NamedTuple("Person", (["id", int], ["name", str]))
alice4 = Person4(1, "Alice")

reveal_type(alice4.id)  # revealed: int
reveal_type(alice4.name)  # revealed: str
```

Functional syntax with a list of lists:

```py
Person5 = NamedTuple("Person", [["id", int], ["name", str]])
alice5 = Person5(1, "Alice")

reveal_type(alice5.id)  # revealed: int
reveal_type(alice5.name)  # revealed: str
```

### Functional syntax with string annotations

String annotations (forward references) are properly evaluated to types:

```py
from typing import NamedTuple

Point = NamedTuple("Point", [("x", "int"), ("y", "int")])
p = Point(1, 2)

reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int
```

Recursive references in functional syntax are supported:

```py
from typing import NamedTuple

Node = NamedTuple("Node", [("value", int), ("next", "Node | None")])
n = Node(1, None)

reveal_type(n.value)  # revealed: int
reveal_type(n.next)  # revealed: Node | None

A = NamedTuple("A", [("x", "B | None")])
B = NamedTuple("B", [("x", "C")])
C = NamedTuple("C", [("x", A)])

a = A(x=B(x=C(x=A(x=None))))

reveal_type(a.x)  # revealed: B | None

if a.x:
    reveal_type(a.x and a.x.x)  # revealed: C
    reveal_type(a.x and a.x.x.x)  # revealed: A
    reveal_type(a.x and a.x.x.x.x)  # revealed: B | None

A(x=42)  # error: [invalid-argument-type]

# error: [invalid-argument-type]
# error: [missing-argument]
A(x=C())

# error: [invalid-argument-type]
A(x=C(x=A(x=None)))
```

### Functional syntax as base class (dangling call)

When `NamedTuple` is used directly as a base class without being assigned to a variable first, it's
a "dangling call". The types are still properly inferred:

```py
from typing import NamedTuple

class Point(NamedTuple("Point", [("x", int), ("y", int)])):
    def magnitude(self) -> float:
        return (self.x**2 + self.y**2) ** 0.5

p = Point(3, 4)
reveal_type(p.x)  # revealed: int
reveal_type(p.y)  # revealed: int
reveal_type(p.magnitude())  # revealed: int | float
```

String annotations in dangling calls work correctly for forward references to classes defined in the
same scope. This allows recursive types:

```py
from typing import NamedTuple

class Node(NamedTuple("Node", [("value", int), ("next", "Node | None")])):
    pass

n = Node(1, None)
reveal_type(n.value)  # revealed: int
reveal_type(n.next)  # revealed: Node | None

class A(NamedTuple("A", [("x", "B | None")])): ...
class B(NamedTuple("B", [("x", "C")])): ...
class C(NamedTuple("C", [("x", "A")])): ...

reveal_type(A(x=B(x=C(x=A(x=None)))))  # revealed: A

# error: [invalid-argument-type] "Argument is incorrect: Expected `B | None`, found `C`"
# error: [missing-argument] "No argument provided for required parameter `x`"
A(x=C())

# error: [invalid-argument-type] "Argument is incorrect: Expected `B | None`, found `C`"
A(x=C(x=A(x=None)))
```

Note that the string annotation must reference a name that exists in scope. References to the
internal NamedTuple name (if different from the class name) won't work:

```py
from typing import NamedTuple

# The string "X" in "next"'s type refers to the internal name, not "BadNode", so it won't resolve:
#
# error: [unresolved-reference] "Name `X` used when not defined"
class BadNode(NamedTuple("X", [("value", int), ("next", "X | None")])):
    pass

n = BadNode(1, None)
reveal_type(n.value)  # revealed: int
# X is not in scope, so it resolves to Unknown; None is correctly resolved
reveal_type(n.next)  # revealed: Unknown | None
```

Dangling calls cannot contain other dangling calls; that's an invalid type form:

```py
from ty_extensions import reveal_mro

# error: [invalid-type-form]
class A(NamedTuple("B", [("x", NamedTuple("C", [("x", "A" | None)]))])):
    pass

# revealed: (<class 'A'>, <class 'B'>, <class 'tuple[Unknown]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(A)
```

### Functional syntax with variable name

When the typename is passed via a variable, we can extract it from the inferred literal string type:

```py
from typing import NamedTuple

name = "Person"
Person = NamedTuple(name, [("id", int), ("name", str)])

p = Person(1, "Alice")
reveal_type(p.id)  # revealed: int
reveal_type(p.name)  # revealed: str
```

### Functional syntax with tuple variable fields

When fields are passed via a tuple variable, we cannot extract the literal field names and types
from the inferred tuple type. We instead emit a diagnostic:

```py
from typing import NamedTuple
from ty_extensions import static_assert, is_subtype_of, reveal_mro

fields = (("host", str), ("port", int))
# error: [invalid-named-tuple] "Invalid argument to parameter `fields` of `NamedTuple()`: `fields` must be a literal list or tuple"
Url = NamedTuple("Url", fields)

url = Url("localhost", 8080)
reveal_type(url.host)  # revealed: Any
reveal_type(url.port)  # revealed: Any

generic_fields = (("items", list[int]), ("mapping", dict[str, bool]))
# error: [invalid-named-tuple] "Invalid argument to parameter `fields` of `NamedTuple()`: `fields` must be a literal list or tuple"
Container = NamedTuple("Container", generic_fields)
container = Container([1, 2, 3], {"a": True})
reveal_type(container.items)  # revealed: Any
reveal_type(container.mapping)  # revealed: Any

# revealed: (<class 'Url'>, <class 'tuple[Unknown, ...]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Url)

invalid_fields = (("x", 42),)  # 42 is not a valid type
# error: [invalid-named-tuple] "Invalid argument to parameter `fields` of `NamedTuple()`: `fields` must be a literal list or tuple"
InvalidNT = NamedTuple("InvalidNT", invalid_fields)
reveal_type(InvalidNT)  # revealed: <class 'InvalidNT'>

host, port = url
reveal_type(host)  # revealed: Unknown
reveal_type(port)  # revealed: Unknown

# fails at runtime but we can't detect that
(only_one,) = url

# will error at runtime, but we can't detect that
a, b, c = url

reveal_type(url[0])  # revealed: Unknown
reveal_type(url[1])  # revealed: Unknown

# will error at runtime, but we can't detect that
reveal_type(url[2])  # revealed: Unknown
```

### Functional syntax with Final variable field names

When field names are `Final` variables, they resolve to their literal string values:

```py
from typing import Final, NamedTuple

X: Final = "x"
Y: Final = "y"
N = NamedTuple("N", [(X, int), (Y, int)])

reveal_type(N(x=3, y=4).x)  # revealed: int
reveal_type(N(x=3, y=4).y)  # revealed: int

# error: [invalid-argument-type]
# error: [invalid-argument-type]
N(x="", y="")
```

### Functional syntax with variadic tuple fields

When fields are passed as a variadic tuple (e.g., `tuple[..., *tuple[T, ...]]`), we cannot determine
the exact field count statically. In this case, we fall back to unknown fields:

```toml
[environment]
python-version = "3.11"
```

```py
from typing import NamedTuple
from ty_extensions import reveal_mro

# Variadic tuple - we can't determine the exact fields statically.
def get_fields() -> tuple[tuple[str, type[int]], *tuple[tuple[str, type[str]], ...]]:
    return (("x", int), ("y", str))

fields = get_fields()
# error: [invalid-named-tuple] "Invalid argument to parameter `fields` of `NamedTuple()`: `fields` must be a literal list or tuple"
NT = NamedTuple("NT", fields)

# Fields are unknown, so attribute access returns Any and MRO has Unknown tuple.
reveal_type(NT)  # revealed: <class 'NT'>
# revealed: (<class 'NT'>, <class 'tuple[Unknown, ...]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(NT)
reveal_type(NT(1, "a").x)  # revealed: Any
```

Similarly for `collections.namedtuple`:

```py
import collections
from ty_extensions import reveal_mro

def get_field_names() -> tuple[str, *tuple[str, ...]]:
    return ("x", "y")

field_names = get_field_names()
NT = collections.namedtuple("NT", field_names)

# Fields are unknown, so attribute access returns Any and MRO has Unknown tuple.
reveal_type(NT)  # revealed: <class 'NT'>
# revealed: (<class 'NT'>, <class 'tuple[Unknown, ...]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(NT)
reveal_type(NT(1, 2).x)  # revealed: Any
```

### Class inheriting from functional NamedTuple

Classes can inherit from functional namedtuples. The constructor parameters and field types are
properly inherited:

```py
from typing import NamedTuple
from ty_extensions import reveal_mro

class Url(NamedTuple("Url", [("host", str), ("path", str)])):
    pass

reveal_type(Url)  # revealed: <class 'Url'>
# revealed: (<class 'mdtest_snippet.Url @ src/mdtest_snippet.py:4:7'>, <class 'mdtest_snippet.Url @ src/mdtest_snippet.py:4:11'>, <class 'tuple[str, str]'>, <class 'Sequence[str]'>, <class 'Reversible[str]'>, <class 'Collection[str]'>, <class 'Iterable[str]'>, <class 'Container[str]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Url)
reveal_type(Url.__new__)  # revealed: [Self](cls: type[Self], host: str, path: str) -> Self

# Constructor works with the inherited fields.
url = Url("example.com", "/path")
reveal_type(url)  # revealed: Url
reveal_type(url.host)  # revealed: str
reveal_type(url.path)  # revealed: str

# Error handling works correctly.
# error: [missing-argument]
Url("example.com")

# error: [too-many-positional-arguments]
Url("example.com", "/path", "extra")
```

Subclasses can add methods that use inherited fields:

```py
from typing import NamedTuple
from typing_extensions import Self

class Url(NamedTuple("Url", [("host", str), ("port", int)])):
    def with_port(self, port: int) -> Self:
        reveal_type(self.host)  # revealed: str
        reveal_type(self.port)  # revealed: int
        return self._replace(port=port)

url = Url("localhost", 8080)
reveal_type(url.with_port(9000))  # revealed: Url
```

For `class Foo(namedtuple("Foo", ...)): ...`, the inner call creates a namedtuple class, but the
outer class is just a regular class inheriting from it. This is equivalent to:

```py
class _Foo(NamedTuple): ...

class Foo(_Foo):  # Regular class, not a namedtuple
    ...
```

Because the outer class is not itself a namedtuple, it can use `super()` and override `__new__`:

```py
from collections import namedtuple
from typing import NamedTuple

class ExtType(namedtuple("ExtType", "code data")):
    """Override __new__ to add validation."""

    def __new__(cls, code, data):
        if not isinstance(code, int):
            raise TypeError("code must be int")
        return super().__new__(cls, code, data)

class Url(NamedTuple("Url", [("host", str), ("path", str)])):
    """Override __new__ to normalize the path."""

    def __new__(cls, host, path):
        if path and not path.startswith("/"):
            path = "/" + path
        return super().__new__(cls, host, path)

# Both work correctly.
ext = ExtType(42, b"hello")
reveal_type(ext)  # revealed: ExtType

url = Url("example.com", "path")
reveal_type(url)  # revealed: Url
```

### Functional syntax with list variable fields

When fields are passed via a list variable (not a literal), the field names cannot be determined
statically. Attribute access returns `Any` and the constructor accepts any arguments:

```py
from typing import NamedTuple
from typing_extensions import Self

fields = [("host", str), ("port", int)]

# error: [invalid-named-tuple] "Invalid argument to parameter `fields` of `NamedTuple()`: `fields` must be a literal list or tuple"
class Url(NamedTuple("Url", fields)):
    def with_port(self, port: int) -> Self:
        # Fields are unknown, so attribute access returns Any.
        reveal_type(self.host)  # revealed: Any
        reveal_type(self.port)  # revealed: Any
        reveal_type(self.unknown)  # revealed: Any
        return self._replace(port=port)
```

When constructing a namedtuple directly with dynamically-defined fields, keyword arguments are
accepted because the constructor uses a gradual signature:

```py
import collections
from ty_extensions import reveal_mro

CheckerConfig = ["duration", "video_fps", "audio_sample_rate"]
GroundTruth = collections.namedtuple("GroundTruth", " ".join(CheckerConfig))

# No error - fields are unknown, so any keyword arguments are accepted
config = GroundTruth(duration=0, video_fps=30, audio_sample_rate=44100)
reveal_type(config)  # revealed: GroundTruth
reveal_type(config.duration)  # revealed: Any

# Namedtuples with unknown fields inherit from tuple[Unknown, ...] to avoid false positives.
# revealed: (<class 'GroundTruth'>, <class 'tuple[Unknown, ...]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(GroundTruth)

# No index-out-of-bounds error since the tuple length is unknown.
reveal_type(config[0])  # revealed: Unknown
reveal_type(config[100])  # revealed: Unknown
```

### Functional syntax signature validation

The `collections.namedtuple` function accepts `str | Iterable[str]` for `field_names`:

```py
import collections
from ty_extensions import reveal_mro

# String field names (space-separated)
Point1 = collections.namedtuple("Point", "x y")
reveal_type(Point1)  # revealed: <class 'Point'>
# revealed: (<class 'Point'>, <class 'tuple[Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Point1)

# String field names with multiple spaces
Point1a = collections.namedtuple("Point", "x       y")
reveal_type(Point1a)  # revealed: <class 'Point'>
# revealed: (<class 'Point'>, <class 'tuple[Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Point1a)

# String field names (comma-separated also works at runtime)
Point2 = collections.namedtuple("Point", "x, y")
reveal_type(Point2)  # revealed: <class 'Point'>
# revealed: (<class 'Point'>, <class 'tuple[Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Point2)

# List of strings
Point3 = collections.namedtuple("Point", ["x", "y"])
reveal_type(Point3)  # revealed: <class 'Point'>
# revealed: (<class 'Point'>, <class 'tuple[Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Point3)

# Tuple of strings
Point4 = collections.namedtuple("Point", ("x", "y"))
reveal_type(Point4)  # revealed: <class 'Point'>
# revealed: (<class 'Point'>, <class 'tuple[Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Point4)
# Invalid: integer is not a valid typename
# error: [invalid-argument-type]
Invalid = collections.namedtuple(123, ["x", "y"])
reveal_type(Invalid)  # revealed: <class '<unknown>'>
# revealed: (<class '<unknown>'>, <class 'tuple[Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Invalid)

# Invalid: too many positional arguments
# error: [too-many-positional-arguments] "Too many positional arguments to function `namedtuple`: expected 2, got 4"
TooMany = collections.namedtuple("TooMany", "x", "y", "z")
reveal_type(TooMany)  # revealed: <class 'TooMany'>
```

The `typing.NamedTuple` function accepts `Iterable[tuple[str, Any]]` for `fields`:

```py
from typing import NamedTuple

# List of tuples
Person1 = NamedTuple("Person", [("name", str), ("age", int)])
reveal_type(Person1)  # revealed: <class 'Person'>

# Tuple of tuples
Person2 = NamedTuple("Person", (("name", str), ("age", int)))
reveal_type(Person2)  # revealed: <class 'Person'>

# Invalid: integer is not a valid typename
# error: [invalid-argument-type]
NamedTuple(123, [("name", str)])

# Invalid: too many positional arguments
# error: [too-many-positional-arguments] "Too many positional arguments to function `NamedTuple`: expected 2, got 4"
TooMany = NamedTuple("TooMany", [("x", int)], "extra", "args")
reveal_type(TooMany)  # revealed: <class 'TooMany'>
```

### Keyword arguments for `collections.namedtuple`

The `collections.namedtuple` function accepts `typename` and `field_names` as keyword arguments, as
well as `rename`, `defaults`, and `module`:

```py
import collections
from ty_extensions import reveal_mro

# Both `typename` and `field_names` can be passed as keyword arguments
NT1 = collections.namedtuple(typename="NT1", field_names="x y")
reveal_type(NT1)  # revealed: <class 'NT1'>
# revealed: (<class 'NT1'>, <class 'tuple[Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(NT1)

nt1 = NT1(1, 2)
reveal_type(nt1.x)  # revealed: Any
reveal_type(nt1.y)  # revealed: Any

# Only `field_names` as keyword argument
NT2 = collections.namedtuple("NT2", field_names=["a", "b", "c"])
reveal_type(NT2)  # revealed: <class 'NT2'>
# revealed: (<class 'NT2'>, <class 'tuple[Any, Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(NT2)

nt2 = NT2(1, 2, 3)
reveal_type(nt2.a)  # revealed: Any
reveal_type(nt2.b)  # revealed: Any
reveal_type(nt2.c)  # revealed: Any

# Keyword arguments can be combined with other kwargs like `defaults`
NT3 = collections.namedtuple(typename="NT3", field_names="x y z", defaults=[None])
reveal_type(NT3)  # revealed: <class 'NT3'>
reveal_type(NT3.__new__)  # revealed: [Self](cls: type[Self], x: Any, y: Any, z: Any = None) -> Self

nt3 = NT3(1, 2)
reveal_type(nt3.z)  # revealed: Any

# Passing the same argument positionally and as a keyword is an error
# error: [parameter-already-assigned] "Multiple values provided for parameter `typename` of `namedtuple`"
Bad1 = collections.namedtuple("Bad1", "x y", typename="Bad1")

# error: [parameter-already-assigned] "Multiple values provided for parameter `field_names` of `namedtuple`"
Bad2 = collections.namedtuple("Bad2", "x y", field_names="a b")
```

The `rename`, `defaults`, and `module` keyword arguments:

```py
import collections
from ty_extensions import reveal_mro

# `rename=True` replaces invalid identifiers with positional names
Point = collections.namedtuple("Point", ["x", "class", "_y", "z", "z"], rename=True)
reveal_type(Point)  # revealed: <class 'Point'>
reveal_type(Point.__new__)  # revealed: [Self](cls: type[Self], x: Any, _1: Any, _2: Any, z: Any, _4: Any) -> Self
# revealed: (<class 'Point'>, <class 'tuple[Any, Any, Any, Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Point)
p = Point(1, 2, 3, 4, 5)
reveal_type(p.x)  # revealed: Any
reveal_type(p._1)  # revealed: Any
reveal_type(p._2)  # revealed: Any
reveal_type(p.z)  # revealed: Any
reveal_type(p._4)  # revealed: Any

# Truthy non-bool values for `rename` are also handled, but emit a diagnostic
# error: [invalid-argument-type] "Invalid argument to parameter `rename` of `namedtuple()`"
Point2 = collections.namedtuple("Point2", ["_x", "class"], rename=1)
reveal_type(Point2)  # revealed: <class 'Point2'>
reveal_type(Point2.__new__)  # revealed: [Self](cls: type[Self], _0: Any, _1: Any) -> Self

# Without `rename=True`, invalid field names emit diagnostics:
# - Field names starting with underscore
# error: [invalid-named-tuple] "Field name `_x` in `namedtuple()` cannot start with an underscore"
Underscore = collections.namedtuple("Underscore", ["_x", "y"])
reveal_type(Underscore)  # revealed: <class 'Underscore'>

# - Python keywords
# error: [invalid-named-tuple] "Field name `class` in `namedtuple()` cannot be a Python keyword"
Keyword = collections.namedtuple("Keyword", ["x", "class"])
reveal_type(Keyword)  # revealed: <class 'Keyword'>

# - Duplicate field names
# error: [invalid-named-tuple] "Duplicate field name `x` in `namedtuple()`"
Duplicate = collections.namedtuple("Duplicate", ["x", "y", "x"])
reveal_type(Duplicate)  # revealed: <class 'Duplicate'>

# - Invalid identifiers (e.g., containing spaces)
# error: [invalid-named-tuple] "Field name `not valid` in `namedtuple()` is not a valid identifier"
Invalid = collections.namedtuple("Invalid", ["not valid", "ok"])
reveal_type(Invalid)  # revealed: <class 'Invalid'>

# `defaults` provides default values for the rightmost fields
Person = collections.namedtuple("Person", ["name", "age", "city"], defaults=["Unknown"])
reveal_type(Person)  # revealed: <class 'Person'>
reveal_type(Person.__new__)  # revealed: [Self](cls: type[Self], name: Any, age: Any, city: Any = "Unknown") -> Self

# revealed: (<class 'Person'>, <class 'tuple[Any, Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Person)
# Can create with all fields
person1 = Person("Alice", 30, "NYC")
# Can omit the field with default
person2 = Person("Bob", 25)
reveal_type(person1.city)  # revealed: Any
reveal_type(person2.city)  # revealed: Any

# `module` is valid but doesn't affect type checking
Config = collections.namedtuple("Config", ["host", "port"], module="myapp")
reveal_type(Config)  # revealed: <class 'Config'>

# When more defaults are provided than fields, an error is emitted.
# error: [invalid-named-tuple] "Too many defaults for `namedtuple()`"
TooManyDefaults = collections.namedtuple("TooManyDefaults", ["x", "y"], defaults=("a", "b", "c"))
reveal_type(TooManyDefaults)  # revealed: <class 'TooManyDefaults'>
reveal_type(TooManyDefaults.__new__)  # revealed: [Self](cls: type[Self], x: Any = "a", y: Any = "b") -> Self

# Unknown keyword arguments produce an error
# error: [unknown-argument]
Bad1 = collections.namedtuple("Bad1", ["x", "y"], foobarbaz=42)
reveal_type(Bad1)  # revealed: <class 'Bad1'>
# revealed: (<class 'Bad1'>, <class 'tuple[Any, Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Bad1)

# Multiple unknown keyword arguments
# error: [unknown-argument]
# error: [unknown-argument]
Bad2 = collections.namedtuple("Bad2", ["x"], invalid1=True, invalid2=False)
reveal_type(Bad2)  # revealed: <class 'Bad2'>
# revealed: (<class 'Bad2'>, <class 'tuple[Any]'>, <class 'Sequence[Any]'>, <class 'Reversible[Any]'>, <class 'Collection[Any]'>, <class 'Iterable[Any]'>, <class 'Container[Any]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(Bad2)

# Invalid type for `defaults` (not Iterable[Any] | None)
# error: [invalid-argument-type] "Invalid argument to parameter `defaults` of `namedtuple()`"
Bad3 = collections.namedtuple("Bad3", ["x"], defaults=123)
reveal_type(Bad3)  # revealed: <class 'Bad3'>

# Invalid type for `module` (not str | None)
# error: [invalid-argument-type] "Invalid argument to parameter `module` of `namedtuple()`"
Bad4 = collections.namedtuple("Bad4", ["x"], module=456)
reveal_type(Bad4)  # revealed: <class 'Bad4'>

# Invalid type for `field_names` (not str | Iterable[str])
# error: [invalid-argument-type] "Invalid argument to parameter `field_names` of `namedtuple()`"
Bad5 = collections.namedtuple("Bad5", 12345)
reveal_type(Bad5)  # revealed: <class 'Bad5'>
```

### Keyword arguments for `typing.NamedTuple`

Unlike `collections.namedtuple`, the `typing.NamedTuple` function does not accept `typename` or
`fields` as keyword arguments. It also does not accept `rename`, `defaults`, or `module`:

```py
from typing import NamedTuple

# `typename` and `fields` are not valid as keyword arguments for typing.NamedTuple
# (We only report the missing-argument error in this case since we return early)
# error: [missing-argument]
Bad1 = NamedTuple(typename="Bad1", fields=[("x", int)])

# error: [unknown-argument]
Bad2 = NamedTuple("Bad2", [("x", int)], typename="Bad2")

# error: [unknown-argument]
Bad3 = NamedTuple("Bad3", [("x", int)], fields=[("y", str)])

# `rename`, `defaults`, and `module` are also not valid for typing.NamedTuple
# error: [unknown-argument]
Bad4 = NamedTuple("Bad4", [("x", int)], rename=True)

# error: [unknown-argument]
Bad4 = NamedTuple("Bad4", [("x", int)], defaults=[0])

# error: [unknown-argument]
Bad5 = NamedTuple("Bad5", [("x", int)], foobarbaz=42)

# Invalid type for `fields` (not an iterable)
# error: [invalid-named-tuple] "Invalid argument to parameter `fields` of `NamedTuple()`: `fields` must be a literal list or tuple"
Bad6 = NamedTuple("Bad6", 12345)
reveal_type(Bad6)  # revealed: <class 'Bad6'>

# Invalid field definitions: strings instead of (name, type) tuples
# error: [invalid-named-tuple] "Invalid argument to parameter `fields` of `NamedTuple()`: `fields` must be a sequence of literal lists or tuples"
Bad7 = NamedTuple("Bad7", ["a", "b"])
reveal_type(Bad7)  # revealed: <class 'Bad7'>

# Invalid field definitions: type is not a valid type expression (e.g., int literals)
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
# error: [invalid-type-form] "Int literals are not allowed in this context in a type expression"
Bad8 = NamedTuple("Bad8", [("a", 123), ("b", 456)])
reveal_type(Bad8)  # revealed: <class 'Bad8'>
```

### Missing required arguments

`NamedTuple` and `namedtuple` require `typename` and `fields`/`field_names` arguments. For
`collections.namedtuple`, these can be positional or keyword; for `typing.NamedTuple`, they must be
positional.

```py
import collections
from typing import NamedTuple

# Missing both typename and fields
# error: [missing-argument] "Missing required arguments `typename` and `fields` to `NamedTuple()`"
Bad1 = NamedTuple()
reveal_type(Bad1)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown
Bad1()  # no error

# Missing fields argument
# error: [missing-argument] "Missing required argument `fields` to `NamedTuple()`"
Bad2 = NamedTuple("Bad2")
reveal_type(Bad2)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown
Bad2(Bad2, foo=56)  # no error

# Missing both typename and field_names for collections.namedtuple
# error: [missing-argument] "Missing required arguments `typename` and `field_names` to `namedtuple()`"
Bad3 = collections.namedtuple()
reveal_type(Bad3)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown
Bad3(56, foo="foo")  # no error

# Missing field_names argument
# error: [missing-argument] "Missing required argument `field_names` to `namedtuple()`"
Bad4 = collections.namedtuple("Bad4")
reveal_type(Bad4)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown
Bad4(42, 56, 79)  # no error
```

### Starred and double-starred arguments

For `collections.namedtuple`, starred (`*args`) or double-starred (`**kwargs`) arguments cause us to
fall back to `NamedTupleFallback` since we can't statically determine the arguments:

```py
import collections

args = ("Point", ["x", "y"])
kwargs = {"rename": True}

# Starred positional arguments - falls back to NamedTupleFallback
Point1 = collections.namedtuple(*args)
reveal_type(Point1)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown

# Double-starred keyword arguments - falls back to NamedTupleFallback
Point2 = collections.namedtuple("Point", ["x", "y"], **kwargs)
reveal_type(Point2)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown

# Both starred and double-starred
Point3 = collections.namedtuple(*args, **kwargs)
reveal_type(Point3)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown
```

For `typing.NamedTuple`, variadic arguments are not supported and result in an error:

```py
from typing import NamedTuple

args = ("Point", [("x", int), ("y", int)])
kwargs = {"extra": True}

# error: [invalid-argument-type] "Variadic positional arguments are not supported in `NamedTuple()` calls"
Point1 = NamedTuple(*args)
reveal_type(Point1)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown

# error: [invalid-argument-type] "Variadic positional arguments are not supported in `NamedTuple()` calls"
Point2 = NamedTuple("Point", *args)
reveal_type(Point2)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown

# error: [invalid-argument-type] "Variadic keyword arguments are not supported in `NamedTuple()` calls"
Point3 = NamedTuple("Point", [("x", int), ("y", int)], **kwargs)
reveal_type(Point3)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown

# error: [invalid-argument-type] "Variadic positional and keyword arguments are not supported in `NamedTuple()` calls"
Point4 = NamedTuple(*args, **kwargs)
reveal_type(Point4)  # revealed: type[tuple[Unknown, ...]] & type[NamedTupleLike] & Unknown
Point4(x=46, y=72)  # no error
```

### Definition

<!-- snapshot-diagnostics -->

Fields without default values must come before fields with.

```py
from typing import NamedTuple

class Location(NamedTuple):
    altitude: float = 0.0
    # error: [invalid-named-tuple] "NamedTuple field without default value cannot follow field(s) with default value(s): Field `latitude` defined here without a default value"
    latitude: float
    # error: [invalid-named-tuple] "NamedTuple field without default value cannot follow field(s) with default value(s): Field `longitude` defined here without a default value"
    longitude: float

class StrangeLocation(NamedTuple):
    altitude: float
    altitude: float = 0.0
    altitude: float
    altitude: float = 0.0
    latitude: float  # error: [invalid-named-tuple]
    longitude: float  # error: [invalid-named-tuple]

class VeryStrangeLocation(NamedTuple):
    altitude: float = 0.0
    latitude: float  # error: [invalid-named-tuple]
    longitude: float  # error: [invalid-named-tuple]
    altitude: float = 0.0
```

### Multiple Inheritance

<!-- snapshot-diagnostics -->

Multiple inheritance is not supported for `NamedTuple` classes except with `Generic`:

```py
from typing import NamedTuple, Protocol

# error: [invalid-named-tuple] "NamedTuple class `C` cannot use multiple inheritance except with `Generic[]`"
class C(NamedTuple, object):
    id: int

# fmt: off

class D(
    int,  # error: [invalid-named-tuple]
    NamedTuple
): ...

# fmt: on

# error: [invalid-named-tuple]
class E(NamedTuple, Protocol): ...
```

However, as explained above, for `class Foo(namedtuple("Foo", ...)): ...` the outer class is not
itself a namedtuple—it just inherits from one. So it can use multiple inheritance freely:

```py
from abc import ABC
from collections import namedtuple
from typing import NamedTuple

class Point(namedtuple("Point", ["x", "y"]), ABC):
    """No error - functional namedtuple inheritance allows multiple inheritance."""

class Url(NamedTuple("Url", [("host", str), ("port", int)]), ABC):
    """No error - typing.NamedTuple functional syntax also allows multiple inheritance."""

p = Point(1, 2)
reveal_type(p.x)  # revealed: Any
reveal_type(p.y)  # revealed: Any

u = Url("localhost", 8080)
reveal_type(u.host)  # revealed: str
reveal_type(u.port)  # revealed: int
```

### Inherited tuple methods

Namedtuples inherit methods from their tuple base class, including `count`, `index`, and comparison
methods (`__lt__`, `__le__`, `__gt__`, `__ge__`).

```py
from collections import namedtuple
from typing import NamedTuple

# typing.NamedTuple inherits tuple methods
class Point(NamedTuple):
    x: int
    y: int

p = Point(1, 2)
reveal_type(p.count(1))  # revealed: int
reveal_type(p.index(2))  # revealed: int

# collections.namedtuple also inherits tuple methods
Person = namedtuple("Person", ["name", "age"])
alice = Person("Alice", 30)
reveal_type(alice.count("Alice"))  # revealed: int
reveal_type(alice.index(30))  # revealed: int
```

The `@total_ordering` decorator should not emit a diagnostic, since the required `__lt__` method is
already present:

```py
from collections import namedtuple
from functools import total_ordering
from typing import NamedTuple

# No error - __lt__ is inherited from the tuple base class
@total_ordering
class Point(namedtuple("Point", "x y")): ...

p1 = Point(1, 2)
p2 = Point(3, 4)
# TODO: should be `bool`, not `Any | Literal[False]`
reveal_type(p1 < p2)  # revealed: Any | Literal[False]
reveal_type(p1 <= p2)  # revealed: Any | Literal[True]

# Same for typing.NamedTuple - no error
@total_ordering
class Person(NamedTuple):
    name: str
    age: int

alice = Person("Alice", 30)
bob = Person("Bob", 25)
reveal_type(alice < bob)  # revealed: bool
reveal_type(alice >= bob)  # revealed: bool
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
    age: int | None
    nickname: str

class SuperUser(User):
    # TODO: this should be an error because it implies that the `id` attribute on
    # `SuperUser` is mutable, but the read-only `id` property from the superclass
    # has not been overridden in the class body
    id: int

    # this is fine; overriding a read-only attribute with a mutable one
    # does not conflict with the Liskov Substitution Principle
    name: str = "foo"

    # this is also fine
    @property
    def age(self) -> int:
        return super().age or 42

    def now_called_robert(self):
        self.name = "Robert"  # fine because overridden with a mutable attribute

        # error: 9 [invalid-assignment] "Cannot assign to read-only property `nickname` on object of type `Self@now_called_robert`"
        self.nickname = "Bob"

james = SuperUser(0, "James", 42, "Jimmy")

# fine because the property on the superclass was overridden with a mutable attribute
# on the subclass
james.name = "Robert"

# error: [invalid-assignment] "Cannot assign to read-only property `nickname` on object of type `SuperUser`"
james.nickname = "Bob"
```

### Generic named tuples

```toml
[environment]
python-version = "3.12"
```

```py
from typing import NamedTuple, Generic, TypeVar

class Property[T](NamedTuple):
    name: str
    value: T

reveal_type(Property("height", 3.4))  # revealed: Property[float]
reveal_type(Property.value)  # revealed: property
reveal_type(Property.value.fget)  # revealed: (self, /) -> Unknown
reveal_type(Property[str].value.fget)  # revealed: (self, /) -> str
reveal_type(Property("height", 3.4).value)  # revealed: float

T = TypeVar("T")

class LegacyProperty(NamedTuple, Generic[T]):
    name: str
    value: T

reveal_type(LegacyProperty("height", 42))  # revealed: LegacyProperty[int]
reveal_type(LegacyProperty.value)  # revealed: property
reveal_type(LegacyProperty.value.fget)  # revealed: (self, /) -> Unknown
reveal_type(LegacyProperty[str].value.fget)  # revealed: (self, /) -> str
reveal_type(LegacyProperty("height", 3.4).value)  # revealed: int | float
```

### Functional syntax with generics

Generic namedtuples can also be defined using the functional syntax with type variables in the field
types. We don't currently support this, but mypy does:

```py
from typing import NamedTuple, TypeVar

T = TypeVar("T")

# TODO: ideally this would create a generic namedtuple class
Pair = NamedTuple("Pair", [("first", T), ("second", T)])

# For now, the TypeVar is not specialized, so the field types remain as `T@Pair` and argument type
# errors are emitted when calling the constructor.
reveal_type(Pair)  # revealed: <class 'Pair'>

# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(Pair(1, 2))  # revealed: Pair

# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(Pair(1, 2).first)  # revealed: TypeVar

# error: [invalid-argument-type]
# error: [invalid-argument-type]
reveal_type(Pair(1, 2).second)  # revealed: TypeVar
```

## Attributes on `NamedTuple`

The following attributes are available on `NamedTuple` classes / instances:

```py
from typing import NamedTuple

class Person(NamedTuple):
    name: str
    age: int | None = None

reveal_type(Person._field_defaults)  # revealed: dict[str, Any]
reveal_type(Person._fields)  # revealed: tuple[Literal["name"], Literal["age"]]
reveal_type(Person.__slots__)  # revealed: tuple[()]
reveal_type(Person._make)  # revealed: bound method <class 'Person'>._make(iterable: Iterable[Any]) -> Person
reveal_type(Person._asdict)  # revealed: def _asdict(self) -> dict[str, Any]
reveal_type(Person._replace)  # revealed: (self: Self, *, name: str = ..., age: int | None = ...) -> Self

reveal_type(Person._make(("Alice", 42)))  # revealed: Person

person = Person("Alice", 42)

reveal_type(person._asdict())  # revealed: dict[str, Any]
reveal_type(person._replace(name="Bob"))  # revealed: Person

# Invalid keyword arguments are detected:
# error: [unknown-argument] "Argument `invalid` does not match any known parameter"
person._replace(invalid=42)
```

When accessing them on child classes of generic `NamedTuple`s, the return type is specialized
accordingly:

```py
from typing import NamedTuple, Generic, TypeVar

T = TypeVar("T")

class Box(NamedTuple, Generic[T]):
    content: T

class IntBox(Box[int]):
    pass

reveal_type(IntBox(1)._replace(content=42))  # revealed: IntBox
```

## `collections.namedtuple`

```py
from collections import namedtuple

Person = namedtuple("Person", ["id", "name", "age"], defaults=[None])

alice = Person(1, "Alice", 42)
bob = Person(2, "Bob")

reveal_type(Person.__slots__)  # revealed: tuple[()]
```

## `collections.namedtuple` with tuple variable field names

When field names are passed via a tuple variable, we can extract the literal field names from the
inferred tuple type. The class is properly synthesized (not a fallback), but field types are `Any`
since `collections.namedtuple` doesn't include type annotations:

```py
from collections import namedtuple

field_names = ("name", "age")
Person = namedtuple("Person", field_names)

reveal_type(Person)  # revealed: <class 'Person'>

alice = Person("Alice", 42)
reveal_type(alice)  # revealed: Person
reveal_type(alice.name)  # revealed: Any
reveal_type(alice.age)  # revealed: Any
```

## `collections.namedtuple` with list variable field names

When field names are passed via a list variable (not a literal), we fall back to
`NamedTupleFallback` which allows any attribute access. This is a regression test for accessing
`Self` attributes in methods of classes that inherit from namedtuples with dynamic fields:

```py
from collections import namedtuple
from typing_extensions import Self

field_names = ["host", "port"]

class Url(namedtuple("Url", field_names)):
    def with_port(self, port: int) -> Self:
        # Fields are unknown, so attribute access returns `Any`.
        reveal_type(self.host)  # revealed: Any
        reveal_type(self.port)  # revealed: Any
        reveal_type(self.unknown)  # revealed: Any
        return self._replace(port=port)
```

## `collections.namedtuple` attributes

Functional namedtuples have synthesized attributes similar to class-based namedtuples:

```py
from collections import namedtuple

Person = namedtuple("Person", ["name", "age"])

reveal_type(Person._fields)  # revealed: tuple[Literal["name"], Literal["age"]]
reveal_type(Person._field_defaults)  # revealed: dict[str, Any]
reveal_type(Person._make)  # revealed: bound method <class 'Person'>._make(iterable: Iterable[Any]) -> Person
reveal_type(Person._asdict)  # revealed: def _asdict(self) -> dict[str, Any]
reveal_type(Person._replace)  # revealed: (self: Self, *, name: Any = ..., age: Any = ...) -> Self

# _make creates instances from an iterable.
reveal_type(Person._make(["Alice", 30]))  # revealed: Person

# _asdict converts to a dictionary.
person = Person("Alice", 30)
reveal_type(person._asdict())  # revealed: dict[str, Any]

# _replace creates a copy with replaced fields.
reveal_type(person._replace(name="Bob"))  # revealed: Person
```

## The symbol `NamedTuple` itself

At runtime, `NamedTuple` is a function, and we understand this:

```py
import types
import typing

def expects_functiontype(x: types.FunctionType): ...

expects_functiontype(typing.NamedTuple)
```

This means we also understand that all attributes on function objects are available on the symbol
`typing.NamedTuple`:

```py
reveal_type(typing.NamedTuple.__name__)  # revealed: str
reveal_type(typing.NamedTuple.__qualname__)  # revealed: str
reveal_type(typing.NamedTuple.__kwdefaults__)  # revealed: dict[str, Any] | None

# error: [unresolved-attribute]
reveal_type(typing.NamedTuple.__mro__)  # revealed: Unknown
```

By the normal rules, `NamedTuple` and `type[NamedTuple]` should not be valid in type expressions --
there is no object at runtime that is an "instance of `NamedTuple`", nor is there any class at
runtime that is a "subclass of `NamedTuple`" -- these are both impossible, since `NamedTuple` is a
function and not a class. However, for compatibility with other type checkers, we allow `NamedTuple`
in type expressions and understand it as describing an interface that all `NamedTuple` classes would
satisfy:

```py
def expects_named_tuple(x: typing.NamedTuple):
    reveal_type(x)  # revealed: tuple[object, ...] & NamedTupleLike
    reveal_type(x._make)  # revealed: bound method type[NamedTupleLike]._make(iterable: Iterable[Any]) -> NamedTupleLike
    reveal_type(x._replace)  # revealed: bound method NamedTupleLike._replace(...) -> NamedTupleLike
    # revealed: Overload[(value: tuple[object, ...], /) -> tuple[object, ...], [_T](value: tuple[_T, ...], /) -> tuple[object, ...]]
    reveal_type(x.__add__)
    reveal_type(x.__iter__)  # revealed: bound method tuple[object, ...].__iter__() -> Iterator[object]

def _(y: type[typing.NamedTuple]):
    reveal_type(y)  # revealed: @Todo(unsupported type[X] special form)

# error: [invalid-type-form] "Special form `typing.NamedTuple` expected no type parameter"
def _(z: typing.NamedTuple[int]): ...
```

NamedTuples are assignable to `NamedTupleLike`. The `NamedTupleLike._replace` method is typed with
`(*args, **kwargs)`, which type checkers treat as equivalent to `...` (per the typing spec), making
all NamedTuple implementations automatically compatible:

```py
from typing import NamedTuple, Protocol, Iterable, Any
from ty_extensions import static_assert, is_assignable_to

class Point(NamedTuple):
    x: int
    y: int

reveal_type(Point._make)  # revealed: bound method <class 'Point'>._make(iterable: Iterable[Any]) -> Point
reveal_type(Point._asdict)  # revealed: def _asdict(self) -> dict[str, Any]
reveal_type(Point._replace)  # revealed: (self: Self, *, x: int = ..., y: int = ...) -> Self

# Point is assignable to NamedTuple.
static_assert(is_assignable_to(Point, NamedTuple))

# NamedTuple instances can be passed to functions expecting NamedTupleLike.
expects_named_tuple(Point(x=42, y=56))

# But plain tuples are not NamedTupleLike (they don't have _make, _asdict, _replace, etc.).
# error: [invalid-argument-type] "Argument to function `expects_named_tuple` is incorrect: Expected `tuple[object, ...] & NamedTupleLike`, found `tuple[Literal[1], Literal[2]]`"
expects_named_tuple((1, 2))
```

The type described by `NamedTuple` in type expressions is understood as being assignable to
`tuple[object, ...]` and `tuple[Any, ...]`:

```py
static_assert(is_assignable_to(NamedTuple, tuple))
static_assert(is_assignable_to(NamedTuple, tuple[object, ...]))
static_assert(is_assignable_to(NamedTuple, tuple[Any, ...]))

def expects_tuple(x: tuple[object, ...]): ...
def _(x: NamedTuple):
    expects_tuple(x)  # fine
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

## `super()` is not supported in NamedTuple methods

Using `super()` in a method of a `NamedTuple` class will raise an exception at runtime. In Python
3.14+, a `TypeError` is raised; in earlier versions, a confusing `RuntimeError` about
`__classcell__` is raised.

```py
from typing import NamedTuple

class F(NamedTuple):
    x: int

    def method(self):
        # error: [super-call-in-named-tuple-method] "Cannot use `super()` in a method of NamedTuple class `F`"
        super()

    def method_with_args(self):
        # error: [super-call-in-named-tuple-method] "Cannot use `super()` in a method of NamedTuple class `F`"
        super(F, self)

    def method_with_different_pivot(self):
        # Even passing a different pivot class fails.
        # error: [super-call-in-named-tuple-method] "Cannot use `super()` in a method of NamedTuple class `F`"
        super(tuple, self)

    @classmethod
    def class_method(cls):
        # error: [super-call-in-named-tuple-method] "Cannot use `super()` in a method of NamedTuple class `F`"
        super()

    @staticmethod
    def static_method():
        # error: [super-call-in-named-tuple-method] "Cannot use `super()` in a method of NamedTuple class `F`"
        super()

    @property
    def prop(self):
        # error: [super-call-in-named-tuple-method] "Cannot use `super()` in a method of NamedTuple class `F`"
        return super()
```

However, classes that **inherit from** a `NamedTuple` class (but don't directly inherit from
`NamedTuple`) can use `super()` normally:

```py
from typing import NamedTuple

class Base(NamedTuple):
    x: int

class Child(Base):
    def method(self):
        super()
```

And regular classes that don't inherit from `NamedTuple` at all can use `super()` as normal:

```py
class Regular:
    def method(self):
        super()  # fine
```

Using `super()` on a `NamedTuple` class also works fine if it occurs outside the class:

```py
from typing import NamedTuple

class F(NamedTuple):
    x: int

super(F, F(42))  # fine
```

## NamedTuples cannot have field names starting with underscores

<!-- snapshot-diagnostics -->

```py
from typing import NamedTuple

class Foo(NamedTuple):
    # error: [invalid-named-tuple] "NamedTuple field `_bar` cannot start with an underscore"
    _bar: int

class Bar(NamedTuple):
    x: int

class Baz(Bar):
    _whatever: str  # `Baz` is not a NamedTuple class, so this is fine
```

The same validation applies to the functional `typing.NamedTuple` syntax:

```py
from typing import NamedTuple

# error: [invalid-named-tuple] "Field name `_x` in `NamedTuple()` cannot start with an underscore"
Underscore = NamedTuple("Underscore", [("_x", int), ("y", str)])
reveal_type(Underscore)  # revealed: <class 'Underscore'>

# error: [invalid-named-tuple] "Field name `class` in `NamedTuple()` cannot be a Python keyword"
Keyword = NamedTuple("Keyword", [("x", int), ("class", str)])
reveal_type(Keyword)  # revealed: <class 'Keyword'>

# error: [invalid-named-tuple] "Duplicate field name `x` in `NamedTuple()`"
Duplicate = NamedTuple("Duplicate", [("x", int), ("y", str), ("x", float)])
reveal_type(Duplicate)  # revealed: <class 'Duplicate'>

# error: [invalid-named-tuple] "Field name `not valid` in `NamedTuple()` is not a valid identifier"
Invalid = NamedTuple("Invalid", [("not valid", int), ("ok", str)])
reveal_type(Invalid)  # revealed: <class 'Invalid'>
```

## Prohibited NamedTuple attributes

`NamedTuple` classes have certain synthesized attributes that cannot be overwritten. Attempting to
assign to these attributes (without type annotations) will raise an `AttributeError` at runtime.

```py
from typing import NamedTuple

class F(NamedTuple):
    x: int

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_asdict`"
    _asdict = 42

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_make`"
    _make = "foo"

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_replace`"
    _replace = lambda self: self

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_fields`"
    _fields = ()

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_field_defaults`"
    _field_defaults = {}

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `__new__`"
    __new__ = None

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `__init__`"
    __init__ = None

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `__getnewargs__`"
    __getnewargs__ = None
```

However, other attributes (including those starting with underscores) can be assigned without error:

```py
from typing import NamedTuple

class G(NamedTuple):
    x: int

    # These are fine (not prohibited attributes)
    _custom = 42
    __custom__ = "ok"
    regular_attr = "value"
```

Note that type-annotated attributes become NamedTuple fields, not attribute overrides. They are not
flagged as prohibited attribute overrides (though field names starting with `_` are caught by the
underscore field name check):

```py
from typing import NamedTuple

class H(NamedTuple):
    x: int
    # This is a field declaration, not an override. It's not flagged as an override,
    # but is flagged because field names cannot start with underscores.
    # error: [invalid-named-tuple] "NamedTuple field `_asdict` cannot start with an underscore"
    _asdict: int = 0
```

The check also applies to assignments within conditional blocks:

```py
from typing import NamedTuple

class I(NamedTuple):
    x: int

    if True:
        # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_asdict`"
        _asdict = 42
```

Method definitions with prohibited names are also flagged:

```py
from typing import NamedTuple

class J(NamedTuple):
    x: int

    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_asdict`"
    def _asdict(self):
        return {}

    @classmethod
    # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_make`"
    def _make(cls, iterable):
        return cls(*iterable)
```

Classes that inherit from a `NamedTuple` class (but don't directly inherit from `NamedTuple`) are
not subject to these restrictions:

```py
from typing import NamedTuple

class Base(NamedTuple):
    x: int

class Child(Base):
    # This is fine - Child is not directly a NamedTuple
    _asdict = 42
```

## `NamedTuple` with `@dataclass` decorator

Applying `@dataclass` to a `NamedTuple` class is invalid. An exception will be raised when
instantiating the class at runtime:

```py
from dataclasses import dataclass
from typing import NamedTuple

@dataclass
# error: [invalid-dataclass] "`NamedTuple` class `Foo` cannot be decorated with `@dataclass`"
class Foo(NamedTuple):
    x: int
    y: str
```

The same error occurs with `dataclasses.dataclass` used with parentheses:

```py
from dataclasses import dataclass
from typing import NamedTuple

@dataclass()
# error: [invalid-dataclass]
class Bar(NamedTuple):
    x: int
```

It also applies when using `frozen=True` or other dataclass parameters:

```py
from dataclasses import dataclass
from typing import NamedTuple

@dataclass(frozen=True)
# error: [invalid-dataclass]
class Baz(NamedTuple):
    x: int
```

Classes that inherit from a `NamedTuple` class also cannot be decorated with `@dataclass`:

```py
from dataclasses import dataclass
from typing import NamedTuple

class Base(NamedTuple):
    x: int

@dataclass
# error: [invalid-dataclass]
class Child(Base):
    y: str
```

The same restriction applies to classes inheriting from functional namedtuples:

```py
from dataclasses import dataclass
from collections import namedtuple
from typing import NamedTuple

@dataclass
# error: [invalid-dataclass]
class Foo(namedtuple("Foo", ["x", "y"])):
    pass

@dataclass
# error: [invalid-dataclass]
class Bar(NamedTuple("Bar", [("x", int), ("y", str)])):
    pass
```

The same applies when using `dataclass` as a function on a functional `NamedTuple`:

```py
from dataclasses import dataclass
from typing import NamedTuple

# error: [invalid-dataclass] "Cannot use `dataclass()` on a `NamedTuple` class"
X = dataclass(NamedTuple("X", [("x", int)]))
```

## Edge case: multiple reachable definitions with distinct issues

<!-- snapshot-diagnostics -->

```py
from typing import NamedTuple

def coinflip() -> bool:
    return True

class Foo(NamedTuple):
    if coinflip():
        _asdict: bool  # error: [invalid-named-tuple] "NamedTuple field `_asdict` cannot start with an underscore"
    else:
        # TODO: there should only be one diagnostic here...
        #
        # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_asdict`"
        # error: [invalid-named-tuple] "Cannot overwrite NamedTuple attribute `_asdict`"
        _asdict = True
```

## `super().__new__` in `NamedTuple` subclasses

This is a regression test for <https://github.com/astral-sh/ty/issues/2522>.

```py
from typing import NamedTuple, Generic, TypeVar
from typing_extensions import Self

class Base(NamedTuple):
    x: int
    y: int

class Child(Base):
    def __new__(cls, x: int, y: int) -> Self:
        instance = super().__new__(cls, x, y)
        reveal_type(instance)  # revealed: Self@__new__
        return instance

reveal_type(Child(1, 2))  # revealed: Child

T = TypeVar("T")

class GenericBase(NamedTuple, Generic[T]):
    x: T

class ConcreteChild(GenericBase[str]):
    def __new__(cls, x: str) -> "ConcreteChild":
        instance = super().__new__(cls, x)
        reveal_type(instance)  # revealed: Self@__new__
        return instance

class GenericChild(GenericBase[T]):
    def __new__(cls, x: T) -> Self:
        instance = super().__new__(cls, x)
        reveal_type(instance)  # revealed: @Todo(super in generic class)
        return instance

reveal_type(GenericChild(x=3.14))  # revealed: GenericChild[int | float]
```
