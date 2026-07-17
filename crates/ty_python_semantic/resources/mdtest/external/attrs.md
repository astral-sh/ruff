# attrs

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["attrs==25.4.0"]
```

## Basic class (`attr`)

```py
import attr

@attr.s
class User:
    id: int = attr.ib()
    name: str = attr.ib()

user = User(id=1, name="John Doe")

reveal_type(user.id)  # revealed: int
reveal_type(user.name)  # revealed: str
```

## Basic class (`define`)

```py
from attrs import define, field

@define
class User:
    id: int = field()
    internal_name: str = field(alias="name")

user = User(id=1, name="John Doe")
reveal_type(user.id)  # revealed: int
reveal_type(user.internal_name)  # revealed: str
```

## Field discovery and `auto_attribs`

Classic attrs classes only collect field specifiers by default. `attr.ib(type=...)` supplies the
field type when the assignment has no annotation.

```py
import attr
from typing import reveal_type

@attr.s
class Classic:
    ignored: str
    value = attr.ib(type=int)
    items = attr.ib(type="list[int]")
    other = attr.ib(type="Other")
    optional = attr.ib(type=int | None, default=None)

class Other: ...

reveal_type(
    Classic.__init__  # revealed: (self: Classic, value: int, items: list[int], other: Other, optional: int | None = ...) -> None
)

classic = Classic(value=1, items=[1], other=Other())
reveal_type(classic.value)  # revealed: int
reveal_type(classic.items)  # revealed: list[int]
reveal_type(classic.other)  # revealed: Other
reveal_type(classic.optional)  # revealed: int | None

# error: [invalid-argument-type]
Classic(value="not an int", items=[1], other=Other())
```

Explicit `auto_attribs=True` makes annotations into fields, while `auto_attribs=False` keeps only
field specifiers. The classic decorator aliases have the same defaults.

```py
import attr

@attr.s(auto_attribs=True)
class Annotated:
    value: int
    label: str = "label"

@attr.attributes(auto_attribs=False)
class Explicit:
    ignored: int
    value = attr.ib(type=str)

@attr.dataclass
class Dataclass:
    value: bytes

@attr.s
class Overwritten:
    value = attr.ib(type=int)
    value = 1

reveal_type(Annotated.__init__)  # revealed: (self: Annotated, value: int, label: str = "label") -> None
reveal_type(Explicit.__init__)  # revealed: (self: Explicit, value: str) -> None
reveal_type(Dataclass.__init__)  # revealed: (self: Dataclass, value: bytes) -> None
reveal_type(Overwritten.__init__)  # revealed: (self: Overwritten) -> None
```

Modern attrs decorators infer `auto_attribs` independently for every class: an unannotated field
specifier switches that class to field-specifier-only collection. `attrs.field(type=...)` is
metadata, so it does not supply the static field type.

```py
import attr
import attrs

@attrs.define
class Inferred:
    ignored: int
    untyped = attrs.field()
    typed = attr.ib(type=str)

@attrs.mutable
class AnnotatedBase:
    value: int

@attrs.define
class InferredChild(AnnotatedBase):
    ignored: bool
    label = attr.ib(type=str)

@attrs.define(auto_attribs=False)
class Explicit:
    ignored: int
    value = attrs.field(type=int)

reveal_type(Inferred.__init__)  # revealed: (self: Inferred, untyped: Any, typed: str) -> None
reveal_type(InferredChild.__init__)  # revealed: (self: InferredChild, value: int, label: str) -> None
reveal_type(Explicit.__init__)  # revealed: (self: Explicit, value: Any) -> None
```

## Underscore-prefixed fields

attrs strips leading underscores from generated constructor and replacement parameters. The instance
attribute keeps its original name, and an explicit alias takes precedence.

```py
import attr
import attrs
from typing import reveal_type

@attr.s(auto_attribs=True)
class Base:
    _base: int
    __secret: bytes

@attrs.define
class Example(Base):
    _private: str
    __both__: bytes
    _aliased: bool = attrs.field(alias="custom", kw_only=True)

reveal_type(
    Example.__init__  # revealed: (self: Example, base: int, Base__secret: bytes, private: str, both__: bytes, *, custom: bool) -> None
)
reveal_type(
    Example.__replace__  # revealed: (self: Example, *, base: int = ..., Base__secret: bytes = ..., private: str = ..., both__: bytes = ..., custom: bool = ...) -> Example
)

example = Example(base=1, Base__secret=b"secret", private="value", both__=b"value", custom=True)
reveal_type(example._base)  # revealed: int
reveal_type(example._private)  # revealed: str
reveal_type(example.__both__)  # revealed: bytes
reveal_type(example._aliased)  # revealed: bool

# error: [missing-argument]
# error: [unknown-argument]
Example(base=1, Base__secret=b"secret", _private="value", both__=b"value", custom=True)
```

## Usage of `field` parameters

```py
from attrs import define, field

def serialize_data(data: dict[str, int]) -> bytes:
    raise NotImplementedError

@define
class Product:
    id: int = field(init=False)
    name: str = field()
    price_cent: int = field(kw_only=True)
    data: bytes = field(converter=serialize_data, kw_only=True)

reveal_type(Product.__init__)  # revealed: (self: Product, name: str, *, price_cent: int, data: dict[str, int]) -> None

p = Product(name="Gadget", price_cent=1999, data={"a": 1})

p.data = {"b": 2}
reveal_type(p.data)  # revealed: bytes

p.data = "not a dict"  # error: [invalid-assignment]
```

## Dedicated support for the `default` decorator?

We currently do not support this:

```py
from attrs import define, field

@define
class Person:
    id: int = field()
    name: str = field()

    # error: [call-non-callable] "Object of type `_MISSING_TYPE` is not callable"
    @id.default
    def _default_id(self) -> int:
        raise NotImplementedError

# error: [missing-argument] "No argument provided for required parameter `id`"
person = Person(name="Alice")
reveal_type(person.id)  # revealed: int
reveal_type(person.name)  # revealed: str
```
