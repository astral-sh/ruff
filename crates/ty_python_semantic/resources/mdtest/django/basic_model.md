# Django - Basic Model Support

Tests for basic Django Model class detection.

**Phase**: 0 (Baseline - what currently works)

## Simple Model Definition

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
from typing import Any

class Model:
    # TODO: [Phase 2] Auto-synthesize .objects manager
    # TODO: [Phase 2] Auto-synthesize .DoesNotExist exception
    # TODO: [Phase 2] Auto-add id field
    pk: Any
    def save(self, **kwargs: Any) -> None: ...
    def delete(self) -> tuple[int, dict[str, int]]: ...

class Field:
    pass

class CharField(Field):
    def __init__(self, max_length: int, **kwargs: Any): ...

CASCADE: Any
```

`test.py`:

```py
from django.db import models

class User(models.Model):
    name: str
    email: str

# Model class is detected
reveal_type(User)  # revealed: <class 'User'>
# 1. Current ty (no changes): <class 'User'>
# 2. Future ty (after fix): <class 'User'> (no change needed)
# 3. mypy + django-stubs: type[User]
# 4. pyright + django-types: type[User]

# TODO: [Phase 2] Should auto-synthesize .objects manager
# reveal_type(User.objects)
# 1. Current ty (no changes): error[unresolved-attribute]
# 2. Future ty (after fix): Manager[User]
# 3. mypy + django-stubs: Manager[User]
# 4. pyright + django-types: BaseManager[User]

# Instance creation works
user = User()
reveal_type(user)  # revealed: User
# 1. Current ty (no changes): User
# 2. Future ty (after fix): User (no change needed)
# 3. mypy + django-stubs: User
# 4. pyright + django-types: User

# TODO: [Phase 2] Annotated attributes should be accessible
# reveal_type(user.name)
# 1. Current ty (no changes): error[unresolved-attribute]
# 2. Future ty (after fix): str
# 3. mypy + django-stubs: str
# 4. pyright + django-types: str

# Base model methods are available
reveal_type(user.save)  # revealed: bound method User.save(**kwargs: Any) -> None
# 1. Current ty (no changes): bound method User.save(**kwargs: Any) -> None
# 2. Future ty (after fix): bound method User.save(**kwargs: Any) -> None (no change needed)
# 3. mypy + django-stubs: def (**kwargs: Any) -> None
# 4. pyright + django-types: (**kwargs: Any) -> None

reveal_type(user.delete)  # revealed: bound method User.delete() -> tuple[int, dict[str, int]]
# 1. Current ty (no changes): bound method User.delete() -> tuple[int, dict[str, int]]
# 2. Future ty (after fix): bound method User.delete() -> tuple[int, dict[str, int]] (no change needed)
# 3. mypy + django-stubs: def () -> tuple[int, dict[str, int]]
# 4. pyright + django-types: () -> tuple[int, dict[str, int]]
```
