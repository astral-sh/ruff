# Django Model Basic Field Types

```toml
[environment]
python-version = "3.11"
python = "/.venv"
```

## Field access returns Python type (not descriptor)

Accessing a field on a model instance returns the Python value type, not the descriptor object.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import CharField, IntegerField, FloatField, BooleanField
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

class FloatField(Field[float, float]):
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...

class BooleanField(Field[bool, bool]):
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...
```

```py
from django.db.models import Model, CharField, IntegerField, FloatField, BooleanField

class Article(Model):
    title = CharField(max_length=100)
    view_count = IntegerField()
    rating = FloatField()
    published = BooleanField()

a = Article()
reveal_type(a.title)  # revealed: str
reveal_type(a.view_count)  # revealed: int
reveal_type(a.rating)  # revealed: float
reveal_type(a.published)  # revealed: bool

# TODO(ty#1018): class-level access currently returns the synthesized instance
# type rather than the field descriptor, because attribute resolution does not
# yet distinguish class-level from instance-level access.
reveal_type(Article.title)  # revealed: str
```

## Attribute-style field references (`models.CharField`) are recognized

Django code commonly uses `import django.db.models as models` and then declares fields as
`models.CharField(...)` rather than importing field classes individually. The module-qualified form
resolves the same as the bare import form.

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
class CharField:
    def __init__(self, *, max_length: int = 255, null: bool = False, blank: bool = False, default=None): ...

class IntegerField:
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...
```

```py
import django.db.models as models

class Post(models.Model):
    title = models.CharField(max_length=100)
    count = models.IntegerField()

p = Post()
reveal_type(p.title)  # revealed: str
reveal_type(p.count)  # revealed: int
```

## Stdlib type fields resolve to their corresponding Python types

`DateField`, `DateTimeField`, `TimeField`, `DecimalField`, and `UUIDField` resolve to their
corresponding stdlib types. When `null=True` is passed, the type is unioned with `None`.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import DateField, DateTimeField, TimeField, DecimalField, UUIDField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
class DateField:
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...

class DateTimeField:
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...

class TimeField:
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...

class DecimalField:
    def __init__(
        self, *, max_digits: int = 10, decimal_places: int = 2, null: bool = False, blank: bool = False, default=None
    ): ...

class UUIDField:
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...
```

```py
from django.db.models import Model, DateField, DateTimeField, TimeField, DecimalField, UUIDField

class Event(Model):
    start_date = DateField()
    start_datetime = DateTimeField()
    start_time = TimeField()
    amount = DecimalField(max_digits=10, decimal_places=2)
    session_id = UUIDField()

    nullable_date = DateField(null=True)
    nullable_datetime = DateTimeField(null=True)
    nullable_time = TimeField(null=True)
    nullable_amount = DecimalField(max_digits=10, decimal_places=2, null=True)
    nullable_token = UUIDField(null=True)

e = Event()
reveal_type(e.start_date)  # revealed: date
reveal_type(e.start_datetime)  # revealed: datetime
reveal_type(e.start_time)  # revealed: time
reveal_type(e.amount)  # revealed: Decimal
reveal_type(e.session_id)  # revealed: UUID

reveal_type(e.nullable_date)  # revealed: date | None
reveal_type(e.nullable_datetime)  # revealed: datetime | None
reveal_type(e.nullable_time)  # revealed: time | None
reveal_type(e.nullable_amount)  # revealed: Decimal | None
reveal_type(e.nullable_token)  # revealed: UUID | None
```

## BinaryField, FileField, ImageField, and JSONField

`BinaryField` resolves to `bytes`. `FileField` and `ImageField` return `FieldFile`/`ImageFieldFile`
at runtime, which ty does not yet model, so they resolve to `Unknown`. `JSONField` stores arbitrary
JSON data and also resolves to `Unknown`.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import BinaryField, FileField, ImageField, JSONField
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
class BinaryField:
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...

class FileField:
    def __init__(self, *, upload_to: str = "", null: bool = False, blank: bool = False, default=None): ...

class ImageField:
    def __init__(self, *, upload_to: str = "", null: bool = False, blank: bool = False, default=None): ...

class JSONField:
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...
```

```py
from django.db.models import Model, BinaryField, FileField, ImageField, JSONField

class Media(Model):
    data = BinaryField()
    optional_data = BinaryField(null=True)
    upload = FileField()
    photo = ImageField()
    config = JSONField()

m = Media()
reveal_type(m.data)  # revealed: bytes
reveal_type(m.optional_data)  # revealed: bytes | None
reveal_type(m.upload)  # revealed: Unknown
reveal_type(m.photo)  # revealed: Unknown
reveal_type(m.config)  # revealed: Unknown
```

## Unrecognized field classes fall back to normal inference

