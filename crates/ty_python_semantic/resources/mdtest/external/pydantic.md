# Pydantic

```toml
[environment]
python-version = "3.12"
python-platform = "linux"

[project]
dependencies = ["pydantic==2.13.4", "pydantic-settings==2.14.2"]
```

## Basic model

A basic Pydantic model looks and acts similar to a dataclass:

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

`Field` is a field-specifier function. In the following example, `tags` has a default value, and
`internal_price_cent` can be set through its alias `price_cent`:

```py
from pydantic import BaseModel, Field

class Product(BaseModel):
    name: str = Field(min_length=1)
    tags: list[str] = Field(default_factory=list)
    internal_price_cent: int = Field(gt=0, alias="price_cent")

reveal_type(Product.__init__)  # revealed: (self: Product, *, name: str, tags: list[str] = ..., price_cent: int) -> None

product = Product(name="Laptop", price_cent=999_00)
```

The fields have the expected types:

```py
reveal_type(product.name)  # revealed: str
reveal_type(product.tags)  # revealed: list[str]
reveal_type(product.internal_price_cent)  # revealed: int
```

Omitting the `name` or the `price_cent` is not allowed:

```py
# error: [missing-argument] "No argument provided for required parameter `name`"
Product(price_cent=100_00)
# error: [missing-argument] "No argument provided for required parameter `price_cent`"
Product(name="Phone")
```

Using the internal field name is not possible (the argument will be accepted, but `price_cent` is
missing):

```py
# TODO: This should ideally only report `missing-argument`, not `unknown-argument` (since extra fields are allowed by default)
# error: [missing-argument]
# error: [unknown-argument]
Product(name="Laptop", internal_price_cent=999_00)
```

Conversely, accessing a field through the alias is also not allowed:

```py
product.price_cent  # error: [unresolved-attribute]
```

## Usage of ellipsis in `Field`

A positional argument of `...` to the `Field` function indicates that the field *has no default and
is required*:

```py
from pydantic import BaseModel, Field

class Person(BaseModel):
    name: str = Field(..., max_length=255)

Person(name="Alice")
# TODO: this should be an error
Person()
```

## Strict and lax mode

Pydantic distinguishes a "strict" mode in which it will error if a value is of the wrong type, and a
"lax" mode, in which it attempts to coerce the value to the correct type. We model these two modes
in ty so that static analysis supports the runtime validation behavior when possible.

### Using the model config to enable strict mode

Strict mode can be activated for a whole model:

```py
from pydantic import BaseModel, ConfigDict

class Person(BaseModel):
    model_config = ConfigDict(strict=True)

    name: str
    age: int

Person(name="Alice", age=20)  # okay
Person(name="Alice", age="20")  # error: [invalid-argument-type]
```

### Lax mode is the default

When no configuration is given, or when `strict=False`, lax mode is used:

```py
from pydantic import BaseModel, ConfigDict

class Person1(BaseModel):
    name: str
    age: int

Person1(name="Alice", age=20)  # okay
# TODO: no error here
# error: [invalid-argument-type]
Person1(name="Alice", age="20")  # okay, coerced
# error: [invalid-argument-type]
Person1(name="Alice", age=None)  # error, cannot be coerced

class Person2(BaseModel):
    model_config = ConfigDict(strict=False)

    name: str
    age: int

Person2(name="Alice", age=20)  # okay
# TODO: no error here
# error: [invalid-argument-type]
Person2(name="Alice", age="20")  # okay
# error: [invalid-argument-type]
Person2(name="Alice", age=None)  # error, cannot be coerced
```

### Changing a specific field

Strict mode can also be activated for a specific field only:

```py
from pydantic import BaseModel, ConfigDict, Field

class Person1(BaseModel):
    name: str
    age: int = Field(strict=True)
```

Here, validation is lax for the `name` field (`bytes` is converted to `str`):

```py
Person1(name="Alice", age=20)
# TODO: no error here
# error: [invalid-argument-type]
Person1(name=b"Alice", age=20)
```

But `age` is validated in `strict` mode, so the conversion from `str` to `int` is not allowed here:

```py
Person1(name="Alice", age=20)
Person1(name="Alice", age="20")  # error: [invalid-argument-type]
```

The opposite is also possible. A whole model can be in "strict" mode, and a single field can opt
out:

```py
class Person2(BaseModel):
    model_config = ConfigDict(strict=True)

    name: str = Field(strict=False)
    age: int

Person2(name="Alice", age=20)
# TODO: no error here
# error: [invalid-argument-type]
Person2(name=b"Alice", age=20)

Person2(name="Alice", age=20)
Person2(name="Alice", age="20")  # error: [invalid-argument-type]
```

## `validate_by_name`, `validate_by_alias`

By default, Pydantic only allows a field to be initialized by its alias name, not by its field name:

```py
from pydantic import BaseModel, ConfigDict, Field

class DefaultOnlyAlias(BaseModel):
    name: int = Field(alias="alias")

DefaultOnlyAlias(alias=1)
# TODO: This should ideally only report `missing-argument`, not `unknown-argument` (since extra fields are allowed by default)
# error: [missing-argument]
# error: [unknown-argument]
DefaultOnlyAlias(name=1)
```

