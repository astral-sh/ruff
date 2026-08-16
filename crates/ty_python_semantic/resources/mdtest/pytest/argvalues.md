# Checking Arguments

The examples in this file check that the argvalues are correct. The preprocessing should all be done
correctly.

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["pytest==9.0.2"]
```

## Single Argname

When there is a single argname, each item must have that type.

```py
import pytest

# All valid
@pytest.mark.parametrize("x", [1, 2, 3, 4])
def _(x: int) -> None: ...

# No argnames
@pytest.mark.parametrize("x", [])
def _(x: int) -> None: ...

# All invalid
@pytest.mark.parametrize(
    "x",
    [
        1.0,  # error: [pytest-param-mismatched-type]
        "3",  # error: [pytest-param-mismatched-type]
    ],
)
def _(x: int) -> None: ...

# Mix of valid and invalid
@pytest.mark.parametrize(
    "y",
    (
        1.0,  # error: [pytest-param-mismatched-type]
        "2",
        3,  # error: [pytest-param-mismatched-type]
        "4.0",
    ),
)
def _(x: int, y: str) -> None: ...
```