A field class not recognized as a Django field falls through to normal descriptor inference.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import UnknownCustomField
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

class UnknownCustomField(Field):
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...
```

```py
from django.db.models import Model, UnknownCustomField

class Fallback(Model):
    data = UnknownCustomField()
    nullable_data = UnknownCustomField(null=True)

f = Fallback()
reveal_type(f.data)  # revealed: Unknown
reveal_type(f.nullable_data)  # revealed: Unknown
```

## CharField variant fields resolve to str

Django ships several `CharField` subclasses with built-in validation. ty maps all of them to `str`.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import (
    SlugField,
    EmailField,
    URLField,
    TextField,
    GenericIPAddressField,
    IPAddressField,
    FilePathField,
)
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
class SlugField:
    def __init__(self, *, max_length: int = 50, null: bool = False): ...

class EmailField:
    def __init__(self, *, max_length: int = 254, null: bool = False): ...

class URLField:
    def __init__(self, *, max_length: int = 200, null: bool = False): ...

class TextField:
    def __init__(self, *, null: bool = False, blank: bool = False, default=None): ...

class GenericIPAddressField:
    def __init__(self, *, protocol: str = "both", null: bool = False): ...

class IPAddressField:
    def __init__(self, *, null: bool = False): ...

class FilePathField:
    def __init__(self, *, path: str = "", null: bool = False): ...
```

```py
from django.db.models import (
    Model,
    SlugField,
    EmailField,
    URLField,
    TextField,
    GenericIPAddressField,
    IPAddressField,
    FilePathField,
)

class Page(Model):
    slug = SlugField()
    contact_email = EmailField()
    source_url = URLField()
    body = TextField()
    ip_address = GenericIPAddressField()
    legacy_ip = IPAddressField()
    config_path = FilePathField()

p = Page()
reveal_type(p.slug)  # revealed: str
reveal_type(p.contact_email)  # revealed: str
reveal_type(p.source_url)  # revealed: str
reveal_type(p.body)  # revealed: str
reveal_type(p.ip_address)  # revealed: str
reveal_type(p.legacy_ip)  # revealed: str
reveal_type(p.config_path)  # revealed: str
```

## Integer field variants resolve to int

Django's integer field subclasses all resolve to `int` regardless of their range constraints.

`/.venv/<path-to-site-packages>/django/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/django/db/models/__init__.py`:

```py
from django.db.models.base import Model
from django.db.models.fields import (
    SmallIntegerField,
    BigIntegerField,
    PositiveIntegerField,
    PositiveSmallIntegerField,
    PositiveBigIntegerField,
)
```

`/.venv/<path-to-site-packages>/django/db/models/base.py`:

```py
class Model:
    pass
```

`/.venv/<path-to-site-packages>/django/db/models/fields/__init__.py`:

```py
class SmallIntegerField:
    def __init__(self, *, null: bool = False): ...

class BigIntegerField:
    def __init__(self, *, null: bool = False): ...

class PositiveIntegerField:
    def __init__(self, *, null: bool = False): ...

class PositiveSmallIntegerField:
    def __init__(self, *, null: bool = False): ...

class PositiveBigIntegerField:
    def __init__(self, *, null: bool = False): ...
```

```py
from django.db.models import (
    Model,
    SmallIntegerField,
    BigIntegerField,
    PositiveIntegerField,
    PositiveSmallIntegerField,
    PositiveBigIntegerField,
)

class Counters(Model):
    small = SmallIntegerField()
    big = BigIntegerField()
    positive = PositiveIntegerField()
    positive_small = PositiveSmallIntegerField()
    positive_big = PositiveBigIntegerField()

c = Counters()
reveal_type(c.small)  # revealed: int
reveal_type(c.big)  # revealed: int
reveal_type(c.positive)  # revealed: int
reveal_type(c.positive_small)  # revealed: int
reveal_type(c.positive_big)  # revealed: int
```

## Annotated field declarations trigger invalid-assignment but synthesis still works

Writing `title: str = CharField(max_length=100)` causes an `invalid-assignment` diagnostic because
`CharField(...)` returns a descriptor object, not a `str`, so the assigned value is incompatible
with the declared annotation. The field is still recognized and produces the correct instance type
for attribute access.

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
class CharField:
    def __init__(self, *, max_length: int = 255, null: bool = False): ...

class IntegerField:
    def __init__(self, *, null: bool = False): ...
```

```py
from django.db.models import Model, CharField, IntegerField

class AnnotatedModel(Model):
    title: str = CharField(max_length=100)  # error: [invalid-assignment]
    count: int = IntegerField()  # error: [invalid-assignment]

m = AnnotatedModel()
reveal_type(m.title)  # revealed: str
reveal_type(m.count)  # revealed: int
```
