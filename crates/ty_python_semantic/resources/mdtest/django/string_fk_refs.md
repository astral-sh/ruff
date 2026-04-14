# Django String FK References

```toml
[environment]
python-version = "3.11"
python = "/.venv"
```

## ForeignKey with string "self" reference

`ForeignKey("self", ...)` creates a self-referential relation. The field type resolves to the
enclosing model class.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import CharField
from django.db.models.fields.related import ForeignKey
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
```

`/.venv/<path-to-site-packages>/django/db/models/fields/related.py`:

```py
from typing import Generic, TypeVar, overload

_To = TypeVar("_To")

class ForeignKey(Generic[_To]):
    @overload
    def __get__(self, instance: None, owner: type) -> "ForeignKey[_To]": ...
    @overload
    def __get__(self, instance: object, owner: type) -> _To: ...
    def __get__(self, instance, owner): ...
    def __init__(self, to, *, on_delete, null: bool = False, related_name: str = "", db_column: str = ""): ...
```

```py
from django.db.models import Model, CharField, ForeignKey

class Category(Model):
    name = CharField(max_length=100)
    parent = ForeignKey("self", on_delete=None, null=True)

c = Category()
reveal_type(c.parent)  # revealed: Category | None
```

## ForeignKey with string model name in same file

Passing a class name as a string (`ForeignKey("Author", ...)`) handles forward references to models
defined later in the same file.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import CharField
from django.db.models.fields.related import ForeignKey
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
```

`/.venv/<path-to-site-packages>/django/db/models/fields/related.py`:

```py
from typing import Generic, TypeVar, overload

_To = TypeVar("_To")

class ForeignKey(Generic[_To]):
    @overload
    def __get__(self, instance: None, owner: type) -> "ForeignKey[_To]": ...
    @overload
    def __get__(self, instance: object, owner: type) -> _To: ...
    def __get__(self, instance, owner): ...
    def __init__(self, to, *, on_delete, null: bool = False, related_name: str = "", db_column: str = ""): ...
```

```py
from django.db.models import Model, CharField, ForeignKey

class Author(Model):
    name = CharField(max_length=100)

class Book(Model):
    author = ForeignKey("Author", on_delete=None)

b = Book()
reveal_type(b.author)  # revealed: Author
```

## ForeignKey with settings.AUTH_USER_MODEL

`ForeignKey(settings.AUTH_USER_MODEL, ...)` passes a runtime string attribute as the target. The
model cannot be statically resolved, so the field type is `Unknown`.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields.related import ForeignKey
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/related.py`:

```py
from typing import Generic, TypeVar, overload

_To = TypeVar("_To")

class ForeignKey(Generic[_To]):
    @overload
    def __get__(self, instance: None, owner: type) -> "ForeignKey[_To]": ...
    @overload
    def __get__(self, instance: object, owner: type) -> _To: ...
    def __get__(self, instance, owner): ...
    def __init__(self, to, *, on_delete, null: bool = False, related_name: str = "", db_column: str = ""): ...
```

```py
from django.db.models import Model, ForeignKey

class settings:
    AUTH_USER_MODEL: str = "auth.User"

class Profile(Model):
    user = ForeignKey(settings.AUTH_USER_MODEL, on_delete=None)

p = Profile()
reveal_type(p.user)  # revealed: Unknown
```

## Unresolvable string reference falls back gracefully

When a string reference cannot be resolved to a class in scope, the field type is `Unknown`.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields.related import ForeignKey
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/related.py`:

```py
from typing import Generic, TypeVar, overload

_To = TypeVar("_To")

class ForeignKey(Generic[_To]):
    @overload
    def __get__(self, instance: None, owner: type) -> "ForeignKey[_To]": ...
    @overload
    def __get__(self, instance: object, owner: type) -> _To: ...
    def __get__(self, instance, owner): ...
    def __init__(self, to, *, on_delete, null: bool = False, related_name: str = "", db_column: str = ""): ...
```

```py
from django.db.models import Model, ForeignKey

class Article(Model):
    related = ForeignKey("NoSuchModel", on_delete=None)

a = Article()
reveal_type(a.related)  # revealed: Unknown
```

## Dotted app-label string references resolve to Unknown

`ForeignKey("app_label.ModelName", ...)` uses Django's cross-app reference format. Dotted paths
cannot be resolved statically and produce `Unknown`.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields.related import ForeignKey
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/related.py`:

```py
class ForeignKey:
    def __init__(self, to, *, on_delete, null: bool = False): ...
```

```py
from django.db.models import Model, ForeignKey

class Order(Model):
    user = ForeignKey("accounts.User", on_delete=None)

o = Order()
reveal_type(o.user)  # revealed: Unknown
```
