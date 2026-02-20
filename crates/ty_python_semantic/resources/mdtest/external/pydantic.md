# Pydantic

```toml
[environment]
python-version = "3.12"
python-platform = "linux"

[project]
dependencies = ["pydantic==2.12.2"]
```

## Basic model

```py
from pydantic import BaseModel

class User(BaseModel):
    id: int
    name: str

reveal_type(User.__init__)  # revealed: (self: User, *, id: int, name: str) -> None

user = User(id=1, name="John Doe")
reveal_type(user.id)  # revealed: int
reveal_type(user.name)  # revealed: str

# error: [missing-argument] "No argument provided for required parameter `name`"
invalid_user = User(id=2)
```

## Usage of `Field`

```py
from pydantic import BaseModel, Field

class Product(BaseModel):
    id: int = Field(init=False)
    name: str = Field(..., kw_only=False, min_length=1)
    internal_price_cent: int = Field(..., gt=0, alias="price_cent")

reveal_type(Product.__init__)  # revealed: (self: Product, name: str = ..., *, price_cent: int = ...) -> None

product = Product("Laptop", price_cent=999_00)

reveal_type(product.id)  # revealed: int
reveal_type(product.name)  # revealed: str
reveal_type(product.internal_price_cent)  # revealed: int
```

## Validator decorators

Pydantic validator decorators are implicitly classmethods. The first parameter should be typed as
`type[Self]`, not `Self`.

```py
from pydantic import BaseModel, field_validator, model_validator, field_serializer

class User(BaseModel):
    name: str

    @field_validator("name")
    def validate_name(cls, v: str) -> str:
        reveal_type(cls)  # revealed: type[Self@validate_name]
        return v.strip()

    @model_validator(mode="before")
    def validate_model(cls, values: dict) -> dict:
        reveal_type(cls)  # revealed: type[Self@validate_model]
        return values

    @field_serializer("name")
    def serialize_name(cls, v: str) -> str:
        reveal_type(cls)  # revealed: type[Self@serialize_name]
        return v.upper()
```

## `model_validator` with `mode="after"`

`@model_validator(mode="after")` receives the model *instance*, not the class, so it is not treated
as an implicit classmethod.

```py
from pydantic import BaseModel, model_validator

class Order(BaseModel):
    total: float

    @model_validator(mode="after")
    def validate_order(self) -> "Order":
        reveal_type(self)  # revealed: Self@validate_order
        return self
```

## Validator decorators imported from submodule

```py
from pydantic import BaseModel
from pydantic.functional_validators import field_validator

class Item(BaseModel):
    name: str

    @field_validator("name")
    def validate_name(cls, v: str) -> str:
        reveal_type(cls)  # revealed: type[Self@validate_name]
        return v
```
