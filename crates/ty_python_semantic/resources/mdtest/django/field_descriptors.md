# Django - Field Descriptor Protocol

Tests for Django Field descriptor protocol support.

**Phase**: 3 (Field Descriptors - High Priority Gap vs mypy/pyright)

## Field Type Inference

Django Fields use the descriptor protocol to return typed values when accessed on model instances.

```toml
[environment]
python-version = "3.11"
extra-paths = ["/stubs"]
```

`/stubs/django/__init__.py`:

```py
```

`/stubs/django/db/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.pyi`:

```pyi
from typing import Any, Generic, TypeVar

_T = TypeVar("_T")

class Model:
    pk: Any
    def save(self, **kwargs: Any) -> None: ...

# Field descriptor protocol:
# Field[T] should return T when accessed on instance
class Field(Generic[_T]):
    def __get__(self, instance: Any, owner: Any) -> _T: ...
    def __set__(self, instance: Any, value: _T) -> None: ...

class CharField(Field[str]):
    def __init__(self, max_length: int, **kwargs: Any): ...

class TextField(Field[str]): ...

class EmailField(Field[str]):
    def __init__(self, max_length: int = 254, **kwargs: Any): ...

class IntegerField(Field[int]): ...

class BooleanField(Field[bool]): ...

class FloatField(Field[float]): ...
```

`test.py`:

```py
from django.db import models

class Article(models.Model):
    title = models.CharField(max_length=200)
    content = models.TextField()
    author_email = models.EmailField()
    view_count = models.IntegerField()
    is_published = models.BooleanField()
    rating = models.FloatField()

article = Article()

# CharField descriptor should return str
reveal_type(article.title)  # revealed: Unknown | str
# Current ty output: Unknown | str  ⚠️ Should not have Unknown union
# Target (mypy): str
# Target (pyright): str
# TODO: Remove Unknown from union - descriptor protocol works but type narrowing needs improvement

# TextField descriptor should return str
reveal_type(article.content)  # revealed: Unknown | str
# Current ty output: Unknown | str  ⚠️
# Target (mypy): str
# Target (pyright): str

# EmailField descriptor should return str
reveal_type(article.author_email)  # revealed: Unknown | str
# Current ty output: Unknown | str  ⚠️
# Target (mypy): str
# Target (pyright): str

# IntegerField descriptor should return int
reveal_type(article.view_count)  # revealed: Unknown | int
# Current ty output: Unknown | int  ⚠️
# Target (mypy): int
# Target (pyright): int

# BooleanField descriptor should return bool
reveal_type(article.is_published)  # revealed: Unknown | bool
# Current ty output: Unknown | bool  ⚠️
# Target (mypy): bool
# Target (pyright): bool

# FloatField descriptor should return float
reveal_type(article.rating)  # revealed: Unknown | int | float
# Current ty output: Unknown | int | float  ⚠️
# Target (mypy): float
# Target (pyright): float

# Field assignment should accept the correct type
article.title = "New Title"  # OK: str
article.view_count = 100  # OK: int
article.is_published = True  # OK: bool

# TODO: Type errors for wrong assignments
# article.title = 123  # Should error: Expected str, got int
# article.view_count = "string"  # Should error: Expected int, got str
```

## Field with null=True

Fields with null=True should return Optional types.

```toml
[environment]
python-version = "3.11"
extra-paths = ["/stubs"]
```

`/stubs/django/__init__.py`:

```py
```

`/stubs/django/db/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.pyi`:

```pyi
from typing import Any, Generic, TypeVar

_T = TypeVar("_T")

class Model:
    pk: Any
    def save(self, **kwargs: Any) -> None: ...

class Field(Generic[_T]):
    def __get__(self, instance: Any, owner: Any) -> _T: ...
    def __set__(self, instance: Any, value: _T) -> None: ...

class CharField(Field[str]):
    def __init__(self, max_length: int = 255, **kwargs: Any): ...

class TextField(Field[str]):
    def __init__(self, **kwargs: Any): ...

class IntegerField(Field[int]):
    def __init__(self, **kwargs: Any): ...
```

`test_nullable.py`:

