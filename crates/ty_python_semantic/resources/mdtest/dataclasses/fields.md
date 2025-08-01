# Dataclass fields

## Basic

```py
from dataclasses import dataclass, field

@dataclass
class Member:
    name: str
    role: str = field(default="user")
    tag: str | None = field(default=None, init=False)

# revealed: (self: Member, name: str, role: str = Literal["user"]) -> None
reveal_type(Member.__init__)

alice = Member(name="Alice", role="admin")
reveal_type(alice.role)  # revealed: str
alice.role = "moderator"

# `tag` is marked as `init=False`, so this is an
# error: [unknown-argument] "Argument `tag` does not match any known parameter"
bob = Member(name="Bob", tag="VIP")
```

## `default_factory`

The `default_factory` argument can be used to specify a callable that provides a default value for a
field:

```py
from dataclasses import dataclass, field
from datetime import datetime

@dataclass
class Data:
    content: list[int] = field(default_factory=list)
    timestamp: datetime = field(default_factory=datetime.now, init=False)

# revealed: (self: Data, content: list[int] = list[Unknown]) -> None
reveal_type(Data.__init__)

data = Data([1, 2, 3])
reveal_type(data.content)  # revealed: list[int]
reveal_type(data.timestamp)  # revealed: datetime
```

## `kw_only`

```toml
[environment]
python-version = "3.12"
```

If `kw_only` is set to `True`, the field can only be set using keyword arguments:

```py
from dataclasses import dataclass, field

@dataclass
class Person:
    name: str
    age: int | None = field(default=None, kw_only=True)
    role: str = field(default="user", kw_only=True)

# TODO: the `age` and `role` fields should be keyword-only
# revealed: (self: Person, name: str, age: int | None = None, role: str = Literal["user"]) -> None
reveal_type(Person.__init__)

alice = Person(role="admin", name="Alice")

# TODO: this should be an error
bob = Person("Bob", 30)
```

## The `field` function

```py
from dataclasses import field

def get_default() -> str:
    return "default"

reveal_type(field(default=1))  # revealed: dataclasses.Field[Literal[1]]
reveal_type(field(default=None))  # revealed: dataclasses.Field[None]
reveal_type(field(default_factory=get_default))  # revealed: dataclasses.Field[str]
```
