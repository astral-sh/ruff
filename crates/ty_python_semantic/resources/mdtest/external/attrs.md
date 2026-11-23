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
from attrs import define, field

@define
class Person:
    name: str = "Unnamed"

    # TODO: This should not be an error once we recognize that `id` has a default
    # error: [invalid-dataclass-field-order]
    id: int = field()

    @id.default
    def _default_id(self) -> int:
        return 0

# TODO: No error here
# error: [missing-argument] "No argument provided for required parameter `id`"
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
