# List all members

This test suite acts as a set of unit tests for our `ide_support::all_members` routine, which lists
all members available on a given type. This routine is used for autocomplete suggestions.

## Basic functionality

<!-- snapshot-diagnostics -->

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
from typing_extensions import reveal_type
from ty_extensions import all_members

reveal_type(all_members("a"))  # error: [revealed-type]
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

But instance attributes can not be accessed this way:

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

So far, we do not include synthetic members of dataclasses.

```py
from ty_extensions import has_member, static_assert
from dataclasses import dataclass

@dataclass(order=True)
class Person:
    age: int
    name: str

static_assert(has_member(Person, "name"))
static_assert(has_member(Person, "age"))

# These are always available, since they are also defined on `object`:
static_assert(has_member(Person, "__init__"))
static_assert(has_member(Person, "__repr__"))
static_assert(has_member(Person, "__eq__"))

# TODO: this should ideally be available:
static_assert(has_member(Person, "__lt__"))  # error: [static-assert-error]
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
