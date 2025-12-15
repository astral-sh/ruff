# List all members

This test suite acts as a set of unit tests for our `ide_support::all_members` routine, which lists
all members available on a given type. This routine is used for autocomplete suggestions.

## Basic functionality

The `ty_extensions.all_members` and `ty_extensions.has_member` functions expose a Python-level API
that can be used to query which attributes `ide_support::all_members` understands as being available
on a given object. For example, all member functions of `str` are available on `"a"`. The Python API
`all_members` returns a tuple of all available members; `has_member` returns `Literal[True]` if a
given member is present in that tuple, and `Literal[False]` if not:

```py
from ty_extensions import static_assert, has_member

static_assert(has_member("a", "replace"))
static_assert(has_member("a", "startswith"))
static_assert(has_member("a", "isupper"))
```

Similarly, special members such as `__add__` are also available:

```py
static_assert(has_member("a", "__add__"))
static_assert(has_member("a", "__gt__"))
```

Members of base classes are also included (these dunder methods are defined on `object`):

```py
static_assert(has_member("a", "__doc__"))
static_assert(has_member("a", "__repr__"))
```

Non-existent members are not included:

```py
static_assert(not has_member("a", "non_existent"))
```

The full list of all members is relatively long, but `reveal_type` can be used in combination with
`all_members` to see them all:

```py
from ty_extensions import all_members

# revealed: tuple[Literal["__add__"], Literal["__annotations__"], Literal["__class__"], Literal["__contains__"], Literal["__delattr__"], Literal["__dict__"], Literal["__dir__"], Literal["__doc__"], Literal["__eq__"], Literal["__format__"], Literal["__ge__"], Literal["__getattribute__"], Literal["__getitem__"], Literal["__getnewargs__"], Literal["__gt__"], Literal["__hash__"], Literal["__init__"], Literal["__init_subclass__"], Literal["__iter__"], Literal["__le__"], Literal["__len__"], Literal["__lt__"], Literal["__mod__"], Literal["__module__"], Literal["__mul__"], Literal["__ne__"], Literal["__new__"], Literal["__reduce__"], Literal["__reduce_ex__"], Literal["__repr__"], Literal["__reversed__"], Literal["__rmul__"], Literal["__setattr__"], Literal["__sizeof__"], Literal["__str__"], Literal["__subclasshook__"], Literal["capitalize"], Literal["casefold"], Literal["center"], Literal["count"], Literal["encode"], Literal["endswith"], Literal["expandtabs"], Literal["find"], Literal["format"], Literal["format_map"], Literal["index"], Literal["isalnum"], Literal["isalpha"], Literal["isascii"], Literal["isdecimal"], Literal["isdigit"], Literal["isidentifier"], Literal["islower"], Literal["isnumeric"], Literal["isprintable"], Literal["isspace"], Literal["istitle"], Literal["isupper"], Literal["join"], Literal["ljust"], Literal["lower"], Literal["lstrip"], Literal["maketrans"], Literal["partition"], Literal["removeprefix"], Literal["removesuffix"], Literal["replace"], Literal["rfind"], Literal["rindex"], Literal["rjust"], Literal["rpartition"], Literal["rsplit"], Literal["rstrip"], Literal["split"], Literal["splitlines"], Literal["startswith"], Literal["strip"], Literal["swapcase"], Literal["title"], Literal["translate"], Literal["upper"], Literal["zfill"]]
reveal_type(all_members("a"))
```

## Kinds of types

### Class instances

For instances of classes, class members and implicit instance members of all superclasses are
understood as being available:

```py
from ty_extensions import has_member, static_assert

class Base:
    base_class_attr: int = 1

    def f_base(self):
        self.base_instance_attr: str = "Base"

class Intermediate(Base):
    intermediate_attr: int = 2

    def f_intermediate(self):
        self.intermediate_instance_attr: str = "Intermediate"

class C(Intermediate):
    class_attr: int = 3

    def f_c(self):
        self.instance_attr = "C"

    @property
    def property_attr(self) -> int:
        return 1

    @classmethod
    def class_method(cls) -> int:
        return 1

    @staticmethod
    def static_method() -> int:
        return 1

static_assert(has_member(C(), "base_class_attr"))
static_assert(has_member(C(), "intermediate_attr"))
static_assert(has_member(C(), "class_attr"))

static_assert(has_member(C(), "base_instance_attr"))
static_assert(has_member(C(), "intermediate_instance_attr"))
static_assert(has_member(C(), "instance_attr"))

static_assert(has_member(C(), "f_base"))
static_assert(has_member(C(), "f_intermediate"))
static_assert(has_member(C(), "f_c"))

static_assert(has_member(C(), "property_attr"))
static_assert(has_member(C(), "class_method"))
static_assert(has_member(C(), "static_method"))

static_assert(not has_member(C(), "non_existent"))
```

