# Parsing Argnames

The examples in this file are just to check that argnames are parsed correctly. There should be no
errors in any other case.

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["pytest==9.0.2"]
```

## Empty

Having no argnames is allowed:

```py
import pytest

# Empty string
@pytest.mark.parametrize("", [])
def _() -> None: ...

# Just a space
@pytest.mark.parametrize(" ", [])
def _() -> None: ...

# Noise only
@pytest.mark.parametrize(",, , , ,  ", [])
def _() -> None: ...
```

## Single Name

```py
import pytest

# Single argname
@pytest.mark.parametrize("x", [1])
def _(x: int) -> None: ...

# Different argname
@pytest.mark.parametrize("bar", [1])
def _(bar: int) -> None: ...

# Extra noise
@pytest.mark.parametrize(", foo_8,,, , ", [1])
def _(foo_8: int) -> None: ...
```

## Multiple Comma-Separated Names

Another format is to use items separated by commas. This allows many argnames simultaneously:

```py
import pytest

# Three items
@pytest.mark.parametrize("a, b_, __c", [(1, 2, 3)])
def _(a: int, b: int, c: int) -> None: ...

# Four items with extra noise
@pytest.mark.parametrize(",  aa ,b,c,    ,,,,,,,,d ", [(1, 2, 3, 4)])
def _(a: int, b: int, c: int, d: int) -> None: ...
```

## Invalid Comma-Separated Names

It is possible for the strings to have invalid names.

```py
import pytest

# Invalid identifier
@pytest.mark.parametrize("8ac", [])  # error: [pytest-invalid-argnames-literal] "`8ac` is not a valid Python identifier."
def _() -> None: ...

# Wrong separator
@pytest.mark.parametrize("aa b", [])  # error: [pytest-invalid-argnames-literal]
def _() -> None: ...

# Space in name
@pytest.mark.parametrize("aaa, b b, c", [])  # error: [pytest-invalid-argnames-literal]
def _() -> None: ...

# Python keyword
@pytest.mark.parametrize("if", [])  # error: [pytest-invalid-argnames-literal]
def _() -> None: ...

# Multiple errors
# error: [pytest-invalid-argnames-literal] "`b b` is not a valid Python identifier."
# error: [pytest-invalid-argnames-literal] "`c-d` is not a valid Python identifier."
@pytest.mark.parametrize("aaa, b b, c-d", [])
def _() -> None: ...

# Request is a reversed keyword
@pytest.mark.parametrize("request", [])  # error: [pytest-request-keyword]
def _() -> None: ...

# Request with another parameter
@pytest.mark.parametrize("request, valid", [])  # error: [pytest-request-keyword]
def _() -> None: ...
```

## Valid Sequences

Sequences are also allowed, but they must be lists or tuples to be recognized here.

```py
import pytest

# Empty sequence
@pytest.mark.parametrize([], [])
@pytest.mark.parametrize((), [])
def _() -> None: ...

# Single sequence
# This is treated differently from a single string.
@pytest.mark.parametrize(["foo"], [(1,), (2,)])
@pytest.mark.parametrize(("_bar",), [(3,), (4,)])
def _(foo: int, _bar: int) -> None: ...

# Multiple names
@pytest.mark.parametrize(["a", "b", "c"], [(1, 2, 3)])
@pytest.mark.parametrize(("d", "e"), [(4, 5)])
def _(a: int, b: int, c: int, d: int, e: int) -> None: ...
```

## Invalid Sequences

These argnames must also be valid identifiers.

```py
import pytest

# Invalid identifier
@pytest.mark.parametrize(("$3"), [])  # error: [pytest-invalid-argnames-literal] "`$3` is not a valid Python identifier."
def _() -> None: ...

# Spaces are not allowed
@pytest.mark.parametrize(["foo "], [])  # error: [pytest-invalid-argnames-literal]
def _() -> None: ...

# Not commas
@pytest.mark.parametrize(["foo,bar"], [])  # error: [pytest-invalid-argnames-literal]
def _() -> None: ...

# Keywords are invalid too
# error: [pytest-invalid-argnames-literal] "`if` is not a valid Python identifier"
# error: [pytest-invalid-argnames-literal] "`is` is not a valid Python identifier"
@pytest.mark.parametrize(("if", "it", "is"), [])
def _() -> None: ...

# Request is a keyword
@pytest.mark.parametrize(("request",), [])  # error: [pytest-request-keyword]
def _() -> None: ...

# Request is a keyword
@pytest.mark.parametrize(["request", "valid"], [])  # error: [pytest-request-keyword]
def _() -> None: ...
```

## Duplicate Argnames

Duplicating an argname will cause an error.

```py
import pytest

# Same decorator
@pytest.mark.parametrize("x, y, x", [(1, 2, 1)])  # error: [pytest-duplicate-argname]
def _(x: int, y: int) -> None: ...

# This time as a sequence
@pytest.mark.parametrize(["x", "y", "x"], [(1, 2, 1)])  # error: [pytest-duplicate-argname]
def _(x: int, y: int) -> None: ...

# Separate decorators
@pytest.mark.parametrize(["x", "y"], [(1, 2)])
@pytest.mark.parametrize(["y", "z"], [(3, 4)])  # error: [pytest-duplicate-argname]
def _(x: int, y: int, z: int) -> None: ...

# And the same with string literals
@pytest.mark.parametrize("x, y", [(1, 2)])
@pytest.mark.parametrize("", [])
@pytest.mark.parametrize("y, z", [(3, 4)])  # error: [pytest-duplicate-argname]
def _(x: int, y: int, z: int) -> None: ...

# Or multiple repeats
@pytest.mark.parametrize("x, y", [(1, 2)])
@pytest.mark.parametrize("x, z", [(3, 4)])  # error: [pytest-duplicate-argname]
# error: [pytest-duplicate-argname]
# error: [pytest-duplicate-argname]
@pytest.mark.parametrize("y, z", [(3, 4)])
def _(x: int, y: int, z: int) -> None: ...
```
