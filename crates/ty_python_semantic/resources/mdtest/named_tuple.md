# `NamedTuple`

`NamedTuple` is a type-safe way to define named tuples — a tuple where each field can be accessed by
name, and not just by its numeric position within the tuple:

## `typing.NamedTuple`

### Basics

```py
from typing import NamedTuple
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

Alternative functional syntax:

```py
Person2 = NamedTuple("Person", [("id", int), ("name", str)])
alice2 = Person2(1, "Alice")

# TODO: should be an error
Person2(1)

reveal_type(alice2.id)  # revealed: @Todo(functional `NamedTuple` syntax)
reveal_type(alice2.name)  # revealed: @Todo(functional `NamedTuple` syntax)
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

## Attributes on `NamedTuple`

The following attributes are available on `NamedTuple` classes / instances:

```py
from typing import NamedTuple

class Person(NamedTuple):
    name: str
    age: int | None = None

reveal_type(Person._field_defaults)  # revealed: dict[str, Any]
reveal_type(Person._fields)  # revealed: tuple[Literal["name"], Literal["age"]]
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
    # revealed: Overload[(value: tuple[object, ...], /) -> tuple[object, ...], (value: tuple[_T@__add__, ...], /) -> tuple[object, ...]]
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
