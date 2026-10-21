# Django

Django support in ty relies on [django-stubs]:

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["django-stubs==6.1.2"]
```

## Basic model

A basic Django model might look like this:

```py
from django.db import models

class Book(models.Model):
    title = models.CharField(max_length=200)
    description = models.TextField()
    pages = models.IntegerField()

# TODO: Synthesize a realistic constructor signature
reveal_type(Book.__init__)  # revealed: def __init__(self, *args: Any, **kwargs: Any) -> None

book = Book(title="Example", description="An example book", pages=123)

# TODO: should be `str`
reveal_type(book.title)  # revealed: Unknown

# TODO: should be `str`
reveal_type(book.description)  # revealed: Unknown

# TODO: should be `int`
reveal_type(book.pages)  # revealed: Unknown
```

## Nullable fields

`null=True` adds `None` to the field's value type. 

```py
from django.db import models

class Book(models.Model):
    title = models.CharField(max_length=200, null=True)
    pages = models.IntegerField(null=True)

def _(book: Book):
    # TODO: Should be `str | None`
    reveal_type(book.title)  # revealed: Unknown
    # TODO: int | None
    reveal_type(book.pages)  # revealed: Unknown
```

## Automatic primary key

Django adds an integer `id` field when no primary key is declared. The `pk` attribute is an alias
for the primary key.

```py
from django.db import models

class Book(models.Model):
    title = models.CharField(max_length=200)

def _(book: Book):
    # TODO: Should not be an error, and the type shoule be `int`
    # error: [unresolved-attribute]
    reveal_type(book.id)  # revealed: Unknown

    # TODO: Should be `int`
    reveal_type(book.pk)  # revealed: Any
```

## Custom primary key

An explicit primary key determines the type of `pk` and prevents Django from adding `id`.

```py
from django.db import models

class Book(models.Model):
    isbn = models.CharField(max_length=13, primary_key=True)

def _(book: Book):
    # TODO: Should be `str`
    reveal_type(book.isbn)  # revealed: Unknown

    # TODO: Should be `str`
    reveal_type(book.pk)  # revealed: Any

    book.id  # error: [unresolved-attribute]
```

[django-stubs]: https://github.com/typeddjango/django-stubs
