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

# Method chaining preserves QuerySet type
filtered = User.objects.filter(name="test")
reveal_type(filtered)  # revealed: QuerySet[User]

# QuerySet.get() returns model instance
user = User.objects.get(pk=1)
reveal_type(user)  # revealed: User

# QuerySet methods chain correctly
chained = User.objects.all().filter(name="test").exclude(name="other")
reveal_type(chained)  # revealed: QuerySet[User]

# first() returns optional model instance
maybe_user = User.objects.first()
reveal_type(maybe_user)  # revealed: User | None
```
