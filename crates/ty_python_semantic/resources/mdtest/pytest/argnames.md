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

## Valid

This parses a series of basic strings.

```py
import pytest

# Single argname
@pytest.mark.parametrize("x", [1])
def test_single(x: int) -> None: ...
```

## Invalid

```py
import pytest

# Empty string
# @pytest.mark.parametrize("", []) # error: [pytest-invalid-argnames-literal]
def test_empty_string() -> None: ...
```
