# Pydantic

```toml
[environment]
python-version = "3.12"

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

invalid_user = User(id=2)  # error: [missing-argument] "No argument provided for required parameter `name`"
```

## Usage of `Field`

```py
from pydantic import BaseModel, Field

class Product(BaseModel):
    id: int = Field(init=False)
    name: str = Field(..., kw_only=False, min_length=1)
    internal_price_cent: int = Field(..., gt=0, alias="price_cent")

reveal_type(Product.__init__)  # revealed: (self: Product, name: str = Any, *, price_cent: int = Any) -> None

product = Product("Laptop", price_cent=999_00)

reveal_type(product.id)  # revealed: int
reveal_type(product.name)  # revealed: str
reveal_type(product.internal_price_cent)  # revealed: int
```

## Regression 1159

```py
from pydantic import BaseModel, Field

def secret_from_env(env_var: str, default: str | None = None) -> bytes | None:
    raise NotImplementedError

class BaseChatOpenAI(BaseModel):
    model_name: str = Field(default="gpt-3.5-turbo", alias="model")
    openai_api_key: bytes | None = Field(alias="api_key", default_factory=secret_from_env("OPENAI_API_KEY", default=None))

# TODO: no error here
# error: [unknown-argument] "Argument `model` does not match any known parameter"
BaseChatOpenAI(model="gpt-4", api_key=b"my_secret_key")
```
