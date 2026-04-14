# Django ForeignKey Field Types

```toml
[environment]
python-version = "3.11"
python = "/.venv"
```

## ForeignKey on instance returns related model instance

Accessing a ForeignKey field on a model instance should return an instance of the related model, not
a `ForeignKey` descriptor.

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
    def __init__(self, to: type, *, on_delete, null: bool = False, related_name: str = "", db_column: str = ""): ...
```

```py
from django.db.models import Model, CharField, ForeignKey

class Author(Model):
    name = CharField(max_length=100)

class Book(Model):
    author = ForeignKey(Author, on_delete=None)

b = Book()
reveal_type(b.author)  # revealed: Author
reveal_type(b.author.name)  # revealed: str
```

## Nullable ForeignKey resolves to related model | None

When `null=True` is passed to `ForeignKey`, the instance access type becomes `RelatedModel | None`.

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
    def __init__(self, to: type, *, on_delete, null: bool = False, related_name: str = "", db_column: str = ""): ...
```

```py
from django.db.models import Model, CharField, ForeignKey

class Tag(Model):
    label = CharField(max_length=50)

class Article(Model):
    primary_tag = ForeignKey(Tag, null=True, on_delete=None)

a = Article()
reveal_type(a.primary_tag)  # revealed: Tag | None
```

## ForeignKey with explicit to= keyword

When `to=` is passed as a keyword argument rather than positionally, ty resolves the related model
the same way.

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
class CharField:
    def __init__(self, *, max_length: int = 255, null: bool = False): ...
```

`/.venv/<path-to-site-packages>/django/db/models/fields/related.py`:

```py
class ForeignKey:
    def __init__(self, to: type, *, on_delete, null: bool = False): ...
```

```py
from django.db.models import Model, CharField, ForeignKey

class Publisher(Model):
    name = CharField(max_length=100)

class Book(Model):
    publisher = ForeignKey(to=Publisher, on_delete=None)

b = Book()
reveal_type(b.publisher)  # revealed: Publisher
```

## OneToOneField resolves to related model instance

`OneToOneField` is resolved identically to `ForeignKey`.

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
from django.db.models.fields.related import OneToOneField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
class CharField:
    def __init__(self, *, max_length: int = 255, null: bool = False): ...
```

`/.venv/<path-to-site-packages>/django/db/models/fields/related.py`:

```py
class OneToOneField:
    def __init__(self, to: type, *, on_delete, null: bool = False): ...
```

```py
from django.db.models import Model, CharField, OneToOneField

class UserProfile(Model):
    display_name = CharField(max_length=100)

class Account(Model):
    profile = OneToOneField(UserProfile, on_delete=None)
    nullable_profile = OneToOneField(UserProfile, null=True, on_delete=None)

a = Account()
reveal_type(a.profile)  # revealed: UserProfile
reveal_type(a.nullable_profile)  # revealed: UserProfile | None
```

## ForeignObject resolves like ForeignKey

`ForeignObject` is Django's low-level base for relational fields. ty resolves it the same as
`ForeignKey`.

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
from django.db.models.fields.related import ForeignObject
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
class CharField:
    def __init__(self, *, max_length: int = 255, null: bool = False): ...
```

`/.venv/<path-to-site-packages>/django/db/models/fields/related.py`:

```py
class ForeignObject:
    def __init__(self, to: type, *, on_delete, null: bool = False): ...
```

```py
from django.db.models import Model, CharField, ForeignObject

class Target(Model):
    name = CharField(max_length=100)

class Source(Model):
    target = ForeignObject(Target, on_delete=None)

s = Source()
reveal_type(s.target)  # revealed: Target
```
