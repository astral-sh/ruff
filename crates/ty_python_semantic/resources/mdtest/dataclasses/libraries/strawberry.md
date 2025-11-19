# SQLAlchemy

```toml
[environment]
python-version = "3.13"

[project]
dependencies = ["strawberry-graphql==0.283.3"]
```

## Basic model

```py
import strawberry

@strawberry.type
class User:
    id: int
    role: str = strawberry.field(default="user")

reveal_type(User.__init__)  # revealed: (self: User, *, id: int, role: str = Any) -> None

user = User(id=1)
reveal_type(user.id)  # revealed: int
reveal_type(user.role)  # revealed: str
```