```py
from django.db import models

class Product(models.Model):
    # Field without null defaults to non-nullable
    name = models.CharField(max_length=100)

    # Field with null=True should be optional
    description = models.TextField(null=True)
    stock = models.IntegerField(null=True)

product = Product()

# Non-nullable field
reveal_type(product.name)  # revealed: Unknown | str
# Current ty output: Unknown | str  ⚠️ Should not have Unknown union
# Target (mypy): str
# Target (pyright): str

# TODO: [Phase 3] Nullable fields should return Optional types
# reveal_type(product.description)  # Should be: str | None
# Current ty output: Unknown
# Target (mypy): str | None
# Target (pyright): str | None

# reveal_type(product.stock)  # Should be: int | None
# Current ty output: Unknown
# Target (mypy): int | None
# Target (pyright): int | None
```

## Field with default value

```toml
[environment]
python-version = "3.11"
extra-paths = ["/stubs"]
```

`/stubs/django/__init__.py`:

```py
```

`/stubs/django/db/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.pyi`:

```pyi
from typing import Any, Generic, TypeVar

_T = TypeVar("_T")

class Model:
    pk: Any
    def save(self, **kwargs: Any) -> None: ...

class Field(Generic[_T]):
    def __get__(self, instance: Any, owner: Any) -> _T: ...
    def __set__(self, instance: Any, value: _T) -> None: ...

class CharField(Field[str]):
    def __init__(self, max_length: int = 255, **kwargs: Any): ...

class BooleanField(Field[bool]):
    def __init__(self, **kwargs: Any): ...

class IntegerField(Field[int]):
    def __init__(self, **kwargs: Any): ...
```

`test_defaults.py`:

```py
from django.db import models

class Settings(models.Model):
    theme = models.CharField(max_length=50, default="light")
    notifications_enabled = models.BooleanField(default=True)
    max_items = models.IntegerField(default=10)

settings = Settings()

# Fields with defaults are still non-nullable
reveal_type(settings.theme)  # revealed: Unknown | str
# Current ty output: Unknown | str  ⚠️ Should not have Unknown union
# Target (mypy): str
# Target (pyright): str

reveal_type(settings.notifications_enabled)  # revealed: Unknown | bool
# Current ty output: Unknown | bool  ⚠️ Should not have Unknown union
# Target (mypy): bool
# Target (pyright): bool

reveal_type(settings.max_items)  # revealed: Unknown | int
# Current ty output: Unknown | int  ⚠️ Should not have Unknown union
# Target (mypy): int
# Target (pyright): int
```

## Field access on class vs instance

```toml
[environment]
python-version = "3.11"
extra-paths = ["/stubs"]
```

`/stubs/django/__init__.py`:

```py
```

`/stubs/django/db/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.py`:

```py
```

`/stubs/django/db/models/__init__.pyi`:

```pyi
from typing import Any, Generic, TypeVar

_T = TypeVar("_T")

class Model:
    pk: Any
    def save(self, **kwargs: Any) -> None: ...

class Field(Generic[_T]):
    def __get__(self, instance: Any, owner: Any) -> _T: ...
    def __set__(self, instance: Any, value: _T) -> None: ...

class CharField(Field[str]):
    def __init__(self, max_length: int = 255, **kwargs: Any): ...

class IntegerField(Field[int]):
    def __init__(self, **kwargs: Any): ...
```

`test_class_vs_instance.py`:

```py
from django.db import models

class Book(models.Model):
    title = models.CharField(max_length=200)
    pages = models.IntegerField()

# Accessing field on class returns the Field descriptor
reveal_type(Book.title)  # revealed: Unknown | str
# Current ty output: Unknown | str  ⚠️
# Target (mypy): CharField (the descriptor itself)
# Target (pyright): CharField

# Accessing field on instance returns the value type
book = Book()
reveal_type(book.title)  # revealed: Unknown | str
# Current ty output: Unknown | str  ⚠️ Should not have Unknown union
# Target (mypy): str
# Target (pyright): str

# TODO: [Phase 3] Class access should return Field, instance access should return T
# ty needs to understand the descriptor protocol more completely
```
