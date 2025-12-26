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

## Usage of `field` parameters

```py
from attrs import define, field

@define
class Product:
    id: int = field(init=False)
    name: str = field()
    price_cent: int = field(kw_only=True)

reveal_type(Product.__init__)  # revealed: (self: Product, name: str, *, price_cent: int) -> None
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
