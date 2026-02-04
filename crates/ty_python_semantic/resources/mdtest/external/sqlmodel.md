# SQLModel

SQLModel uses `@__dataclass_transform__()` on its metaclass for backwards compatibility with
pre-3.11 Python, which ty now recognizes.

```toml
[environment]
python-version = "3.13"
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
reveal_type(user.id)  # revealed: int
reveal_type(user.name)  # revealed: str

reveal_type(User.__init__)  # revealed: (self: User, id: int, name: str) -> None

# error: [missing-argument]
User()
```

## Multi-level inheritance

```py
from sqlmodel import SQLModel

class Base(SQLModel):
    id: int

class User(Base):
    name: str

reveal_type(User.__init__)  # revealed: (self: User, id: int, name: str) -> None

User(id=1, name="Test")

# error: [missing-argument]
User()
```
