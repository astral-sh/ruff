# Django

Django support in ty currently relies on the stubs from [django-types]:

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

# TODO: django-types should allow ISO-formatted date strings as input.
book.published = "1980-10-12"  # error: [invalid-assignment]

book.pages = models.F("pages") + 1

book.published = 123  # error: [invalid-assignment]
book.pages = None  # error: [invalid-assignment]
```

Fields can be omitted during construction (and assigned before saving), so this is also fine:

```py
Book(
    title="The Hitchhiker's Guide to the Galaxy",
    pages=180,
)
```

Invalid field types lead to a type error:

```py
# TODO: Reject the invalid type for `published`.
Book(
    title="The Hitchhiker's Guide to the Galaxy",
    published=123,
    pages=180,
)
```

Unrecognized keyword arguments also lead to an error:

```py
# TODO: Reject the unknown keyword `publised`.
Book(
    title="The Hitchhiker's Guide to the Galaxy",
    publised=date(1979, 10, 12),
    pages=180,
)
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

## Field names

The `name` parameter overrides the name of the attribute:

```py
from django.db import models

class Article(models.Model):
    views = models.IntegerField(name="view_count")

article = Article(view_count=1)

# TODO: Should not be an error, and the type should be `int`.
# error: [unresolved-attribute]
reveal_type(article.view_count)  # revealed: Unknown

# The original field name is not available as an attribute.
# TODO: Should be an unresolved-attribute error.
article.views

# This is correct:
Article(view_count=1)

# Using the original attribute name is incorrect.
# TODO: Reject the unknown keyword `views`.
Article(views=1)
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
    reveal_type(book.id)  # revealed: int
    reveal_type(book.pk)  # revealed: int
```

A subclass inherits the primary key behavior of its parent.

```py
class Ebook(Book):
    pass

def _(ebook: Ebook):
    reveal_type(ebook.id)  # revealed: int
```

### Custom primary key

An explicit primary key determines the type of `pk` and prevents Django from adding `id`.

```py
from django.db import models

class Book(models.Model):
    isbn = models.CharField(max_length=13, primary_key=True)

def _(book: Book):
    reveal_type(book.isbn)  # revealed: str
    reveal_type(book.pk)  # revealed: str

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

## Choices

The `choices` parameter can be used to map the actual stored values for a field to human-readable
labels. Django adds `get_<field>_display()` methods for fields that have `choices` defined:

```py
from django.db import models

class Article(models.Model):
    status = models.CharField(
        max_length=1,
        choices=[("D", "Draft"), ("P", "Published")],
    )
    visibility = models.CharField(
        name="access",
        max_length=1,
        choices=[("P", "Public"), ("R", "Private")],
    )

article = Article(status="P")

reveal_type(article.status)  # revealed: str
reveal_type(article.get_status_display())  # revealed: str
```

An explicit `name` determines the names of generated methods, overriding the assignment name:

```py
reveal_type(article.get_access_display())  # revealed: str

# A `get_<field>_display()` method for the original field name is not available.
article.get_visibility_display()  # error: [unresolved-attribute]
```

## Date navigation

Django adds `get_next_by_<field>()` and `get_previous_by_<field>()` methods for date fields that do
not have `null=True`. These methods return the next/previous instance of the corresponding model
when ordered by that specific date field:

```py
from django.db import models

class Article(models.Model):
    published = models.DateField()
    reviewed = models.DateField(null=True)

def _(article: Article):
    reveal_type(article.get_next_by_published())  # revealed: Article
    reveal_type(article.get_previous_by_published())  # revealed: Article

    # Date fields with `null=True` do not generate these methods.
    article.get_next_by_reviewed()  # error: [unresolved-attribute]
    article.get_previous_by_reviewed()  # error: [unresolved-attribute]
```

These methods return `Self`, so when invoked on subclasses, we get back instances of the subclass:

```py
class NewsArticle(Article):
    pass

def _(article: NewsArticle):
    reveal_type(article.get_next_by_published())  # revealed: NewsArticle
    reveal_type(article.get_previous_by_published())  # revealed: NewsArticle
    reveal_type(Article.get_next_by_published(article))  # revealed: NewsArticle
    reveal_type(Article.get_previous_by_published(article))  # revealed: NewsArticle
```

[django-types]: https://github.com/sbdchd/django-types
