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

## What is it that we do not support yet?

Basic setup:

```py
from datetime import datetime

from sqlalchemy import select, Integer, Text, Boolean, DateTime
from sqlalchemy.orm import Session
from sqlalchemy.orm import DeclarativeBase
from sqlalchemy.orm import Mapped, mapped_column
from sqlalchemy import create_engine

engine = create_engine("sqlite://example.db")
session = Session(engine)

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "users"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    name: Mapped[str] = mapped_column(Text)
    is_admin: Mapped[bool] = mapped_column(Boolean, default=False)
```

Why do we see `Unknown`s for `select(User.id, User.name)` here?

```py
stmt = select(User.id, User.name)
# TODO: should be `Select[tuple[int, str]]`
reveal_type(stmt)  # revealed: Select[tuple[Unknown, Unknown]]
```

The types of the arguments seem correct:

```py
reveal_type(User.id)  # revealed: InstrumentedAttribute[int]
reveal_type(User.name)  # revealed: InstrumentedAttribute[str]
```

The two-parameter overload of `select` has a type of

`def select(__ent0: _TCCA[_T0], __ent1: _TCCA[_T1], /) -> Select[_T0, _T1]: ...`

here `_TCCA` is an alias for `_TypedColumnClauseArgument`:

```py
from sqlalchemy.sql._typing import _TypedColumnClauseArgument

# revealed: <types.UnionType special form 'TypedColumnsClauseRole[_T@_TypedColumnClauseArgument] | SQLCoreOperations[_T@_TypedColumnClauseArgument] | type[_T@_TypedColumnClauseArgument]'>
reveal_type(_TypedColumnClauseArgument)
```

If we use that generic type alias in a type expression, we can properly specialize it:

```py
def _(
    col: _TypedColumnClauseArgument[int],
) -> None:
    reveal_type(col)  # revealed: TypedColumnsClauseRole[int] | SQLCoreOperations[int] | type[int]
```

Next, verify that we can assign `User.id` to a fully specialized version of
`_TypedColumnClauseArgument`:

```py
user_id_as_tcca: _TypedColumnClauseArgument[int] = User.id
```

If we use the generic version of `_TypedColumnClauseArgument` without specialization, we get
`Unknown`:

```py
def extract_t_from_tcca[T](col: _TypedColumnClauseArgument[T]) -> T:
    raise NotImplementedError

reveal_type(extract_t_from_tcca(User.id))  # revealed: Unknown
```

However, if we use just the relevant union element of `_TypedColumnClauseArgument`
(`SQLCoreOperations`), it works as expected:

```py
from sqlalchemy.sql.elements import SQLCoreOperations

def extract_t_from_sco[T](col: SQLCoreOperations[T]) -> T:
    raise NotImplementedError

reveal_type(extract_t_from_sco(User.id))  # revealed: int
reveal_type(extract_t_from_sco(User.name))  # revealed: str
```

I reported this as <https://github.com/astral-sh/ty/issues/1772>.

Now let's assume we would be able to solve for `T` here. This would mean we would get a type of
`Select[tuple[int, str]]`. Can we use that type and proceed with it? It looks like this works:

```py
from sqlalchemy.sql.selectable import Select

def _(stmt: Select[tuple[int, str]]) -> None:
    for row in session.execute(stmt):
        reveal_type(row)  # revealed: Row[tuple[int, str]]
```

What about the `_tuple` calls? This seems to work:

```py
def _(stmt: Select[tuple[int, str]]) -> None:
    result = session.execute(stmt)

    reveal_type(result)  # revealed: Result[tuple[int, str]]

    user = result.one_or_none()
    reveal_type(user)  # revealed: Row[tuple[int, str]] | None

    if not user:
        return

    reveal_type(user)  # revealed: Row[tuple[int, str]] & ~AlwaysFalsy
    reveal_type(user._tuple())  # revealed: tuple[int, str]
```

What about `.tuples()`? That seems to work as well:

```py
def _(stmt: Select[tuple[int, str]]) -> None:
    for user_id, name in session.execute(stmt).tuples():
        reveal_type(user_id)  # revealed: int
        reveal_type(name)  # revealed: str
```

What about the `.scalar` calls? Those seem to work too:

```py
def _(stmt: Select[tuple[int]]) -> None:
    user_id = session.scalar(stmt)
    reveal_type(user_id)  # revealed: int | None

    reveal_type(session.scalars(stmt).first())  # revealed: int | None
```
