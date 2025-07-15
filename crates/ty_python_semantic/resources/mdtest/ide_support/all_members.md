# List all members

## Basic functionality

<!-- snapshot-diagnostics -->

The `ty_extensions.all_members` function allows access to a tuple of accessible members/attributes
on a given object. For example, all member functions of `str` are available on `"a"`:

```py
from ty_extensions import all_members, static_assert

members_of_str = all_members("a")

static_assert("replace" in members_of_str)
static_assert("startswith" in members_of_str)
static_assert("isupper" in members_of_str)
```

Similarly, special members such as `__add__` are also available:

```py
static_assert("__add__" in members_of_str)
static_assert("__gt__" in members_of_str)
```

Members of base classes are also included (these dunder methods are defined on `object`):

```py
static_assert("__doc__" in members_of_str)
static_assert("__repr__" in members_of_str)
```

Non-existent members are not included:

```py
static_assert("non_existent" not in members_of_str)
```

Note: The full list of all members is relatively long, but `reveal_type` can theoretically be used
to see them all:

```py
from typing_extensions import reveal_type

reveal_type(members_of_str)  # error: [revealed-type]
```

## Kinds of types

### Class instances

For instances of classes, `all_members` returns class members and implicit instance members of all
classes in the MRO:

```py
from ty_extensions import all_members, static_assert

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

members_of_instance = all_members(C())

static_assert("base_class_attr" in members_of_instance)
static_assert("intermediate_attr" in members_of_instance)
static_assert("class_attr" in members_of_instance)

static_assert("base_instance_attr" in members_of_instance)
static_assert("intermediate_instance_attr" in members_of_instance)
static_assert("instance_attr" in members_of_instance)

static_assert("f_base" in members_of_instance)
static_assert("f_intermediate" in members_of_instance)
static_assert("f_c" in members_of_instance)

static_assert("property_attr" in members_of_instance)
static_assert("class_method" in members_of_instance)
static_assert("static_method" in members_of_instance)

static_assert("non_existent" not in members_of_instance)
```

### Class objects

Class-level attributes can also be accessed through the class itself:

```py
from ty_extensions import all_members, static_assert

class Base:
    base_attr: int = 1

class C(Base):
    class_attr: str = "c"

    def f(self):
        self.instance_attr = True

members_of_class = all_members(C)

static_assert("class_attr" in members_of_class)
static_assert("base_attr" in members_of_class)

static_assert("non_existent" not in members_of_class)
```

But instance attributes can not be accessed this way:

```py
static_assert("instance_attr" not in members_of_class)
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

static_assert("meta_base_attr" in all_members(D))
static_assert("meta_attr" in all_members(D))
static_assert("base_attr" in all_members(D))
static_assert("class_attr" in all_members(D))
```

### Generic classes

```py
from ty_extensions import all_members, static_assert
from typing import Generic, TypeVar

T = TypeVar("T")

class C(Generic[T]):
    base_attr: T

static_assert("base_attr" in all_members(C[int]))
static_assert("base_attr" in all_members(C[int]()))
```

### Other instance-like types

```py
from ty_extensions import all_members, static_assert
from typing_extensions import LiteralString

static_assert("__xor__" in all_members(True))
static_assert("bit_length" in all_members(1))
static_assert("startswith" in all_members("a"))
static_assert("__buffer__" in all_members(b"a"))
static_assert("is_integer" in all_members(3.14))

def _(literal_string: LiteralString):
    static_assert("startswith" in all_members(literal_string))

static_assert("count" in all_members(("some", "tuple", 1, 2)))

static_assert("__doc__" in all_members(len))
static_assert("__doc__" in all_members("a".startswith))
```

### Enums

```py
from ty_extensions import all_members, static_assert
from enum import Enum

class Answer(Enum):
    NO = 0
    YES = 1

static_assert("NO" in all_members(Answer))
static_assert("YES" in all_members(Answer))
static_assert("__members__" in all_members(Answer))
```

### Unions

For unions, `all_members` will only return members that are available on all elements of the union.