When `validate_by_name=True`, a field can also be initialized using its internal name:

```py
class AliasAndName(BaseModel):
    model_config = ConfigDict(validate_by_name=True)

    name: int = Field(alias="alias")

AliasAndName(alias=1)
# TODO: no errors here
# error: [unknown-argument]
# error: [missing-argument]
AliasAndName(name=1)
```

Passing none of these should be an error:

```py
# Note: this might be hard to support once we implement the feature above?
# error: [missing-argument]
AliasAndName()
```

Conversely, when `validate_by_alias=False`, validation by alias can be disallowed:

```py
class OnlyName(BaseModel):
    model_config = ConfigDict(validate_by_name=True, validate_by_alias=False)

    name: int = Field(alias="alias")

# TODO: this should be an error
OnlyName(alias=1)
# TODO: no errors here
# error: [unknown-argument]
# error: [missing-argument]
OnlyName(name=1)
```

## Extra fields

By default, Pydantic allows arbitrary extra data which is simply ignored:

```py
from pydantic import BaseModel, ConfigDict

class Person(BaseModel):
    name: str

# TODO: no error here
# error: [unknown-argument]
Person(name="Alice", something_else=7)
```

By setting `extra="forbid"`, this can be disallowed:

```py
class PersonWithoutExtras(BaseModel):
    model_config = ConfigDict(extra="forbid")

    name: str

PersonWithoutExtras(name="Alice", something_else=7)  # error: [unknown-argument]
```

## Frozen models and fields

There are various ways to make a field immutable. A model can be globally frozen using a class
parameter:

```py
from pydantic import BaseModel, ConfigDict, Field

class PersonFrozenName1(BaseModel, frozen=True):
    name: str

person = PersonFrozenName1(name="Alice")
person.name = "Bob"  # error: [invalid-assignment]
```

It can also be globally frozen using the model config:

```py
class PersonFrozenName2(BaseModel):
    model_config = ConfigDict(frozen=True)

    name: str

person = PersonFrozenName2(name="Alice")
# TODO: This should be an error
person.name = "Bob"
```

Finally, individual fields can also be made immutable on a non-frozen model:

```py
class PersonFrozenName3(BaseModel):
    name: str = Field(frozen=True)
    age: int

person = PersonFrozenName3(name="Alice", age=20)
# TODO: this should be an error
person.name = "Bob"
person.age += 1
```

## Validation of default values

At runtime, default values are *not* validated against the field type annotation, unless
`validate_default=True` is set. In static analysis, we still need to verify the default values
against the type annotation. Not doing so would be unsound. We do this unconditionally, even if
`validate_default=False` (which is also the default):

```py
from pydantic import BaseModel, ConfigDict, Field

class Person1(BaseModel):
    # error: [invalid-assignment]
    name: str = Field(default=None)

class Person2(BaseModel):
    model_config = ConfigDict(validate_default=False)

    # error: [invalid-assignment]
    name: str = Field(default=None)

class Person3(BaseModel):
    model_config = ConfigDict(validate_default=True)

    # error: [invalid-assignment]
    name: str = Field(default=None)

class Person4(BaseModel):
    # error: [invalid-assignment]
    name: str = Field(default=None, validate_default=False)

class Person5(BaseModel):
    # TODO: this should be an error
    name: str = Field(default=None, validate_default=True)
```

## Verification of models

A field without a type annotation leads to a runtime error.

```py
from pydantic import BaseModel, Field

# TODO: this should ideally be an error
class PersonUntypedField(BaseModel):
    name: str
    age = Field(default=0)
```

## BaseSettings

A model derived from `BaseSettings` can use environment variables, so we assume that it is okay not
to provide their values:

```py
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    host: str
    port: int

# Would succeed at runtime if HOST and PORT environment variables are set
# TODO: no error here
# error: [missing-argument]
Settings()
```

## Pydantic dataclasses

Pydantic's dataclasses are similar to the standard library dataclasses:

```py
from pydantic import Field
from pydantic.dataclasses import dataclass

@dataclass
class Person:
    name: str
    id: int = Field(default=0, init=False)
    age: int = Field(default=0)

# `id` is absent in the constructor signature:
reveal_type(Person.__init__)  # revealed: (self: Person, name: str, age: int = 0) -> None

Person(name="Alice")
Person(name="Alice", age=20)
```

## Inherited `ModelMetaclass`

Pydantic's metaclass-based `@dataclass_transform` metadata should continue to apply when a custom
metaclass inherits from `ModelMetaclass`.

```py
from pydantic import BaseModel
from pydantic._internal._model_construction import ModelMetaclass

class RegistryMeta(ModelMetaclass): ...

class User(BaseModel, metaclass=RegistryMeta):
    name: str
    age: int = 0

reveal_type(User.__init__)  # revealed: (self: User, *, name: str, age: int = 0) -> None

User(name="alice")
User(name="alice", age=1)

# error: [missing-argument]
User()
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
    def validate_model_before(cls, values: dict[str, object]) -> dict[str, object]:
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