### Class objects

```toml
[environment]
python-version = "3.12"
```

Class-level attributes can also be accessed through the class itself:

```py
from ty_extensions import has_member, static_assert

class Base:
    base_attr: int = 1

class C(Base):
    class_attr: str = "c"

    def f(self):
        self.instance_attr = True

static_assert(has_member(C, "class_attr"))
static_assert(has_member(C, "base_attr"))

static_assert(not has_member(C, "non_existent"))
```

But instance attributes cannot be accessed this way:

```py
static_assert(not has_member(C, "instance_attr"))
```

When a class has a metaclass, members of that metaclass (and bases of that metaclass) are also
accessible:

```py
class MetaBase(type):
    meta_base_attr = 1

class Meta(MetaBase):
    meta_attr = 2

class D(Base, metaclass=Meta):
    class_attr = 3

static_assert(has_member(D, "meta_base_attr"))
static_assert(has_member(D, "meta_attr"))
static_assert(has_member(D, "base_attr"))
static_assert(has_member(D, "class_attr"))

def _(x: type[D]):
    static_assert(has_member(x, "meta_base_attr"))
    static_assert(has_member(x, "meta_attr"))
    static_assert(has_member(x, "base_attr"))
    static_assert(has_member(x, "class_attr"))

def _[T: D](x: type[T]):
    static_assert(has_member(x, "meta_base_attr"))
    static_assert(has_member(x, "meta_attr"))
    static_assert(has_member(x, "base_attr"))
    static_assert(has_member(x, "class_attr"))
```

### Generic classes

```py
from ty_extensions import has_member, static_assert
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    base_attr: T

static_assert(has_member(C[int], "base_attr"))
static_assert(has_member(C[int](), "base_attr"))
```

Generic classes can also have metaclasses:

```py
class Meta(type):
    FOO = 42

class E(Generic[T], metaclass=Meta): ...

static_assert(has_member(E[int], "FOO"))

def f(x: type[E[str]]):
    static_assert(has_member(x, "FOO"))
```

### `type[Any]` and `Any`

`type[Any]` has all members of `type`.

```py
from typing import Any
from ty_extensions import has_member, static_assert

def f(x: type[Any]):
    static_assert(has_member(x, "__base__"))
    static_assert(has_member(x, "__qualname__"))
```

`Any` has all members of `object`, since it is a subtype of `object`:

```py
def f(x: Any):
    static_assert(has_member(x, "__repr__"))
```

### Other instance-like types

```py
from ty_extensions import has_member, static_assert
from typing_extensions import LiteralString

static_assert(has_member(True, "__xor__"))
static_assert(has_member(1, "bit_length"))
static_assert(has_member("a", "startswith"))
static_assert(has_member(b"a", "__buffer__"))
static_assert(has_member(3.14, "is_integer"))

def _(literal_string: LiteralString):
    static_assert(has_member(literal_string, "startswith"))

static_assert(has_member(("some", "tuple", 1, 2), "count"))

static_assert(has_member(len, "__doc__"))
static_assert(has_member("a".startswith, "__doc__"))
```

### Enums

```py
from ty_extensions import has_member, static_assert
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

static_assert(has_member(Answer, "NO"))
static_assert(has_member(Answer, "YES"))
static_assert(has_member(Answer, "__members__"))
```

### TypedDicts

