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

## Validator and serializer decorators with explicit `@classmethod`

Pydantic [recommends](https://docs.pydantic.dev/latest/concepts/validators/#class-validators) using
an explicit `@classmethod` decorator below `@field_validator` / `@model_validator(mode="before")` /
`@field_serializer` to get proper type checking. The first parameter should be inferred as
`type[Self]`. ty does not support recognizing these functions as *implicit* class methods, so the
`@classmethod` decorator is required for correct type inference.

```py
from pydantic import BaseModel, field_validator, model_validator, field_serializer

class User(BaseModel):
    name: str

    @field_validator("name")
    @classmethod
    def validate_name(cls, v: str) -> str:
        reveal_type(cls)  # revealed: type[Self@validate_name]
        return v.strip()

    @model_validator(mode="before")
    @classmethod
    def validate_model_before(cls, values: dict) -> dict:
        reveal_type(cls)  # revealed: type[Self@validate_model_before]
        return values

    @field_serializer("name")
    @classmethod
    def serialize_name(cls, v: str) -> str:
        reveal_type(cls)  # revealed: type[Self@serialize_name]
        return v.upper()

    # No @classmethod for "after" validators: the first parameter should be inferred as "Self"
    @model_validator(mode="after")
    def validate_model_after(self) -> "User":
        reveal_type(self)  # revealed: Self@validate_model_after
        return self
```
