# attrs

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["attrs==26.1.0"]
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

## Field metadata and the `AttrsInstance` protocol

attrs stores field metadata in the `__attrs_attrs__` class variable. Both the class and its
instances expose this tuple.

```py
import attr
import attrs

@attr.s
class User1:
    name: str = attr.ib()

@attrs.define
class User2:
    name: str

reveal_type(User1.__attrs_attrs__)  # revealed: tuple[Attribute[Unknown], ...]
reveal_type(User2.__attrs_attrs__)  # revealed: tuple[Attribute[Unknown], ...]

user1 = User1("Ada")
user2 = User2("Ada")

reveal_type(user1.__attrs_attrs__[0].name)  # revealed: str
reveal_type(user2.__attrs_attrs__[0].name)  # revealed: str
```

The `AttrsInstance` protocol requires `__attrs_attrs__`. `attrs.fields` accepts
`type[AttrsInstance] | AttrsInstance`, while `attrs.asdict` expects an `AttrsInstance`.

```py
attrs.fields(User1)
attrs.fields(User2)
attrs.fields(user1)
attrs.fields(user2)

attrs.asdict(user1)
attrs.asdict(user2)

class Child1(User1): ...
class Child2(User2): ...

attr.fields(Child1)
attr.fields(Child2)
```

An ordinary dataclass lacks the required member.

```py
from dataclasses import dataclass

@dataclass
class Other:
    name: str

attrs.fields(Other)  # error: [invalid-argument-type]
attrs.asdict(Other("Ada"))  # error: [invalid-argument-type]
```

`attrs.has` narrows a class to `type[AttrsInstance]`.

```py
def inspect_class(cls: type):
    if attrs.has(cls):
        reveal_type(cls.__attrs_attrs__)  # revealed: Any
```

## Private fields

attrs strips leading underscores from field names when generating constructor parameters.

```py
import copy
from attrs import define, field
import attr

@attr.s
class User1:
    _id: int = attr.ib()
    name: str = attr.ib()

@define
class User2:
    _id: int
    name: str

reveal_type(User1.__init__)  # revealed: (self: User1, /, id: int, name: str) -> None
reveal_type(User2.__init__)  # revealed: (self: User2, /, id: int, name: str) -> None

user1 = User1(id=1, name="John Doe")
user2 = User2(id=1, name="John Doe")

# error: [missing-argument]
# error: [unknown-argument]
User1(_id=1, name="John Doe")
```

Fields are still accessed through the original attribute name, and they cannot be accessed using the
stripped name:

```py
reveal_type(user1._id)  # revealed: int
reveal_type(user1.name)  # revealed: str

reveal_type(user2._id)  # revealed: int
reveal_type(user2.name)  # revealed: str

user1.id  # error: [missing-attribute]
user2.id  # error: [missing-attribute]
```

The signature for `__replace__` also uses the stripped name:

```py
copy.replace(user1, id=2)  # ok
copy.replace(user2, id=2)  # ok
copy.replace(user2, _id=3)  # error: [unknown-argument]
```

An explicit alias overwrites underscore-stripping:

```py
@define
class AliasedUser:
    _name: str = field(alias="_name")

reveal_type(AliasedUser.__init__)  # revealed: (self: AliasedUser, /, _name: str) -> None

AliasedUser(_name="John Doe")  # ok
```

