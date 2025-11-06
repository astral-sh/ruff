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
    objects: models.Manager[
        Self
    ]  # error: [invalid-argument-type] "Argument to class `Manager` is incorrect: Expected `Model`, found `typing.Self`"

# Manager type inference
reveal_type(User.objects)  # revealed: Unknown
# 1. Current ty (no changes): Unknown (Self not accepted as generic argument)
# 2. Future ty (after fix): Manager[User]
# 3. mypy + django-stubs: Manager[User]
# 4. pyright + django-types: BaseManager[User]
# TODO: [Phase 2] Implement PEP 673 Self type support

# QuerySet from Manager.all()
all_users = User.objects.all()
reveal_type(all_users)  # revealed: Unknown
# 1. Current ty (no changes): Unknown (propagates from Manager[Self] error)
# 2. Future ty (after fix): QuerySet[User]
# 3. mypy + django-stubs: QuerySet[User, User]
# 4. pyright + django-types: QuerySet[User, User]

# Model instance from QuerySet.get()
user = User.objects.get(id=1)
reveal_type(user)  # revealed: Unknown
# 1. Current ty (no changes): Unknown (propagates from Manager[Self] error)
# 2. Future ty (after fix): User
# 3. mypy + django-stubs: User
# 4. pyright + django-types: User

# Optional from QuerySet.first()
maybe_user = User.objects.first()
reveal_type(maybe_user)  # revealed: Unknown
# 1. Current ty (no changes): Unknown (propagates from Manager[Self] error)
# 2. Future ty (after fix): User | None
# 3. mypy + django-stubs: User | None
# 4. pyright + django-types: User | None

# TODO: Count method not found due to Manager[Self] error
# count = User.objects.count()
# 1. Current ty (no changes): error[unresolved-attribute]
# 2. Future ty (after fix): int
# 3. mypy + django-stubs: int
# 4. pyright + django-types: int

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

TODO: [Phase 2] These tests require Self type resolution to work. Currently commented out because
Self resolves to Unknown in class body.

## Custom Manager

TODO: [Phase 2] Requires Self type resolution.

## Manager Class Access vs Instance Access

TODO: [Phase 2] Requires Self type resolution.
