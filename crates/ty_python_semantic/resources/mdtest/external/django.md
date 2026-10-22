# Django

These tests use [django-types] for Django's type stubs:

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["django-types==0.24.0"]
```

## Basic model

Define a basic model by subclassing Django's `Model`:

```py
from datetime import date
from django.db import models

class Book(models.Model):
    title = models.CharField(max_length=200)
    published = models.DateField()
    pages = models.IntegerField()
```

Create an instance of this model:

```py
# TODO: Synthesize a realistic constructor signature
reveal_type(Book.__init__)  # revealed: def __init__(self, *args: Any, **kwargs: Any) -> None

book = Book(
    title="The Hitchhiker's Guide to the Galaxy",
    published=date(1979, 10, 12),
    pages=180,
)

book.save()
```

Attribute access yields types based on the field definitions:

```py
reveal_type(book.title)  # revealed: str
reveal_type(book.published)  # revealed: date
reveal_type(book.pages)  # revealed: int
```

Attribute assignments are checked against the field's *input* type:

```py
book.title = "The Restaurant at the End of the Universe"
book.pages = 208

book.pages = models.F("pages") + 1

book.title = 123  # error: [invalid-assignment]
book.pages = None  # error: [invalid-assignment]
```

## Nullable fields

`null=True` adds `None` to the field's value type.

```py
from django.db import models

class Book(models.Model):
    title = models.CharField(max_length=200, null=True)
    pages = models.IntegerField(null=True)

def _(book: Book):
    reveal_type(book.title)  # revealed: str | None

    reveal_type(book.pages)  # revealed: int | None
```

## Primary keys

### Automatic primary key

Django adds an integer `id` field when no primary key is declared. The `pk` attribute is an alias
for the primary key.

```py
from django.db import models

class Book(models.Model):
    title = models.CharField()

def _(book: Book):
    # TODO: Should not be an error, and the type should be `int`
    # error: [unresolved-attribute]
    reveal_type(book.id)  # revealed: Unknown

    # TODO: Should be `int`
    reveal_type(book.pk)  # revealed: Any
```

A subclass inherits the primary key behavior of its parent.

```py
class Ebook(Book):
    pass

def _(ebook: Ebook):
    # TODO: Should not be an error, and the type should be `int`
    # error: [unresolved-attribute]
    reveal_type(ebook.id)  # revealed: Unknown
```

### Custom primary key

An explicit primary key determines the type of `pk` and prevents Django from adding `id`.

```py
from django.db import models

class Book(models.Model):
    isbn = models.CharField(max_length=13, primary_key=True)

def _(book: Book):
    reveal_type(book.isbn)  # revealed: str

    # TODO: Should be `str`
    reveal_type(book.pk)  # revealed: Any

    # This is an error now, `id` is not created when a custom primary key is defined.
    book.id  # error: [unresolved-attribute]
```

Subclassing a model with a custom primary key inherits the primary key behavior and does not
introduce an automatic `id` field.

```py
class Ebook(Book):
    pass

def _(ebook: Ebook):
    reveal_type(ebook.isbn)  # revealed: str
    ebook.id  # error: [unresolved-attribute]
```

### Explicit `id` field

An explicitly declared `id` keeps its field type:

```py
from django.db import models

class Book(models.Model):
    id = models.CharField(max_length=13, primary_key=True)

class Ebook(Book):
    pass

def _(book: Book, ebook: Ebook):
    reveal_type(book.id)  # revealed: str
    reveal_type(ebook.id)  # revealed: str
```

[django-types]: https://github.com/sbdchd/django-types