For double-underscore field names (that do not end with double underscores), Python mangles `__id`
to `_<class-name>__id` before attrs strips the leading underscore. So the effective constructor
parameter name for `__id` becomes `<class-name>__id`
(<https://github.com/python-attrs/attrs/issues/619>). A "dunder" attribute is not mangled, but
leading underscores are still stripped, so `__name__` becomes `name__`.

```py
@define
class VeryPrivateUser:
    __id: int
    __name__: str

reveal_type(VeryPrivateUser.__init__)  # revealed: (self: VeryPrivateUser, /, VeryPrivateUser__id: int, name__: str) -> None
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

reveal_type(Product.__init__)  # revealed: (self: Product, /, name: str, *, price_cent: int, data: dict[str, int]) -> None

p = Product(name="Gadget", price_cent=1999, data={"a": 1})

p.data = {"b": 2}
reveal_type(p.data)  # revealed: bytes

p.data = "not a dict"  # error: [invalid-assignment]
```

The constructor parameters are optional keywords in `copy.replace`.

```py
from copy import replace

reveal_type(replace(p, name="Widget"))  # revealed: Product
replace(p, id=1)  # error: [unknown-argument]
```

## Field validators

attrs supports validator functions, which can either be specified using `field(validator=...)` or by
decorating a method with `@<field>.validator`:

```py
import attr
from attrs import define, field, validators

@attr.s
class User1:
    name: str = attr.ib(validator=attr.validators.instance_of(str))
    age: int = attr.ib()

    @age.validator
    def non_negative(self, attr: object, value: int) -> None:
        if value < 0:
            raise ValueError

@define
class User2:
    name: str = field(validator=validators.instance_of(str))
    age: int = field()

    @age.validator
    def non_negative(self, attr: object, value: int) -> None:
        if value < 0:
            raise ValueError

user1 = User1(name="John Doe", age=30)
user2 = User2(name="John Doe", age=30)

reveal_type(user1.age)  # revealed: int
reveal_type(user2.age)  # revealed: int
```

The decorator `@<field>.validator` only registers the method as a validator. It does not modify the
signature of the function:

```py
reveal_type(user1.non_negative)  # revealed: bound method User1.non_negative(attr: object, value: int) -> None
reveal_type(user2.non_negative)  # revealed: bound method User2.non_negative(attr: object, value: int) -> None
```

We could plausibly go further here and also verify that the validator function's input type matches
the type of the field it is validating:

```py
@define
class Wrong:
    age: int = field()

    # TODO: emit an error here
    @age.validator
    def wrong_validator(self, attr: object, value: str) -> None:
        if not value:
            raise ValueError
```

## Field defaults

A default value can be specified using a value on the right-hand side, or using the
`@<field>.default` decorator.

```py
import attr
from attrs import define, field

@define
class Person:
    name: str = "Unnamed"
    id: int = field()

    @id.default
    def _default_id(self) -> int:
        return 0

reveal_type(Person.__init__)  # revealed: (self: Person, /, name: str = "Unnamed", id: int = ...) -> None

person = Person()
```

The types of the fields are not affected by this:

```py
reveal_type(person.id)  # revealed: int
reveal_type(person.name)  # revealed: str
```

The `@<field>.default` decorator also doesn't modify the signature of the method it decorates:

```py
reveal_type(person._default_id)  # revealed: bound method Person._default_id() -> int
```

Only the field named in the decorator gets a default.

```py
@attr.s
class Range:
    start: int = attr.ib()
    end: int = attr.ib()

    @end.default
    def _default_end(self) -> int:
        return self.start

Range(1)  # ok
Range()  # error: [missing-argument] "No argument provided for required parameter `start`"
```

A required positional field cannot follow a field with a default.

```py
@define
class Invalid:
    x: int = field()
    y: int  # error: [invalid-dataclass-field-order]

    @x.default
    def default_x(self) -> int:
        return 0
```

Similar to above, we could also verify that the return type of the default method matches the type
of the field it is providing a default for:

```py
@define
class WrongDefault:
    age: int = field()

    # TODO: emit an error here
    @age.default
    def _default_age(self) -> str:
        return "0"
```

## Field converters

A field converter can be specified with `field(converter=...)` or by decorating a method with
`@<field>.converter`. The method receives the instance, the field, and the value to convert.

```py
from attrs import define, field

def decode_name(value: bytes) -> str:
    return value.decode()

@define
class Person:
    name: str = field(converter=decode_name)
    age: int = field()

    @age.converter
    def _convert_age(self, attr: object, value: str | float) -> int:
        return int(value)

reveal_type(Person.__init__)  # revealed: (self: Person, /, name: bytes, age: str | float) -> None

person = Person(name=b"Ada", age="36")
Person(name=b"Ada", age=36.5)
Person(name="Ada", age=36.5)  # error: [invalid-argument-type]
Person(name=b"Ada", age=None)  # error: [invalid-argument-type]
Person(name=b"Ada")  # error: [missing-argument]
```

Assignments accept the converter's input type. Reading the field gives its declared type.

```py
reveal_type(person.name)  # revealed: str
reveal_type(person.age)  # revealed: int

person.age = "37"
reveal_type(person.age)  # revealed: int
person.age = None  # error: [invalid-assignment]
```

The decorator preserves the method's signature.

```py
reveal_type(person._convert_age)  # revealed: bound method Person._convert_age(attr: object, value: str | float) -> int
```

Decorated converters run in definition order. The first converter determines the input type.

```py
@define
class User:
    age: int = field()

    @age.converter
    def _decode_age(self, attr: object, value: bytes) -> str:
        return value.decode()

    @age.converter
    def _parse_age(self, attr: object, value: str) -> int:
        return int(value)

reveal_type(User.__init__)  # revealed: (self: User, /, age: bytes) -> None
reveal_type(User(b"36").age)  # revealed: int
```

## Custom initialization

With `init=False`, attrs generates an `__attrs_init__` method that a custom `__init__` can call to
initialize the fields, even on a frozen class.

```py
from attrs import frozen

@frozen(init=False)
class User:
    name: str
    role: str = "member"

    def __init__(self, first_name: str, last_name: str) -> None:
        self.__attrs_init__(name=f"{first_name} {last_name}")

User("John", "Doe")  # ok
User("John")  # error: [missing-argument]

reveal_type(User.__attrs_init__)  # revealed: (self: User, /, name: str, role: str = "member") -> None
```

For classes without a custom `__init__`, attrs does not generate `__attrs_init__` when `init` is
omitted or set to `True`.

```py
from attrs import define

@define
class Person:
    name: str

Person.__attrs_init__  # error: [missing-attribute]
```

attrs also generates an `__attrs_init__` method when `init` is omitted but the class has a custom
`__init__`.

```py
@define
class Guest:
    name: str

    def __init__(self) -> None:
        # TODO: Recognize the generated `__attrs_init__` without an explicit `init=False`.
        self.__attrs_init__(name="Guest")  # error: [missing-attribute]
```

## Unannotated fields

attrs supports field definitions without annotations. In this case, the `type` argument can be used
to declare the field type.

In this example, fields without a `type=` declaration have the type `Unknown`.

```py
import attr

@attr.s
class User1:
    id = attr.ib(type=int)
    name = attr.ib()

# TODO: Include the unannotated fields in the constructor signature.
reveal_type(User1.__init__)  # revealed: (self: User1, /) -> None

# TODO: No error here.
# error: [unknown-argument]
# error: [unknown-argument]
user = User1(id=1, name="Alice")
reveal_type(user.id)  # revealed: int
reveal_type(user.name)  # revealed: Any

# TODO: Report an invalid argument type for `id`.
# error: [unknown-argument]
# error: [unknown-argument]
User1(id="1", name="Alice")
```

Unannotated fields also work with modern decorators. A field can declare a union type and a default.

```py
from attrs import define, field

@define
class User2:
    name = field()
    id = attr.ib(type=int | None, default=None)

# TODO: Include the unannotated fields in the constructor signature.
reveal_type(User2.__init__)  # revealed: (self: User2, /) -> None

# TODO: No error here.
# error: [unknown-argument]
# error: [unknown-argument]
user = User2(name="Alice", id=12)
# TODO: This should reveal `int | None`.
reveal_type(user.id)  # revealed: Any
```
