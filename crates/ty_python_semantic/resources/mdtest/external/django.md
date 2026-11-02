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

## Foreign key IDs

### Automatic primary key

A `ForeignKey` field `author` can be used to access the referenced `Author` instance via `.author`.
It also creates an `.author_id` attribute which contains that instance's primary-key value:

```py
from django.db import models

class Author(models.Model):
    pass

class Book(models.Model):
    author = models.ForeignKey(Author, on_delete=models.CASCADE)
    editor = models.ForeignKey(to=Author, on_delete=models.SET_NULL, null=True, related_name="edited_books")

def _(book: Book):
    reveal_type(book.author)  # revealed: Author
    reveal_type(book.author_id)  # revealed: int

    reveal_type(book.editor)  # revealed: Author | None
    reveal_type(book.editor_id)  # revealed: int | None

    book.editor = None
    book.author = None  # error: [invalid-assignment]
```

A `ForeignKey` adds a reverse accessor to the related model. By default, its name is the lowercase
source model name followed by `_set`. The `related_name` parameter overrides this name:

```py
def _(author: Author):
    # TODO: Should not be an error, and the type should be `QuerySet[Book]`.
    # error: [unresolved-attribute]
    reveal_type(author.book_set.all())  # revealed: Unknown

    # TODO: Should not be an error, and the type should be `QuerySet[Book]`.
    # error: [unresolved-attribute]
    reveal_type(author.edited_books.all())  # revealed: Unknown
```

Setting `related_name="+"` disables the reverse accessor:

```py
class Review(models.Model):
    reviewer = models.ForeignKey(Author, on_delete=models.CASCADE, related_name="+")

def _(author: Author):
    author.review_set  # error: [unresolved-attribute]
```

### Custom primary key

The type of `<field>_id` follows the related model's primary key, which need not be an integer:

```py
from django.db import models

class Book(models.Model):
    isbn = models.CharField(max_length=13, primary_key=True)

class Review(models.Model):
    book = models.ForeignKey(Book, on_delete=models.CASCADE)

def _(review: Review):
    reveal_type(review.book)  # revealed: Book

    reveal_type(review.book_id)  # revealed: str
```

### Explicit target field

With `to_field`, `<field>_id` contains the value of that field instead of the primary key. In the
following example, the foreign key refers to the `isbn` field, not to `Book`'s primary key (`id`):

```py
from django.db import models

class Book(models.Model):
    id = models.IntegerField(primary_key=True)
    isbn = models.CharField(max_length=13, unique=True)

class Review(models.Model):
    book = models.ForeignKey(Book, to_field="isbn", on_delete=models.CASCADE)

def _(review: Review):
    reveal_type(review.book)  # revealed: Book

    reveal_type(review.book_id)  # revealed: str
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
    updated = models.DateTimeField()
    reviewed = models.DateField(null=True)

def _(article: Article):
    reveal_type(article.get_next_by_published())  # revealed: Article
    reveal_type(article.get_previous_by_published())  # revealed: Article

    # Date fields with `null=True` do not generate these methods.
    article.get_next_by_reviewed()  # error: [unresolved-attribute]
    article.get_previous_by_reviewed()  # error: [unresolved-attribute]
```

`DateTimeField` also generates these methods:

```py
def _(article: Article):
    reveal_type(article.get_next_by_updated())  # revealed: Article
    reveal_type(article.get_previous_by_updated())  # revealed: Article
```

Keyword arguments filter the candidate instances:

```py
def _(article: Article):
    reveal_type(article.get_next_by_published(published__year=2026))  # revealed: Article
    reveal_type(article.get_previous_by_published(published__year=2026))  # revealed: Article
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

## Abstract models

### Basic

An abstract model provides reusable fields without creating its own database table. Its concrete
subclasses receive an integer primary key unless they declare or inherit an explicit primary key.

```py
from django.db import models

class AbstractBook(models.Model):
    title = models.CharField(max_length=200)

    class Meta:
        abstract = True

class Ebook(AbstractBook):
    file_size = models.IntegerField()

class Book(AbstractBook):
    isbn = models.CharField(max_length=13, primary_key=True)

def _(abstract: AbstractBook, ebook: Ebook, book: Book):
    # Abstract models do not have an automatic primary key.
    abstract.id  # error: [unresolved-attribute]

    # Concrete models without an explicit primary key have an automatic integer key.
    reveal_type(ebook.id)  # revealed: int
    reveal_type(ebook.pk)  # revealed: int
    reveal_type(ebook.title)  # revealed: str
    reveal_type(ebook.file_size)  # revealed: int

    # The explicit ISBN primary key prevents an automatic `id`.
    book.id  # error: [unresolved-attribute]
    reveal_type(book.pk)  # revealed: str
    reveal_type(book.title)  # revealed: str
    reveal_type(book.isbn)  # revealed: str
```

### Annotated abstract setting

A bare annotation for `Meta.abstract` does not make a model abstract.

```py
from django.db import models

class Book(models.Model):
    class Meta:
        abstract: bool

def _(book: Book):
    reveal_type(book.id)  # revealed: int
    reveal_type(book.pk)  # revealed: int
```

When an assignment follows the annotation, its value determines whether the model is abstract.

```py
class AbstractBook(models.Model):
    class Meta:
        abstract: bool
        abstract = True

def _(abstract: AbstractBook):
    abstract.id  # error: [unresolved-attribute]
```

### Inherited Meta configuration

Django resets an abstract model's `Meta.abstract` to `False`. A child that subclasses this `Meta` is
concrete unless it explicitly sets `abstract = True` again.

```py
from django.db import models

class AbstractBook(models.Model):
    class Meta:
        abstract = True

class NotAbstract(AbstractBook):
    class Meta(AbstractBook.Meta):
        ordering = ["id"]

class Abstract(AbstractBook):
    class Meta(AbstractBook.Meta):
        abstract = True

def _(not_abstract: NotAbstract, abstract: Abstract):
    # concrete models have an automatic primary key
    not_abstract.id

    # abstract models do not:
    abstract.id  # error: [unresolved-attribute]
```

Configuration inherited from an ordinary class retains its `abstract` setting.

```py
class Options:
    abstract = True

class InheritedAbstract(models.Model):
    class Meta(Options):
        pass

def _(abstract: InheritedAbstract):
    abstract.id  # error: [unresolved-attribute]
```

### Multiple inheritance

An abstract base without a primary key does not shadow an inherited `id`, regardless of the base
class order. Regression test for <https://github.com/astral-sh/ty/issues/4822>.

```py
from uuid import uuid4
from django.db import models

class AbstractHasTitle(models.Model):
    title = models.CharField(max_length=200)

    class Meta:
        abstract = True

class AbstractHasId(models.Model):
    id = models.UUIDField(primary_key=True, default=uuid4)

    class Meta:
        abstract = True

class Book1(AbstractHasTitle, AbstractHasId):
    pass

class Book2(AbstractHasId, AbstractHasTitle):
    pass

def _(book1: Book1, book2: Book2):
    reveal_type(book1.id)  # revealed: UUID
    reveal_type(book1.pk)  # revealed: UUID
    reveal_type(book1.title)  # revealed: str

    reveal_type(book2.id)  # revealed: UUID
    reveal_type(book2.pk)  # revealed: UUID
    reveal_type(book2.title)  # revealed: str
```

[django-types]: https://github.com/sbdchd/django-types
