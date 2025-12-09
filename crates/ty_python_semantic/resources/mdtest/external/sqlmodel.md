# SQLModel

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = [
    "sqlmodel==0.0.27",
    # TODO: remove this pin, once we have a lockfile
    "sqlalchemy==2.0.44"
]
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

# TODO: this should not mention `__pydantic_self__`, and have proper parameters defined by the fields
reveal_type(User.__init__)  # revealed: def __init__(__pydantic_self__, **data: Any) -> None

# TODO: this should be an error
User()
```
