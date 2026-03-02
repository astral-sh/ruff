# SQLModel

```toml
[environment]
python-version = "3.10"
python-platform = "linux"

[project]
dependencies = ["sqlmodel==0.0.27"]
```

## Basic model

`SQLModel.__new__` returns `Any`, so the spec-compliant behavior (shared by ty and pyrefly as of
2026.03.01) is that the returned type from constructing any instance of a `SQLModel` model is `Any`.

Mypy generally considers a `__new__` returning `Any` to still be "instance-returning". This is a
reasonable behavior (it's somewhat weird to specify different behavior for `-> Any` than for no
annotation), but it's not what is specified.

Pyright follows the spec regarding `Any` returned from `__new__`, but it also models
dataclass-transform as explicitly overriding `__new__`. This doesn't match runtime behavior, but
means that it infers precise types for `SQLModel` models.

The simplest fix here would be for SQLModel to remove the `-> Any` annotation from `__new__`, which
would give all type checkers the same inference.

```py
from sqlmodel import SQLModel

class User(SQLModel):
    id: int
    name: str

user = User(id=1, name="John Doe")

# TODO: Users probably want these to be `int` and `str` instead.
reveal_type(user.id)  # revealed: Any
reveal_type(user.name)  # revealed: Any

# This is the expected type for `__init__`, so we are synthesizing it correctly.
reveal_type(User.__init__)  # revealed: (self: User, *, id: int, name: str) -> None

# ... but `__init__` is not checked since `__new__` returns `Any`.
# TODO: users probably want an error here.
User()
```
