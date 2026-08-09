# Parsing Argnames

The examples in this file are just to check that argnames are parsed correctly. The should be no
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

## Multiple Names

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

## Invalid

It is possible for the strings to have invalid names.

```py
import pytest

# Invalid identifier
@pytest.mark.parametrize("8ac", [])  # error: [pytest-invalid-argnames-literal] "`8ac` is not a valid Python identifier."
def _(): ...

# Wrong separator
@pytest.mark.parametrize("aa b", [])  # error: [pytest-invalid-argnames-literal]
def _(): ...

# Space in name
@pytest.mark.parametrize("aaa, b b, c", [])  # error: [pytest-invalid-argnames-literal]
def _(): ...

# Python keyword
@pytest.mark.parametrize("if", [])  # error: [pytest-invalid-argnames-literal]
def _(): ...

# Multiple errors
# error: [pytest-invalid-argnames-literal] "`b b` is not a valid Python identifier."
# error: [pytest-invalid-argnames-literal] "`c-d` is not a valid Python identifier."
@pytest.mark.parametrize("aaa, b b, c-d", [])
def _(): ...
```
