# SQLModel

```toml
[environment]
python-version = "3.10"
python-platform = "linux"

[project]
dependencies = ["sqlmodel==0.0.27"]
```

## Basic model

```py
from sqlmodel import SQLModel

class User(SQLModel):
    id: int
    name: str

user = User(id=1, name="John Doe")
# TODO: these should be `int` and `str` once we add pydantic model synthesis.
# Currently `Any` because `SQLModel.__new__` is annotated as `-> Any`, and the spec says
# "an explicit return type of `Any` should be treated as a type that is not an instance of
# the class being constructed."
reveal_type(user.id)  # revealed: Any
reveal_type(user.name)  # revealed: Any

reveal_type(User.__init__)  # revealed: (self: User, *, id: int, name: str) -> None

# No `missing-argument` error here: `SQLModel.__new__` returns `Any`, so per the spec
# `__init__` is not evaluated and any arguments are accepted via `__new__`.
User()
```
