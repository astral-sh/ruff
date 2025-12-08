# SQLAlchemy

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["SQLAlchemy==2.0.44"]
```

## ORM Model

This test makes sure that ty understands SQLAlchemy's `dataclass_transform` setup:

```py
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user"

    id: Mapped[int] = mapped_column(primary_key=True, init=False)
    internal_name: Mapped[str] = mapped_column(alias="name")

user = User(name="John Doe")
reveal_type(user.id)  # revealed: int
reveal_type(user.internal_name)  # revealed: str
```

Unfortunately, SQLAlchemy overrides `__init__` and explicitly accepts all combinations of keyword
arguments. This is why we currently cannot flag invalid constructor calls:

```py
reveal_type(User.__init__)  # revealed: def __init__(self, **kw: Any) -> Unknown

# TODO: this should ideally be an error
invalid_user = User(invalid_arg=42)
```

## Basic query example

First, set up a `Session`:

```py
from sqlalchemy import select, Integer, Text, Boolean
from sqlalchemy.orm import Session
from sqlalchemy.orm import DeclarativeBase
from sqlalchemy.orm import Mapped, mapped_column
from sqlalchemy import create_engine

engine = create_engine("sqlite://example.db")
session = Session(engine)
```

And define a simple model:

```py
class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "users"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    name: Mapped[str] = mapped_column(Text)
    is_admin: Mapped[bool] = mapped_column(Boolean, default=False)
```

Finally, we can execute queries:

```py
stmt = select(User)
reveal_type(stmt)  # revealed: Select[tuple[User]]

users = session.scalars(stmt).all()
reveal_type(users)  # revealed: Sequence[User]

for row in session.execute(stmt):
    reveal_type(row)  # revealed: Row[tuple[User]]

stmt = select(User).where(User.name == "Alice")
alice1 = session.scalars(stmt).first()
reveal_type(alice1)  # revealed: User | None

alice2 = session.scalar(stmt)
reveal_type(alice2)  # revealed: User | None

result = session.execute(stmt)
row = result.one_or_none()
assert row is not None
(alice3,) = row._tuple()
reveal_type(alice3)  # revealed: User
```

This also works with more complex queries:

```py
stmt = select(User).where(User.is_admin == True).order_by(User.name).limit(10)
admin_users = session.scalars(stmt).all()
reveal_type(admin_users)  # revealed: Sequence[User]
```

We can also specify particular columns to select:

```py
stmt = select(User.id, User.name)
# TODO: should be `Select[tuple[int, str]]`
reveal_type(stmt)  # revealed: Select[tuple[Unknown, Unknown]]

ids_and_names = session.execute(stmt).all()
# TODO: should be `Sequence[Row[tuple[int, str]]]`
reveal_type(ids_and_names)  # revealed: Sequence[Row[tuple[Unknown, Unknown]]]

for row in session.execute(stmt):
    # TODO: should be `Row[tuple[int, str]]`
    reveal_type(row)  # revealed: Row[tuple[Unknown, Unknown]]

for user_id, name in session.execute(stmt).tuples():
    # TODO: should be `int`
    reveal_type(user_id)  # revealed: Unknown
    # TODO: should be `str`
    reveal_type(name)  # revealed: Unknown

result = session.execute(stmt)
row = result.one_or_none()
assert row is not None
(user_id, name) = row._tuple()
# TODO: should be `int`
reveal_type(user_id)  # revealed: Unknown
# TODO: should be `str`
reveal_type(name)  # revealed: Unknown

stmt = select(User.id).where(User.name == "Alice")

# TODO: should be `Select[tuple[int]]`
reveal_type(stmt)  # revealed: Select[tuple[Unknown]]

alice_id = session.scalars(stmt).first()
# TODO: should be `int | None`
reveal_type(alice_id)  # revealed: Unknown | None

alice_id = session.scalar(stmt)
# TODO: should be `int | None`
reveal_type(alice_id)  # revealed: Unknown | None
```

Using the legacy `query` API also works:

```py
users_legacy = session.query(User).all()
reveal_type(users_legacy)  # revealed: list[User]

query = session.query(User)
reveal_type(query)  # revealed: Query[User]

reveal_type(query.all())  # revealed: list[User]

for row in query:
    reveal_type(row)  # revealed: User
```

And similarly when specifying particular columns:

```py
query = session.query(User.id, User.name)
# TODO: should be `RowReturningQuery[tuple[int, str]]`
reveal_type(query)  # revealed: RowReturningQuery[tuple[Unknown, Unknown]]

# TODO: should be `list[Row[tuple[int, str]]]`
reveal_type(query.all())  # revealed: list[Row[tuple[Unknown, Unknown]]]

for row in query:
    # TODO: should be `Row[tuple[int, str]]`
    reveal_type(row)  # revealed: Row[tuple[Unknown, Unknown]]
```

## Async API

The async API is supported as well:

```py
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, Integer, Text
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "users"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    name: Mapped[str] = mapped_column(Text)

async def test_async(session: AsyncSession):
    stmt = select(User).where(User.name == "Alice")
    alice = await session.scalar(stmt)
    reveal_type(alice)  # revealed: User | None

    stmt = select(User.id, User.name)
    result = await session.execute(stmt)
    for user_id, name in result.tuples():
        # TODO: should be `int`
        reveal_type(user_id)  # revealed: Unknown
        # TODO: should be `str`
        reveal_type(name)  # revealed: Unknown
```
