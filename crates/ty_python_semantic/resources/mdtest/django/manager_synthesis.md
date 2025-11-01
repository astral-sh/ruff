# Django - Manager Auto-Synthesis

Tests for Django Manager auto-synthesis and type inference.

**Phase**: 2 (Manager Synthesis - ty's Competitive Advantage)

## Auto-synthesized .objects Manager

Django's ModelBase metaclass auto-synthesizes a `.objects` Manager attribute on every Model class.

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
from typing_extensions import Self

_M = TypeVar("_M", bound="Model")

class QuerySet(Generic[_M]):
    def all(self) -> Self: ...
    def filter(self, **kwargs: Any) -> Self: ...
    def get(self, **kwargs: Any) -> _M: ...
    def first(self) -> _M | None: ...
    def count(self) -> int: ...

class Manager(Generic[_M]):
    def all(self) -> QuerySet[_M]: ...
    def filter(self, **kwargs: Any) -> QuerySet[_M]: ...
    def get(self, **kwargs: Any) -> _M: ...
    def first(self) -> _M | None: ...

class Model:
    pk: Any
    def save(self, **kwargs: Any) -> None: ...
```

`test.py`:

```py
from django.db import models
from typing_extensions import Self

class User(models.Model):
    name: str
    # Note: In real Django, .objects is auto-synthesized by metaclass
    # For now, we manually annotate it (like mypy/pyright require)
    objects: models.Manager[Self]  # error: [invalid-argument-type] "Argument to class `Manager` is incorrect: Expected `Model`, found `typing.Self`"

# Manager type inference
reveal_type(User.objects)  # revealed: Unknown
# Current ty output: Unknown  ❌ Self not accepted as generic argument!
# Target (mypy): Manager[User]
# Target (pyright): BaseManager[User]
# TODO: [Phase 2] Fix Self type resolution in class body

# QuerySet from Manager.all()
all_users = User.objects.all()
reveal_type(all_users)  # revealed: Unknown
# Current ty output: Unknown  ❌ Propagates from Manager[Self] error
# Target (mypy): QuerySet[User, User]
# Target (pyright): BaseManager[User]

# Model instance from QuerySet.get()
user = User.objects.get(id=1)
reveal_type(user)  # revealed: Unknown
# Current ty output: Unknown  ❌ Propagates from Manager[Self] error
# Target (mypy): User
# Target (pyright): User

# Optional from QuerySet.first()
maybe_user = User.objects.first()
reveal_type(maybe_user)  # revealed: Unknown
# Current ty output: Unknown  ❌ Propagates from Manager[Self] error
# Target (mypy): User | None
# Target (pyright): User | None

# TODO: Count method not found due to Manager[Unknown]
# count = User.objects.count()
# reveal_type(count)  # Should be: int
# Current ty output: error[unresolved-attribute]
# This breaks because Self resolves to Unknown

# TODO: [Phase 2] Auto-synthesize .objects so manual annotation not needed
# class AutoUser(models.Model):
#     name: str
#     # .objects should be auto-synthesized here
#
# reveal_type(AutoUser.objects)  # Should be: Manager[AutoUser]
# Currently: error[unresolved-attribute]
# This is where ty could beat mypy/pyright!
```

## QuerySet Method Chaining

TODO: [Phase 2] These tests require Self type resolution to work.
Currently commented out because Self resolves to Unknown in class body.

## Custom Manager

TODO: [Phase 2] Requires Self type resolution.

## Manager Class Access vs Instance Access

TODO: [Phase 2] Requires Self type resolution.
