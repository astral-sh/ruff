# Unpack for \*\*kwargs (PEP 692)

PEP 692 introduced the ability to use `Unpack[TypedDict]` to more precisely type `**kwargs`
parameters.

## Basic usage

Inside a function with `**kwargs: Unpack[TypedDict]`, the kwargs parameter is typed as the TypedDict
itself:

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

def foo(**kwargs: Unpack[Movie]) -> None:
    reveal_type(kwargs)  # revealed: Movie
```

## kwargs is typed as TypedDict

```py
from typing import TypedDict
from typing_extensions import Unpack

class Config(TypedDict):
    debug: bool
    verbose: bool

def configure(**kwargs: Unpack[Config]) -> None:
    # kwargs is typed as Config
    reveal_type(kwargs)  # revealed: Config

    # We can access keys from the TypedDict
    debug = kwargs["debug"]
    reveal_type(debug)  # revealed: bool
```

## Required and NotRequired keys

TypedDict keys can be required or not required:

```py
from typing import TypedDict
from typing_extensions import Unpack, NotRequired

class Options(TypedDict):
    required_key: str
    optional_key: NotRequired[int]

def with_options(**kwargs: Unpack[Options]) -> None:
    reveal_type(kwargs)  # revealed: Options
```

## Invalid Unpack usage

`Unpack` should only be used with TypedDict types (or TypeVarTuple for variadic generics):

```py
from typing_extensions import Unpack

# error: [invalid-type-form] "`Unpack` must be used with a TypedDict, TypeVarTuple, or tuple type, got `int`"
def invalid(**kwargs: Unpack[int]) -> None:
    pass
```

## Using a regular TypeVar with Unpack is invalid

```py
from typing import TypeVar
from typing_extensions import Unpack

T = TypeVar("T")

# error: [invalid-type-form] "`Unpack` must be used with a TypedDict, TypeVarTuple, or tuple type, got `T@invalid_typevar`"
def invalid_typevar(**kwargs: Unpack[T]) -> None:
    pass
```

## Unpack without argument

`Unpack` requires exactly one type argument:

```py
from typing_extensions import Unpack

# error: [invalid-type-form] "`typing.Unpack` requires exactly one argument when used in a type expression"
x: Unpack
```

## Call binding with Unpack[TypedDict]

When calling a function with `**kwargs: Unpack[TypedDict]`, keyword arguments are validated against
the TypedDict fields.

### Valid calls with all required fields

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

def foo(**kwargs: Unpack[Movie]) -> None:
    pass

# Valid: all required fields provided
foo(name="Life of Brian", year=1979)
```

### Unknown keyword argument

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

def foo(**kwargs: Unpack[Movie]) -> None:
    pass

# error: [unknown-argument] "Argument `extra` does not match any known parameter"
foo(name="Life of Brian", year=1979, extra=True)
```

### Missing required fields

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

def foo(**kwargs: Unpack[Movie]) -> None:
    pass

# error: [missing-argument] "Missing required keyword argument `year` for function `foo`"
foo(name="Life of Brian")

# error: [missing-argument] "Missing required keyword arguments `name`, `year` for function `foo`"
foo()
```

### Mixed regular parameters and Unpack[TypedDict]

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

def with_prefix(prefix: str, **kwargs: Unpack[Movie]) -> None:
    pass

# Valid: regular parameter and TypedDict fields
with_prefix(">>", name="Life of Brian", year=1979)

# error: [unknown-argument] "Argument `extra` does not match any known parameter"
with_prefix(">>", name="Life of Brian", year=1979, extra=True)

# error: [missing-argument] "Missing required keyword argument `year` for function `with_prefix`"
with_prefix(">>", name="Life of Brian")
```

### NotRequired fields are optional

```py
from typing import TypedDict
from typing_extensions import Unpack, NotRequired

class MovieWithRating(TypedDict):
    name: str
    year: int
    rating: NotRequired[float]

def with_optional(**kwargs: Unpack[MovieWithRating]) -> None:
    pass

# Valid: NotRequired field can be omitted
with_optional(name="Life of Brian", year=1979)

# Valid: NotRequired field can be provided
with_optional(name="Life of Brian", year=1979, rating=9.5)

# error: [unknown-argument] "Argument `extra` does not match any known parameter"
with_optional(name="Life of Brian", year=1979, extra=True)
```

### Type checking for field types

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

def foo(**kwargs: Unpack[Movie]) -> None:
    pass

# error: [invalid-argument-type] "Argument to function `foo` is incorrect: Expected `str`, found `Literal[123]`"
foo(name=123, year=1979)

# error: [invalid-argument-type]
foo(name="Life of Brian", year="1979")
```

### Passing TypedDict variables via \*\*

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

def foo(**kwargs: Unpack[Movie]) -> None:
    pass

# Valid: Passing a compatible TypedDict via **
movie: Movie = {"name": "Life of Brian", "year": 1979}
foo(**movie)
```

### Passing incompatible TypedDict via \*\*

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

class BadMovie(TypedDict):
    name: int  # Wrong type
    year: str  # Wrong type

def foo(**kwargs: Unpack[Movie]) -> None:
    pass

bad_movie: BadMovie = {"name": 123, "year": "1979"}

# error: [invalid-argument-type]
# error: [invalid-argument-type]
foo(**bad_movie)
```

### Passing TypedDict with extra fields via \*\*

When passing a TypedDict with extra fields that are not in the parameter TypedDict, the extra fields
are reported as unknown arguments:

```py
from typing import TypedDict
from typing_extensions import Unpack

class Movie(TypedDict):
    name: str
    year: int

class MovieWithDirector(TypedDict):
    name: str
    year: int
    director: str

def foo(**kwargs: Unpack[Movie]) -> None:
    pass

movie_with_director: MovieWithDirector = {"name": "Life of Brian", "year": 1979, "director": "Terry Jones"}

# error: [unknown-argument] "Argument `director` does not match any known parameter of function `foo`"
foo(**movie_with_director)
```