```py
from ty_extensions import has_member, static_assert
from typing import TypedDict

class Person(TypedDict):
    name: str
    age: int | None

static_assert(not has_member(Person, "name"))
static_assert(has_member(Person, "keys"))
static_assert(has_member(Person, "__total__"))

def _(person: Person):
    static_assert(not has_member(person, "name"))
    static_assert(not has_member(person, "__total__"))
    static_assert(has_member(person, "keys"))

    # type(person) is `dict` at runtime, so `__total__` is not available:
    static_assert(not has_member(type(person), "name"))
    static_assert(not has_member(type(person), "__total__"))
    static_assert(has_member(type(person), "keys"))

def _(t_person: type[Person]):
    static_assert(not has_member(t_person, "name"))
    static_assert(has_member(t_person, "__total__"))
    static_assert(has_member(t_person, "keys"))
```

### NamedTuples

```py
from ty_extensions import has_member, static_assert
from typing import NamedTuple, Generic, TypeVar

class Person(NamedTuple):
    id: int
    name: str

static_assert(has_member(Person, "id"))
static_assert(has_member(Person, "name"))

static_assert(has_member(Person, "_make"))
static_assert(has_member(Person, "_asdict"))
static_assert(has_member(Person, "_replace"))

def _(person: Person):
    static_assert(has_member(person, "id"))
    static_assert(has_member(person, "name"))

    static_assert(has_member(person, "_make"))
    static_assert(has_member(person, "_asdict"))
    static_assert(has_member(person, "_replace"))

def _(t_person: type[Person]):
    static_assert(has_member(t_person, "id"))
    static_assert(has_member(t_person, "name"))

    static_assert(has_member(t_person, "_make"))
    static_assert(has_member(t_person, "_asdict"))
    static_assert(has_member(t_person, "_replace"))

T = TypeVar("T")

class Box(NamedTuple, Generic[T]):
    item: T

static_assert(has_member(Box, "item"))

static_assert(has_member(Box, "_make"))
static_assert(has_member(Box, "_asdict"))
static_assert(has_member(Box, "_replace"))

def _(box: Box[int]):
    static_assert(has_member(box, "item"))

    static_assert(has_member(box, "_make"))
    static_assert(has_member(box, "_asdict"))
    static_assert(has_member(box, "_replace"))
```

### Unions

For unions, `ide_support::all_members` only returns members that are available on all elements of
the union.

```py
from ty_extensions import has_member, static_assert

class A:
    on_both: int = 1
    only_on_a: str = "a"

class B:
    on_both: int = 2
    only_on_b: str = "b"

def f(union: A | B):
    static_assert(has_member(union, "on_both"))
    static_assert(not has_member(union, "only_on_a"))
    static_assert(not has_member(union, "only_on_b"))
```

### Intersections

#### Only positive types

Conversely, for intersections, `ide_support::all_members` lists members that are available on any of
the elements:

```py
from ty_extensions import has_member, static_assert

class A:
    on_both: int = 1
    only_on_a: str = "a"

class B:
    on_both: int = 2
    only_on_b: str = "b"

def f(intersection: object):
    if isinstance(intersection, A):
        if isinstance(intersection, B):
            static_assert(has_member(intersection, "on_both"))
            static_assert(has_member(intersection, "only_on_a"))
            static_assert(has_member(intersection, "only_on_b"))
```

#### With negative types

It also works when negative types are introduced:

```py
from ty_extensions import has_member, static_assert

class A:
    on_all: int = 1
    only_on_a: str = "a"
    only_on_ab: str = "a"
    only_on_ac: str = "a"

class B:
    on_all: int = 2
    only_on_b: str = "b"
    only_on_ab: str = "b"
    only_on_bc: str = "b"

class C:
    on_all: int = 3
    only_on_c: str = "c"
    only_on_ac: str = "c"
    only_on_bc: str = "c"

def f(intersection: object):
    if isinstance(intersection, A):
        if isinstance(intersection, B):
            if not isinstance(intersection, C):
                reveal_type(intersection)  # revealed: A & B & ~C
                static_assert(has_member(intersection, "on_all"))
                static_assert(has_member(intersection, "only_on_a"))
                static_assert(has_member(intersection, "only_on_b"))
                static_assert(not has_member(intersection, "only_on_c"))
                static_assert(has_member(intersection, "only_on_ab"))
                static_assert(has_member(intersection, "only_on_ac"))
                static_assert(has_member(intersection, "only_on_bc"))
```

## Modules

### Basic support with sub-modules

`ide_support::all_members` can also list attributes on modules:

