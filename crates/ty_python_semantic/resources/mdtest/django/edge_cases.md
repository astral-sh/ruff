# Django Model Edge Cases

```toml
[environment]
python-version = "3.11"
python = "/.venv"
```

## Model with no fields has pk and id synthesized

A model with no field declarations still has `pk` and `id` as `int` from the implicit auto primary
key.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

```py
from django.db.models import Model

class Empty(Model):
    pass

e = Empty()
reveal_type(e.pk)  # revealed: int
reveal_type(e.id)  # revealed: int
```

## Circular ForeignKey between two models in the same file

When model `A` references `B` via FK and `B` references `A` via FK in the same file, both forward
references resolve correctly.

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
from typing import Generic, TypeVar, overload, Union

_To = TypeVar("_To")

class ForeignKey(Generic[_To]):
    @overload
    def __get__(self, instance: None, owner: type) -> "ForeignKey[_To]": ...
    @overload
    def __get__(self, instance: object, owner: type) -> _To: ...
    def __get__(self, instance, owner): ...
    def __init__(self, to: Union[type, str], *, on_delete, null: bool = False, related_name: str = "", db_column: str = ""): ...
```

```py
from django.db.models import Model, ForeignKey

class NodeA(Model):
    partner = ForeignKey("NodeB", on_delete=None)

class NodeB(Model):
    partner = ForeignKey("NodeA", on_delete=None)

a = NodeA()
b = NodeB()
reveal_type(a.partner)  # revealed: NodeB
reveal_type(b.partner)  # revealed: NodeA
```

## Fields from a chain of abstract models are visible on the concrete subclass

Fields defined on a chain of abstract models all appear on the final concrete model.

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

class AbstractBase(Model):
    base_field = CharField(max_length=50)

    class Meta:
        abstract = True

class AbstractMiddle(AbstractBase):
    middle_field = IntegerField()

    class Meta:
        abstract = True

class Concrete(AbstractMiddle):
    own_field = CharField(max_length=100)

c = Concrete()
reveal_type(c.base_field)  # revealed: str
reveal_type(c.middle_field)  # revealed: int
reveal_type(c.own_field)  # revealed: str
```

## Class named Model that is not a Django model

A class named `Model` that does not inherit from `django.db.models.Model` does not receive any
Django-specific attribute synthesis.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

```py
class Model:
    name: str = "placeholder"

m = Model()
reveal_type(m.name)  # revealed: str
```

## Accessing a nonexistent attribute on a Django model is an error

Accessing an attribute that is not a declared field or a synthesized virtual attribute (`pk`, `id`)
is an `unresolved-attribute` error.

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

```py
from django.db.models import Model, CharField

class Article(Model):
    title = CharField(max_length=100)

a = Article()
a.title  # fine
a.nonexistent_field  # error: [unresolved-attribute]
```
