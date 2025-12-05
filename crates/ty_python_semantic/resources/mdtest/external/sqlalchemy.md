# SQLAlchemy

```toml
[environment]
python-version = "3.13"

[project]
dependencies = ["SQLAlchemy==2.0.44"]
```

## Basic model

Here, we mostly make sure that ty understands SQLAlchemy's dataclass-transformer setup:

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

## Queries

First, the basic setup:

```py
from datetime import datetime

from sqlalchemy import select, Integer, Text, Boolean, DateTime
from sqlalchemy.orm import Session
from sqlalchemy.orm import DeclarativeBase
from sqlalchemy.orm import Mapped, mapped_column
from sqlalchemy import create_engine

engine = create_engine("sqlite://example.db")
session = Session(engine)
```

Now we can declare a simple model:

```py
class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "users"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    name: Mapped[str] = mapped_column(Text)
    is_admin: Mapped[bool] = mapped_column(Boolean, default=False)
```

And perform simple queries:

```py
stmt = select(User)
reveal_type(stmt)  # revealed: Select[tuple[User]]

users = session.scalars(stmt).all()
reveal_type(users)  # revealed: Sequence[User]

for row in session.execute(stmt):
    reveal_type(row)  # revealed: Row[tuple[User]]

stmt = select(User).where(User.name == "Alice")
alice = session.scalars(stmt).first()
reveal_type(alice)  # revealed: User | None

stmt = select(User).where(User.is_admin == True).order_by(User.name).limit(10)
admin_users = session.scalars(stmt).all()
reveal_type(admin_users)  # revealed: Sequence[User]
```

This also works with the legacy `query` API:

```py
users_legacy = session.query(User).all()
reveal_type(users_legacy)  # revealed: list[User]
```

We can also specify particular columns to select:

```py
stmt = select(User.id, User.name)
# TODO: should be `Select[tuple[int, str]]`
reveal_type(stmt)  # revealed: Select[tuple[Unknown, Unknown]]

for row in session.execute(stmt):
    # TODO: should be `Row[Tuple[int, str]]`
    reveal_type(row)  # revealed: Row[tuple[Unknown, Unknown]]
```

And similarly with the legacy `query` API:

```py
query = session.query(User.id, User.name)
# TODO: should be `RowReturningQuery[tuple[int, str]]`
reveal_type(query)  # revealed: RowReturningQuery[tuple[Unknown, Unknown]]

for row in query.all():
    # TODO: should be `Row[Tuple[int, str]]`
    reveal_type(row)  # revealed: Row[tuple[Unknown, Unknown]]
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

reveal_type(
    _TypedColumnClauseArgument
)  # revealed: <types.UnionType special form 'TypedColumnsClauseRole[_T@_TypedColumnClauseArgument] | SQLCoreOperations[_T@_TypedColumnClauseArgument] | type[_T@_TypedColumnClauseArgument]'>
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
