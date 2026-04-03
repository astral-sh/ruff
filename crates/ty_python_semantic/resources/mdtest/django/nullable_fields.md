# Django Model Nullable Fields

```toml
[environment]
python-version = "3.11"
python = "/.venv"
```

## Nullable fields resolve to T | None

When a field is declared with `null=True`, accessing it on an instance returns `T | None` rather
than just `T`.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import CharField, IntegerField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
from typing import Generic, TypeVar, overload

_ST = TypeVar("_ST")
_GT = TypeVar("_GT")

class Field(Generic[_ST, _GT]):
    @overload
    def __get__(self, instance: None, owner: type) -> "Field[_ST, _GT]": ...
    @overload
    def __get__(self, instance: object, owner: type) -> _GT: ...
    def __get__(self, instance, owner): ...
    def __set__(self, instance: object, value: _ST) -> None: ...

class CharField(Field[str, str]):
    def __init__(self, *, max_length: int = 255, null: bool = False, blank: bool = False, default=None): ...

class IntegerField(Field[int, int]):
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...
```

```py
from django.db.models import Model, CharField, IntegerField

class Post(Model):
    title = CharField(max_length=200)
    subtitle = CharField(max_length=200, null=True)
    views = IntegerField()
    rank = IntegerField(null=True)

p = Post()
reveal_type(p.title)  # revealed: str
reveal_type(p.subtitle)  # revealed: str | None
reveal_type(p.views)  # revealed: int
reveal_type(p.rank)  # revealed: int | None
```

## NullBooleanField is always nullable

`NullBooleanField` stores `True`, `False`, or `NULL` at the database level. Unlike other fields
where `null=True` must be explicit, `NullBooleanField` is intrinsically nullable regardless of the
`null=` argument.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import NullBooleanField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
from typing import Generic, TypeVar, overload

_ST = TypeVar("_ST")
_GT = TypeVar("_GT")

class Field(Generic[_ST, _GT]):
    @overload
    def __get__(self, instance: None, owner: type) -> "Field[_ST, _GT]": ...
    @overload
    def __get__(self, instance: object, owner: type) -> _GT: ...
    def __get__(self, instance, owner): ...
    def __set__(self, instance: object, value: _ST) -> None: ...

class NullBooleanField(Field[bool | None, bool | None]):
    def __init__(self, *, blank: bool = False, default=None): ...
```

```py
from django.db.models import Model, NullBooleanField

class Flag(Model):
    active = NullBooleanField()

f = Flag()
reveal_type(f.active)  # revealed: bool | None
```

## NullBooleanField with explicit null=False is still nullable

`NullBooleanField` is intrinsically nullable regardless of an explicit `null=False` argument.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import NullBooleanField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
from typing import Generic, TypeVar, overload

_ST = TypeVar("_ST")
_GT = TypeVar("_GT")

class Field(Generic[_ST, _GT]):
    @overload
    def __get__(self, instance: None, owner: type) -> "Field[_ST, _GT]": ...
    @overload
    def __get__(self, instance: object, owner: type) -> _GT: ...
    def __get__(self, instance, owner): ...
    def __set__(self, instance: object, value: _ST) -> None: ...

class NullBooleanField(Field[bool | None, bool | None]):
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...
```

```py
from django.db.models import Model, NullBooleanField

class Toggle(Model):
    active = NullBooleanField(null=False)

t = Toggle()
reveal_type(t.active)  # revealed: bool | None
```
