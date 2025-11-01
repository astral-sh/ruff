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
# 1. Current ty (no changes): Unknown | str
# 2. Future ty (after fix): str
# 3. mypy + django-stubs: str
# 4. pyright + django-types: str
# TODO: Remove Unknown from union - descriptor protocol works but needs type narrowing

# TextField descriptor should return str
reveal_type(article.content)  # revealed: Unknown | str
# 1. Current ty (no changes): Unknown | str
# 2. Future ty (after fix): str
# 3. mypy + django-stubs: str
# 4. pyright + django-types: str

# EmailField descriptor should return str
reveal_type(article.author_email)  # revealed: Unknown | str
# 1. Current ty (no changes): Unknown | str
# 2. Future ty (after fix): str
# 3. mypy + django-stubs: str
# 4. pyright + django-types: str

# IntegerField descriptor should return int
reveal_type(article.view_count)  # revealed: Unknown | int
# 1. Current ty (no changes): Unknown | int
# 2. Future ty (after fix): int
# 3. mypy + django-stubs: int
# 4. pyright + django-types: int

# BooleanField descriptor should return bool
reveal_type(article.is_published)  # revealed: Unknown | bool
# 1. Current ty (no changes): Unknown | bool
# 2. Future ty (after fix): bool
# 3. mypy + django-stubs: bool
# 4. pyright + django-types: bool

# FloatField descriptor should return float
reveal_type(article.rating)  # revealed: Unknown | int | float
# 1. Current ty (no changes): Unknown | int | float
# 2. Future ty (after fix): float
# 3. mypy + django-stubs: float
# 4. pyright + django-types: float

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
# 1. Current ty (no changes): Unknown | str
# 2. Future ty (after fix): str
# 3. mypy + django-stubs: str
# 4. pyright + django-types: str

# TODO: [Phase 3] Nullable fields should return Optional types
# reveal_type(product.description)
# 1. Current ty (no changes): Unknown (field not recognized due to missing null support)
# 2. Future ty (after fix): str | None
# 3. mypy + django-stubs: str | None
# 4. pyright + django-types: str | None

# reveal_type(product.stock)
# 1. Current ty (no changes): Unknown (field not recognized due to missing null support)
# 2. Future ty (after fix): int | None
# 3. mypy + django-stubs: int | None
# 4. pyright + django-types: int | None
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
# 1. Current ty (no changes): Unknown | str
# 2. Future ty (after fix): str
# 3. mypy + django-stubs: str
# 4. pyright + django-types: str

reveal_type(settings.notifications_enabled)  # revealed: Unknown | bool
# 1. Current ty (no changes): Unknown | bool
# 2. Future ty (after fix): bool
# 3. mypy + django-stubs: bool
# 4. pyright + django-types: bool

reveal_type(settings.max_items)  # revealed: Unknown | int
# 1. Current ty (no changes): Unknown | int
# 2. Future ty (after fix): int
# 3. mypy + django-stubs: int
# 4. pyright + django-types: int
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
# 1. Current ty (no changes): Unknown | str
# 2. Future ty (after fix): CharField (the descriptor itself)
# 3. mypy + django-stubs: CharField
# 4. pyright + django-types: CharField

# Accessing field on instance returns the value type
book = Book()
reveal_type(book.title)  # revealed: Unknown | str
# 1. Current ty (no changes): Unknown | str
# 2. Future ty (after fix): str
# 3. mypy + django-stubs: str
# 4. pyright + django-types: str

# TODO: [Phase 3] Class access should return Field, instance access should return T
# ty needs to understand the descriptor protocol more completely
```