```py
from ty_extensions import has_member, static_assert
import math

static_assert(has_member(math, "pi"))
static_assert(has_member(math, "cos"))
```

This also works for submodules:

```py
import os

static_assert(has_member(os, "path"))

import os.path

static_assert(has_member(os.path, "join"))
```

Special members available on all modules are also included:

```py
static_assert(has_member(math, "__name__"))
static_assert(has_member(math, "__doc__"))
```

### `__all__` is not respected for direct module access

`foo.py`:

```py
from ty_extensions import has_member, static_assert

import bar

static_assert(has_member(bar, "lion"))
static_assert(has_member(bar, "tiger"))
```

`bar.py`:

```py
__all__ = ["lion"]

lion = 1
tiger = 1
```

### `__all__` is respected for `*` imports

`foo.py`:

```py
from ty_extensions import has_member, static_assert

import bar

static_assert(has_member(bar, "lion"))
static_assert(not has_member(bar, "tiger"))
```

`bar.py`:

```py
from quux import *
```

`quux.py`:

```py
__all__ = ["lion"]

lion = 1
tiger = 1
```

### `__all__` is respected for stub files

`module.py`:

```py
def evaluate(x=None):
    if x is None:
        return 0
    return x
```

`module.pyi`:

```pyi
from typing import Optional

__all__ = ["evaluate"]

def evaluate(x: Optional[int] = None) -> int: ...
```

`play.py`:

```py
from ty_extensions import has_member, static_assert

import module

static_assert(has_member(module, "evaluate"))
static_assert(not has_member(module, "Optional"))
```

## Conditionally available members

Some members are only conditionally available. For example, `int.bit_count` was only introduced in
Python 3.10:

### 3.9

```toml
[environment]
python-version = "3.9"
```

```py
from ty_extensions import has_member, static_assert

static_assert(not has_member(42, "bit_count"))
```

### 3.10

```toml
[environment]
python-version = "3.10"
```

```py
from ty_extensions import has_member, static_assert

static_assert(has_member(42, "bit_count"))
```

## Failure cases

### Dynamically added members

Dynamically added members cannot be accessed:

```py
from ty_extensions import has_member, static_assert

class C:
    static_attr = 1

    def __setattr__(self, name: str, value: str) -> None:
        pass

    def __getattr__(self, name: str) -> str:
        return "a"

c = C()
c.dynamic_attr = "a"

static_assert(has_member(c, "static_attr"))
static_assert(not has_member(c, "dynamic_attr"))
```

### Dataclasses

#### Basic

For dataclasses, we make sure to include all synthesized members:

```toml
[environment]
python-version = "3.9"
```

```py
from ty_extensions import has_member, static_assert
from dataclasses import dataclass

@dataclass
class Person:
    age: int
    name: str

static_assert(has_member(Person, "name"))
static_assert(has_member(Person, "age"))

static_assert(has_member(Person, "__dataclass_fields__"))
static_assert(has_member(Person, "__dataclass_params__"))

# These are always available, since they are also defined on `object`:
static_assert(has_member(Person, "__init__"))
static_assert(has_member(Person, "__repr__"))
static_assert(has_member(Person, "__eq__"))
static_assert(has_member(Person, "__ne__"))

# There are not available, unless `order=True` is set:
static_assert(not has_member(Person, "__lt__"))
static_assert(not has_member(Person, "__le__"))
static_assert(not has_member(Person, "__gt__"))
static_assert(not has_member(Person, "__ge__"))

# These are not available, unless `slots=True`, `weakref_slot=True` are set:
static_assert(not has_member(Person, "__slots__"))
static_assert(not has_member(Person, "__weakref__"))

# Not available before Python 3.13:
static_assert(not has_member(Person, "__replace__"))
```

The same behavior applies to instances of dataclasses:

```py
def _(person: Person):
    static_assert(has_member(person, "name"))
    static_assert(has_member(person, "age"))

    static_assert(has_member(person, "__dataclass_fields__"))
    static_assert(has_member(person, "__dataclass_params__"))

    static_assert(has_member(person, "__init__"))
    static_assert(has_member(person, "__repr__"))
    static_assert(has_member(person, "__eq__"))
    static_assert(has_member(person, "__ne__"))

    static_assert(not has_member(person, "__lt__"))
    static_assert(not has_member(person, "__le__"))
    static_assert(not has_member(person, "__gt__"))
    static_assert(not has_member(person, "__ge__"))

    static_assert(not has_member(person, "__slots__"))

    static_assert(not has_member(person, "__replace__"))
```

