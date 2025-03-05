# Case Sensitive Imports

TODO: This test should use the real file system instead of the memory file system.

Python's import system is case-sensitive even on case-insensitive file system. This means, importing
a module `a` should fail if the file in the search paths is named `A.py`. See
[PEP 235](https://peps.python.org/pep-0235/).

## Correct casing

Importing a module where the name matches the file name's casing should succeed.

`a.py`:

```py
class A:
    x: int = 1
```

```python
from a import A

reveal_type(A().x)  # revealed: int
```

## Incorrect casing

Importing a module where the name does not match the file name's casing should fail.

`A.py`:

```py
class A:
    x: int = 1
```

```python
# error: [unresolved-import]
from a import A
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
class A:
    x: int = 1
```

`/search-2/a.py`:

```py
class A:
    x: str = "test"
```

```python
from a import A as ALower
from A import A as AUpper

reveal_type(AUpper().x)  # revealed: int
reveal_type(ALower().x)  # revealed: str
```

## Intermediate segments

`db/__init__.py`:

```py
```

`db/a.py`:

```py
class A:
    x: int = 1
```

`correctly_cased.py`:

```python
from db.a import A

reveal_type(A().x)  # revealed: int
```

Imports where some segments are incorrectly cased should fail.

`incorrectly_cased.py`:

```python
# error: [unresolved-import]
from DB.a import A

# error: [unresolved-import]
from DB.A import A

# error: [unresolved-import]
from db.A import A
```

## Incorrectly extension casing

The extension of imported python modules must be `.py` or `.pyi` but not `.PY` or `Py` or any
variant where some characters are uppercase.

`a.PY`:

```py
class A:
    x: int = 1
```

```python
# error: [unresolved-import]
from a import A
```