```py
from ty_extensions import all_members, static_assert

class A:
    on_both: int = 1
    only_on_a: str = "a"

class B:
    on_both: int = 2
    only_on_b: str = "b"

def f(union: A | B):
    static_assert("on_both" in all_members(union))
    static_assert("only_on_a" not in all_members(union))
    static_assert("only_on_b" not in all_members(union))
```

### Intersections

#### Only positive types

Conversely, for intersections, `all_members` will list members that are available on any of the
elements:

```py
from ty_extensions import all_members, static_assert

class A:
    on_both: int = 1
    only_on_a: str = "a"

class B:
    on_both: int = 2
    only_on_b: str = "b"

def f(intersection: object):
    if isinstance(intersection, A):
        if isinstance(intersection, B):
            static_assert("on_both" in all_members(intersection))
            static_assert("only_on_a" in all_members(intersection))
            static_assert("only_on_b" in all_members(intersection))
```

#### With negative types

It also works when negative types are introduced:

```py
from ty_extensions import all_members, static_assert

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
                static_assert("on_all" in all_members(intersection))
                static_assert("only_on_a" in all_members(intersection))
                static_assert("only_on_b" in all_members(intersection))
                static_assert("only_on_c" not in all_members(intersection))
                static_assert("only_on_ab" in all_members(intersection))
                static_assert("only_on_ac" in all_members(intersection))
                static_assert("only_on_bc" in all_members(intersection))
```

## Modules

### Basic support with sub-modules

`all_members` can also list attributes on modules:

```py
from ty_extensions import all_members, static_assert
import math

static_assert("pi" in all_members(math))
static_assert("cos" in all_members(math))
```

This also works for submodules:

```py
import os

static_assert("path" in all_members(os))

import os.path

static_assert("join" in all_members(os.path))
```

Special members available on all modules are also included:

```py
static_assert("__name__" in all_members(math))
static_assert("__doc__" in all_members(math))
```

### `__all__` is not respected for direct module access

`foo.py`:

```py
from ty_extensions import all_members, static_assert

import bar

static_assert("lion" in all_members(bar))
static_assert("tiger" in all_members(bar))
```

`bar.py`:

```py
__all__ = ["lion"]

lion = 1
tiger = 1
```

### `__all__` is respected for glob imports

`foo.py`:

```py
from ty_extensions import all_members, static_assert

import bar

static_assert("lion" in all_members(bar))
static_assert("tiger" not in all_members(bar))
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
from ty_extensions import all_members, static_assert

import module

static_assert("evaluate" in all_members(module))
static_assert("Optional" not in all_members(module))
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
from ty_extensions import all_members, static_assert

static_assert("bit_count" not in all_members(42))
```

### 3.10

```toml
[environment]
python-version = "3.10"
```

```py
from ty_extensions import all_members, static_assert

static_assert("bit_count" in all_members(42))
```

## Failures cases

### Dynamically added members

Dynamically added members can not be accessed:

```py
from ty_extensions import all_members, static_assert

class C:
    static_attr = 1

    def __setattr__(self, name: str, value: str) -> None:
        pass

    def __getattr__(self, name: str) -> str:
        return "a"

c = C()
c.dynamic_attr = "a"

static_assert("static_attr" in all_members(c))
static_assert("dynamic_attr" not in all_members(c))
```

### Dataclasses

So far, we do not include synthetic members of dataclasses.

```py
from ty_extensions import all_members, static_assert
from dataclasses import dataclass

@dataclass(order=True)
class Person:
    name: str
    age: int

static_assert("name" in all_members(Person))
static_assert("age" in all_members(Person))

# These are always available, since they are also defined on `object`:
static_assert("__init__" in all_members(Person))
static_assert("__repr__" in all_members(Person))
static_assert("__eq__" in all_members(Person))

# TODO: this should ideally be available:
static_assert("__lt__" in all_members(Person))  # error: [static-assert-error]
```

### Attributes not available at runtime

Typeshed includes some attributes in `object` that are not available for some (builtin) types. For
example, `__annotations__` does not exist on `int` at runtime, but it is available as an attribute
on `object` in typeshed:

```py
from ty_extensions import all_members, static_assert

# TODO: this should ideally not be available:
static_assert("__annotations__" not in all_members(3))  # error: [static-assert-error]
```
