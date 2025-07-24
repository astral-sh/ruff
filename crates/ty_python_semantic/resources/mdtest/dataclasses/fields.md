# Dataclass fields

## Basic

```py
from dataclasses import dataclass, field

@dataclass
class Member:
    name: str
    role: str = field(default="user")
    tag: str | None = field(default=None, init=False)

# TODO: this should not include the `tag` parameter, since it has `init=False` set
# revealed: (self: Member, name: str, role: str = Literal["user"], tag: str | None = None) -> None
reveal_type(Member.__init__)

alice = Member(name="Alice", role="admin")
reveal_type(alice.role)  # revealed: str
alice.role = "moderator"

# TODO: this should be an error, `tag` has `init=False`
bob = Member(name="Bob", tag="VIP")
```

## The `field` function

```py
from dataclasses import field

reveal_type(field(default=1))  # revealed: Literal[1]
```