#### `__init__`, `__repr__` and `__eq__`

`__init__`, `__repr__` and `__eq__` are always available (via `object`), even when `init=False`,
`repr=False` and `eq=False` are set:

```py
from ty_extensions import has_member, static_assert
from dataclasses import dataclass

@dataclass(init=False, repr=False, eq=False)
class C:
    x: int

static_assert(has_member(C, "__init__"))
static_assert(has_member(C, "__repr__"))
static_assert(has_member(C, "__eq__"))
static_assert(has_member(C, "__ne__"))
static_assert(has_member(C(), "__init__"))
static_assert(has_member(C(), "__repr__"))
static_assert(has_member(C(), "__eq__"))
static_assert(has_member(C(), "__ne__"))
```

#### `order=True`

When `order=True` is set, comparison dunder methods become available:

```py
from ty_extensions import has_member, static_assert
from dataclasses import dataclass

@dataclass(order=True)
class C:
    x: int

static_assert(has_member(C, "__lt__"))
static_assert(has_member(C, "__le__"))
static_assert(has_member(C, "__gt__"))
static_assert(has_member(C, "__ge__"))

def _(c: C):
    static_assert(has_member(c, "__lt__"))
    static_assert(has_member(c, "__le__"))
    static_assert(has_member(c, "__gt__"))
    static_assert(has_member(c, "__ge__"))
```

#### `slots=True`

When `slots=True`, the corresponding dunder attribute becomes available:

```py
from ty_extensions import has_member, static_assert
from dataclasses import dataclass

@dataclass(slots=True)
class C:
    x: int

static_assert(has_member(C, "__slots__"))
static_assert(has_member(C(1), "__slots__"))
```

#### `weakref_slot=True`

When `weakref_slot=True` on Python >=3.11, the corresponding dunder attribute becomes available:

```toml
[environment]
python-version = "3.11"
```

```py
from ty_extensions import has_member, static_assert
from dataclasses import dataclass

@dataclass(slots=True, weakref_slot=True)
class C:
    x: int

static_assert(has_member(C, "__weakref__"))
static_assert(has_member(C(1), "__weakref__"))
```

#### `__replace__` in Python 3.13+

Since Python 3.13, dataclasses have a `__replace__` method:

```toml
[environment]
python-version = "3.13"
```

```py
from ty_extensions import has_member, static_assert
from dataclasses import dataclass

@dataclass
class C:
    x: int

static_assert(has_member(C, "__replace__"))

def _(c: C):
    static_assert(has_member(c, "__replace__"))
```

#### `__match_args__`

Since Python 3.10, dataclasses have a `__match_args__` attribute:

```toml
[environment]
python-version = "3.10"
```

```py
from ty_extensions import has_member, static_assert
from dataclasses import dataclass

@dataclass
class C:
    x: int

static_assert(has_member(C, "__match_args__"))

def _(c: C):
    static_assert(has_member(c, "__match_args__"))
```

### Attributes added on new Python versions are not synthesized on older Python versions

```toml
[environment]
python-version = "3.9"
```

```py
from dataclasses import dataclass
from ty_extensions import static_assert, has_member

# TODO: these parameters don't exist on Python 3.9;
# we should emit a diagnostic (or two)
@dataclass(slots=True, weakref_slot=True)
class F: ...

static_assert(not has_member(F, "__slots__"))
static_assert(not has_member(F, "__match_args__"))

# In actual fact, all non-slotted instances have this attribute
# (and even slotted instances can, if `__weakref__` is included in `__slots__`);
# we could possibly model that more fully?
# It's not added by the dataclasses machinery, though
static_assert(not has_member(F(), "__weakref__"))
```

### Attributes not available at runtime

Typeshed includes some attributes in `object` that are not available for some (builtin) types. For
example, `__annotations__` does not exist on `int` at runtime, but it is available as an attribute
on `object` in typeshed:

```py
from ty_extensions import has_member, static_assert

# TODO: this should ideally not be available:
static_assert(not has_member(3, "__annotations__"))  # error: [static-assert-error]
```
