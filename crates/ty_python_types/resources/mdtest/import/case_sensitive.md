# Case Sensitive Imports

```toml
system = "os"
```

Python's import system is case-sensitive even on case-insensitive file system. This means, importing
a module `a` should fail if the file in the search paths is named `A.py`. See
[PEP 235](https://peps.python.org/pep-0235/).

## Correct casing

Importing a module where the name matches the file name's casing should succeed.

`a.py`:

```py
class Foo:
    x: int = 1
```

```python
from a import Foo

reveal_type(Foo().x)  # revealed: int
```

## Incorrect casing

Importing a module where the name does not match the file name's casing should fail.

`A.py`:

```py
class Foo:
    x: int = 1
```

```python
# error: [unresolved-import]
from a import Foo
```

## Multiple search paths with different cased modules

The resolved module is the first matching the file name's casing but Python falls back to later
search paths if the file name's casing does not match.

```toml
[environment]
extra-paths = ["/search-1", "/search-2"]
```

`/search-1/A.py`:

```py
class Foo:
    x: int = 1
```

`/search-2/a.py`:

```py
class Bar:
    x: str = "test"
```

```python
from A import Foo
from a import Bar

reveal_type(Foo().x)  # revealed: int
reveal_type(Bar().x)  # revealed: str
```

## Intermediate segments

`db/__init__.py`:

```py
```

`db/a.py`:

```py
class Foo:
    x: int = 1
```

`correctly_cased.py`:

```python
from db.a import Foo

reveal_type(Foo().x)  # revealed: int
```

Imports where some segments are incorrectly cased should fail.

`incorrectly_cased.py`:

```python
# error: [unresolved-import]
from DB.a import Foo

# error: [unresolved-import]
from DB.A import Foo

# error: [unresolved-import]
from db.A import Foo
```

## Incorrect extension casing

The extension of imported python modules must be `.py` or `.pyi` but not `.PY` or `Py` or any
variant where some characters are uppercase.

`a.PY`:

```py
class Foo:
    x: int = 1
```

```python
# error: [unresolved-import]
from a import Foo
```
