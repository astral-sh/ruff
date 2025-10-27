# SQLAlchemy

```toml
[environment]
python-version = "3.13"

[project]
dependencies = ["SQLAlchemy==2.0.44"]
```

## Basic model

```py
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user"

    id: Mapped[int] = mapped_column(primary_key=True, init=False)
    internal_name: Mapped[str] = mapped_column(alias="name")

# TODO: SQLAlchemy overrides `__init__` to accepted all combinations of keyword arguments
reveal_type(User.__init__)  # revealed: def __init__(self, **kw: Any) -> Unknown

user = User(name="John Doe")
reveal_type(user.id)  # revealed: int
reveal_type(user.internal_name)  # revealed: str
```
