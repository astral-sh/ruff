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

# TODO: [Phase 2] Should auto-synthesize .objects manager
# reveal_type(User.objects)  # Should be: Manager[User]
# Currently: error[unresolved-attribute]

# Instance creation works
user = User()
reveal_type(user)  # revealed: User

# TODO: [Phase 2] Annotated attributes should be accessible
# reveal_type(user.name)  # Should be: str
# Currently: error[unresolved-attribute]

# Base model methods are available
reveal_type(user.save)  # revealed: bound method User.save(**kwargs: Any) -> None
reveal_type(user.delete)  # revealed: bound method User.delete() -> tuple[int, dict[str, int]]
```
