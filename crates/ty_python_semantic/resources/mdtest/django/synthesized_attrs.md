# Django Synthesized Model Attributes

```toml
[environment]
python-version = "3.11"
python = "/.venv"
```

## Default pk and id resolve to int

Models without an explicit primary key field get `pk` and `id` synthesized as `int`.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import CharField, AutoField
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

class AutoField(Field[int, int]):
    pass
```

```py
from django.db.models import Model, CharField

class Product(Model):
    name = CharField(max_length=50)

p = Product()
reveal_type(p.pk)  # revealed: int
reveal_type(p.id)  # revealed: int
```

## UUIDField(primary_key=True) makes pk return uuid.UUID

When a model sets a `UUIDField` as the primary key, `pk` reflects that type.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import AutoField, UUIDField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
import uuid
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

class AutoField(Field[int, int]):
    pass

class UUIDField(Field[uuid.UUID, uuid.UUID]):
    def __init__(self, *, null: bool = False, blank: bool = False, default=None, primary_key: bool = False): ...
```

```py
from uuid import UUID
from django.db.models import Model, UUIDField

class Document(Model):
    id = UUIDField(primary_key=True)

d = Document()
reveal_type(d.pk)  # revealed: UUID
reveal_type(d.id)  # revealed: UUID
```

## CharField(primary_key=True) makes pk return str

When a model uses a `CharField` as the primary key, `pk` returns `str`.

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
    def __init__(
        self, *, max_length: int = 255, null: bool = False, blank: bool = False, default=None, primary_key: bool = False
    ): ...
```

```py
from django.db.models import Model, CharField

class Country(Model):
    code = CharField(max_length=2, primary_key=True)

c = Country()
reveal_type(c.pk)  # revealed: str
reveal_type(c.code)  # revealed: str
```

## Inherited custom PK propagates pk type to child

When a parent model defines a custom primary key, child models inherit it. `pk` on child instances
returns the same type.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import CharField, UUIDField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
import uuid
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
    def __init__(
        self, *, max_length: int = 255, null: bool = False, blank: bool = False, default=None, primary_key: bool = False
    ): ...

class UUIDField(Field[uuid.UUID, uuid.UUID]):
    def __init__(self, *, null: bool = False, blank: bool = False, default=None, primary_key: bool = False): ...
```

```py
from uuid import UUID
from django.db.models import Model, UUIDField, CharField

class BaseEntity(Model):
    id = UUIDField(primary_key=True)

class ChildEntity(BaseEntity):
    name = CharField(max_length=100)

c = ChildEntity()
reveal_type(c.pk)  # revealed: UUID
```

## Inherited custom PK suppresses implicit id on child

When a parent model defines a custom primary key that is not named `id`, child models inherit that
primary key and do not get an implicit `id` attribute.

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
class CharField:
    def __init__(self, *, max_length: int = 255, primary_key: bool = False, null: bool = False): ...
```

```py
from django.db.models import Model, CharField

class BaseWithCustomPK(Model):
    code = CharField(max_length=2, primary_key=True)

class ChildOfCustomPK(BaseWithCustomPK):
    name = CharField(max_length=100)

c = ChildOfCustomPK()
reveal_type(c.pk)  # revealed: str
reveal_type(c.code)  # revealed: str

# error: [unresolved-attribute]
reveal_type(c.id)  # revealed: Unknown
```

## CharField(primary_key=True) overrides the implicit auto pk

Explicitly setting `primary_key=True` on a `CharField` suppresses the auto-generated integer `id`,
so both `pk` and the named field are `str`, not `int`.

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
    def __init__(
        self, *, max_length: int = 255, null: bool = False, blank: bool = False, default=None, primary_key: bool = False
    ): ...
```

```py
from django.db.models import Model, CharField

class Currency(Model):
    code = CharField(max_length=3, primary_key=True)
    name = CharField(max_length=50)

c = Currency()
reveal_type(c.pk)  # revealed: str
reveal_type(c.code)  # revealed: str
```

## BigAutoField and SmallAutoField resolve to int

Explicit auto-field variants are recognized as integer primary keys.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import BigAutoField, SmallAutoField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
class BigAutoField:
    def __init__(self, *, primary_key: bool = False): ...

class SmallAutoField:
    def __init__(self, *, primary_key: bool = False): ...
```

```py
from django.db.models import Model, BigAutoField, SmallAutoField

class LargeTable(Model):
    id = BigAutoField(primary_key=True)

class SmallTable(Model):
    id = SmallAutoField(primary_key=True)

t = LargeTable()
reveal_type(t.id)  # revealed: int
reveal_type(t.pk)  # revealed: int

s = SmallTable()
reveal_type(s.id)  # revealed: int
reveal_type(s.pk)  # revealed: int
```

## Custom primary key suppresses implicit id

When a model uses a custom primary key, `obj.id` is not a valid attribute and `obj.pk` resolves to
the custom field's type.

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
class CharField:
    def __init__(self, *, max_length: int = 255, primary_key: bool = False, null: bool = False): ...
```

```py
from django.db.models import Model, CharField

class Country(Model):
    code = CharField(max_length=2, primary_key=True)

c = Country()
reveal_type(c.pk)  # revealed: str
reveal_type(c.code)  # revealed: str

# error: [unresolved-attribute]
reveal_type(c.id)  # revealed: Unknown
```
