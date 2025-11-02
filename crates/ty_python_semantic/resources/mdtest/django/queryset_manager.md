# Django - QuerySet and Manager Support

Tests for Django QuerySet and Manager generic typing.

**Phase**: 1 (Generic QuerySet/Manager)

## QuerySet and Manager Generics

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
    # TODO: [Phase 1] Properly implement generic QuerySet support
    def all(self) -> Self: ...
    def filter(self, **kwargs: Any) -> Self: ...
    def exclude(self, **kwargs: Any) -> Self: ...
    def get(self, **kwargs: Any) -> _M: ...
    def first(self) -> _M | None: ...
    def count(self) -> int: ...
    def create(self, **kwargs: Any) -> _M: ...

class Manager(Generic[_M]):
    # TODO: [Phase 1] Properly implement generic Manager support
    def all(self) -> QuerySet[_M]: ...
    def filter(self, **kwargs: Any) -> QuerySet[_M]: ...
    def get(self, **kwargs: Any) -> _M: ...
    def first(self) -> _M | None: ...
    def create(self, **kwargs: Any) -> _M: ...

class Model:
    # TODO: [Phase 2] Auto-synthesize .objects manager
    objects: Any
    pk: Any
```

`test.py`:

```py
from django.db import models

class User(models.Model):
    name: str

# TODO: [Phase 2] .objects should be auto-synthesized as Manager[User]
# Currently need to manually annotate
User.objects = models.Manager[User]()

# Manager methods return QuerySet with correct generic
qs = User.objects.all()
reveal_type(qs)  # revealed: QuerySet[User]
# 1. Current ty (no changes): QuerySet[User]
# 2. Future ty (after fix): QuerySet[User] (no change needed - works with manual annotation)
# 3. mypy + django-stubs: QuerySet[User, User]
# 4. pyright + django-types: QuerySet[User, User]

# Method chaining preserves QuerySet type
filtered = User.objects.filter(name="test")
reveal_type(filtered)  # revealed: QuerySet[User]
# 1. Current ty (no changes): QuerySet[User]
# 2. Future ty (after fix): QuerySet[User] (no change needed - works with manual annotation)
# 3. mypy + django-stubs: QuerySet[User, User]
# 4. pyright + django-types: QuerySet[User, User]

# QuerySet.get() returns model instance
user = User.objects.get(pk=1)
reveal_type(user)  # revealed: User
# 1. Current ty (no changes): User
# 2. Future ty (after fix): User (no change needed - works with manual annotation)
# 3. mypy + django-stubs: User
# 4. pyright + django-types: User

# QuerySet methods chain correctly
chained = User.objects.all().filter(name="test").exclude(name="other")
reveal_type(chained)  # revealed: QuerySet[User]
# 1. Current ty (no changes): QuerySet[User]
# 2. Future ty (after fix): QuerySet[User] (no change needed - works with manual annotation)
# 3. mypy + django-stubs: QuerySet[User, User]
# 4. pyright + django-types: QuerySet[User, User]

# first() returns optional model instance
maybe_user = User.objects.first()
reveal_type(maybe_user)  # revealed: User | None
# 1. Current ty (no changes): User | None
# 2. Future ty (after fix): User | None (no change needed - works with manual annotation)
# 3. mypy + django-stubs: User | None
# 4. pyright + django-types: User | None
```
