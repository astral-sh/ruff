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
# revealed: (self: Member, name: str, role: str = Unknown, tag: str | None = Unknown) -> None
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

# TODO: this should be `Literal[1]`. This is currently blocked on enum support, because
# the `dataclasses.field` overloads make use of a `_MISSING_TYPE` enum, for which we
# infer a @Todo type, and therefore pick the wrong overload.
reveal_type(field(default=1))  # revealed: Unknown
```
