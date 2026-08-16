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

## Empty

Only the empty tuple is accepted.

```py
import pytest
from collections import namedtuple

# Different kinds of empty tuples.
@pytest.mark.parametrize("", [(), (), namedtuple("Named", [])()])
def _() -> None: ...

# An empty list is also allowed.
@pytest.mark.parametrize("", [])
def _() -> None: ...

# And the argnames can also be a sequence.
@pytest.mark.parametrize([], [()])
def _() -> None: ...

# Other values are rejected.
# error: [pytest-param-mismatched-type]
# error: [pytest-param-mismatched-type]
# error: [pytest-param-mismatched-type]
@pytest.mark.parametrize("", [1, (2,), [3.0]])
def _() -> None: ...
```

## Multiple Argnames

The argvalues must be in a tuple when there are multiple argnames.

```py
import pytest
from collections import namedtuple

# All valid.
@pytest.mark.parametrize("x, y", [(1, "2"), (3, "4")])
def _(x: int, y: str) -> None: ...

# Even though other sequences may be correct, they are not accepted.
@pytest.mark.parametrize(
    ["x", "y"],
    [
        [1, "2"],  # error: [pytest-param-mismatched-type]
        {3, "4"},  # error: [pytest-param-mismatched-type]
        ("5", 6),  # error: [pytest-param-mismatched-type]
        None,  # error: [pytest-param-mismatched-type]
    ],
)
def _(x: int, y: str) -> None: ...

# The number of arguments needs to be correct too.
@pytest.mark.parametrize(
    ("x", "y", "z"),
    [
        (),  # error: [pytest-param-mismatched-type]
        (1,),  # error: [pytest-param-mismatched-type]
        (1, "2"),  # error: [pytest-param-mismatched-type]
        (1, "2", True),
        (1, "2", True, 4),  # error: [pytest-param-mismatched-type]
    ],
)
def _(x: int, y: str, z: bool) -> None: ...

# This is a special case where a single tuple is expected, not a single value.
@pytest.mark.parametrize(
    ("x",),
    [
        (),  # error: [pytest-param-mismatched-type]
        (1,),
        1,  # error: [pytest-param-mismatched-type]
        (1, 2),  # error: [pytest-param-mismatched-type]
        (lambda: (1,))(),
    ],
)
def _(x: int) -> None: ...
```
