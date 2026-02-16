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
