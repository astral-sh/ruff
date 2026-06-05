# SQLModel

```toml
[environment]
python-version = "3.10"
python-platform = "linux"

[project]
dependencies = ["sqlmodel==0.0.38"]
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

reveal_type(User.__init__)  # revealed: (self: User, *, id: int, name: str) -> None

User()  # error: [missing-argument]
```

## Table model

SQLModel table models (with `table=True`) use SQLAlchemy's ORM under the hood. At runtime,
class-level attribute access on these models returns `InstrumentedAttribute` descriptors, not
the plain annotated types. ty should understand this and allow descriptor methods like `.in_()`.

```py
from sqlmodel import SQLModel, Field, select

class Item(SQLModel, table=True):
    id: int | None = Field(default=None, primary_key=True)
    name: str

# Instance-level access should still return the plain types
item = Item(id=1, name="test")
reveal_type(item.id)  # revealed: int | None
reveal_type(item.name)  # revealed: str

# Class-level access should return InstrumentedAttribute descriptors
reveal_type(Item.id)  # revealed: InstrumentedAttribute[int | None]
reveal_type(Item.name)  # revealed: InstrumentedAttribute[str]

# Descriptor methods should work on class-level attributes
stmt = select(Item).where(Item.name == "test")
stmt2 = select(Item).where(Item.id.in_([1, 2, 3]))
```

## Non-table model regression guard

Non-table SQLModel models are pure Pydantic models and should NOT have their attributes
wrapped as `InstrumentedAttribute`. This test ensures we don't accidentally affect them.

```py
from sqlmodel import SQLModel

class ItemBase(SQLModel):
    id: int
    name: str

reveal_type(ItemBase.id)  # revealed: int
reveal_type(ItemBase.name)  # revealed: str

item = ItemBase(id=1, name="test")
reveal_type(item.id)  # revealed: int
reveal_type(item.name)  # revealed: str
```

## Inheritance from table model

A child class inheriting from a table model should also be detected as a table model.

```py
from sqlmodel import SQLModel, Field

class BaseItem(SQLModel, table=True):
    id: int | None = Field(default=None, primary_key=True)
    name: str

class SpecialItem(BaseItem):
    __tablename__ = "special_items"
    extra: str = ""

reveal_type(SpecialItem.id)  # revealed: InstrumentedAttribute[int | None]
reveal_type(SpecialItem.name)  # revealed: InstrumentedAttribute[str]

special = SpecialItem(id=1, name="test", extra="x")
reveal_type(special.id)  # revealed: int | None
reveal_type(special.name)  # revealed: str
```

## Mixed table and non-table models

A non-table base model should not be affected, even when a table model inherits from it.

```py
from sqlmodel import SQLModel, Field

class ItemBase(SQLModel):
    name: str

class Item(ItemBase, table=True):
    id: int | None = Field(default=None, primary_key=True)

# Non-table base: should NOT have InstrumentedAttribute
reveal_type(ItemBase.name)  # revealed: str

# Table model: should have InstrumentedAttribute
reveal_type(Item.id)  # revealed: InstrumentedAttribute[int | None]
reveal_type(Item.name)  # revealed: InstrumentedAttribute[str]
```
