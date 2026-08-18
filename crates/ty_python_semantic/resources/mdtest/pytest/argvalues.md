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

# All valid.
@pytest.mark.parametrize("x", [1, 2, 3, 4])
def _(x: int) -> None: ...

# No argnames.
@pytest.mark.parametrize("x", [])
def _(x: int) -> None: ...

# All invalid.
@pytest.mark.parametrize(
    "x",
    [
        1.0,  # error: [pytest-param-mismatched-type]
        "3",  # error: [pytest-param-mismatched-type]
    ],
)
def _(x: int) -> None: ...

# Mix of valid and invalid.
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

# The single-item tuple is a special case.
# But it only applies when the argnames are not a string.
@pytest.mark.parametrize(
    "x",
    (
        1,  # error: [pytest-param-mismatched-type]
        2,  # error: [pytest-param-mismatched-type]
        (3,),
        ((4,),),  # error: [pytest-param-mismatched-type]
    ),
)
def _(x: tuple[int]) -> None: ...
```

## Empty

Only the empty tuple is accepted.

```py
import pytest
from collections import namedtuple

# Different kinds of empty tuples.
@pytest.mark.parametrize("", [(), (), namedtuple("Named", [])(), tuple()])
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

# All valid.
@pytest.mark.parametrize("x, y", [(1, "2"), (3, "4")])
def _(x: int, y: str) -> None: ...

# Even though other sequences may be correct, they are not accepted.
@pytest.mark.parametrize(
    ["x", "y"],
    [
        [1, "2"],  # error: [pytest-param-mismatched-type]
        {3, "4"},  # error: [pytest-param-mismatched-type]
        None,  # error: [pytest-param-mismatched-type]
    ],
)
def _(x: int, y: str) -> None: ...

# When the argvalues are passed as a tuple, there is potential for multiple errors.
# The error is for each individual argument.

@pytest.mark.parametrize(
    ("x", "y", "z"),
    [
        (1, "2", True),
        # error: [pytest-param-mismatched-type]
        (1, "2", None),
        # error: [pytest-param-mismatched-type]
        (1, None, True),
        # error: [pytest-param-mismatched-type]
        (None, "2", True),
        # error: [pytest-param-mismatched-type]
        # error: [pytest-param-mismatched-type]
        (1, None, None),
        # error: [pytest-param-mismatched-type]
        # error: [pytest-param-mismatched-type]
        (None, "2", None),
        # error: [pytest-param-mismatched-type]
        # error: [pytest-param-mismatched-type]
        (None, None, True),
        # error: [pytest-param-mismatched-type]
        # error: [pytest-param-mismatched-type]
        # error: [pytest-param-mismatched-type]
        (None, None, None),
    ],
)
def _(x: int, y: str, z: bool) -> None: ...

# The number of arguments needs to be correct too.
valid_value = (0, "0.0", False)

@pytest.mark.parametrize(
    ("x", "y", "z"),
    [
        (),  # error: [pytest-param-mismatched-type]
        (1,),  # error: [pytest-param-mismatched-type]
        (1, "2"),  # error: [pytest-param-mismatched-type]
        (1, "2", True),
        valid_value,
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

## Sequences

In each of the previous examples, every test case is checked. However, sometimes we just check the
sequence.

```py
import pytest

# We can't check every item in the range, just the sequence as a whole.
@pytest.mark.parametrize("x", range(5))
def _(x: int) -> None: ...

# The same with a broken list.
@pytest.mark.parametrize(
    "x",
    [1, 2] + [3.0],  # error: [pytest-param-mismatched-type]
)
def _(x: int) -> None: ...

# There are other ways to "hide" the whole sequence.
test_cases = [(1, "1"), (2, b"2")]

@pytest.mark.parametrize(
    "x, s",
    test_cases,  # error: [pytest-param-mismatched-type]
)
def _(x: int, s: str) -> None: ...

# The single-item list is still an edge case.
test_cases = range(3)

@pytest.mark.parametrize(
    ["x"],
    test_cases,  # error: [pytest-param-mismatched-type]
)
def _(x: int) -> None: ...

# And while this might be a bug, it still type checks.
@pytest.mark.parametrize("s", "test-value")
def _(s: str) -> None: ...
```

## Unchecked Cases

In the current setup, there are cases where tests do not get checked. These are deliberately
incorrect, but on errors are changed. If you update the `pytest` implementation, this may change.

```py
import pytest
from typing import cast, overload, Any, Iterable

# This function generates no errors either.
def identity[T](x: T, /) -> T:
    return x

# If there are other decorators, nothing gets checked.
@identity
@pytest.mark.parametrize("x", ["oops"])
def _(x: str) -> None: ...

# Incorrect argument names are not checked.
# They may be requested by fixtures, and this is not currently implemented.
@pytest.mark.parametrize("y", ["oops"])
def _(x: int) -> None: ...

# Extra arguments to `pytest.mark.parametrize` also prevents checking.
# This is overly strict for now, as you may just be adding `ids`.
# But prevents issues with indirect/scoped parametrizations.
@pytest.mark.parametrize("x", ["oops"], ids=["test"])
@pytest.mark.parametrize("y", [lambda x: x + x], indirect=True)
def _(x: int, y: str) -> None: ...

# If you use args or kwargs, it's also too difficult to check.
# In this case, you get an error that's not related to the pytest-specific checking.
args = ("x", [1, 2, 3])
kwargs = dict(argnames="y", argvalues=["a", "b", "c"])

@pytest.mark.parametrize(*args)
@pytest.mark.parametrize(**kwargs)  # error: [invalid-argument-type]
def _(x: int, y: str) -> None: ...

# If the argnames are a string literal, the types can be checked.
# But otherwise, it's not possible.
readable_argnames = "x, y"
unreadable_argnames = cast(str, "x, y")

@pytest.mark.parametrize(readable_argnames, [None])  # error: [pytest-param-mismatched-type]
@pytest.mark.parametrize(unreadable_argnames, [None])
def _(x: int, y: str) -> None: ...

# Fixtures are also only checked in place.
x_range = pytest.mark.parametrize("x", range(5))

@x_range
def _(x: bool) -> None: ...

# When a `pytest.Parameter` is used, it is ignored.
@pytest.mark.parametrize("x", [pytest.param(1), pytest.param("4"), pytest.param(None, id="skipped-test")])
@pytest.mark.parametrize(("y",), [pytest.param(2, marks=pytest.mark.xfail), pytest.param(1, 2, 3)])
def _(x: int, y: str) -> None: ...

# Interspersed values are checked.
bool_params = [True, pytest.param(False)]

@pytest.mark.parametrize(
    "x",
    [
        pytest.param(1),
        "4",  # error: [pytest-param-mismatched-type]
        pytest.param(None, marks=[pytest.mark.skip]),
    ],
)
@pytest.mark.parametrize(
    ("y",),
    [
        pytest.param(2, marks=pytest.mark.xfail),
        pytest.param(1, 2, 3),
        ("2", "3"),  # error: [pytest-param-mismatched-type]
    ],
)
@pytest.mark.parametrize("z", bool_params)
def _(x: int, y: str, z: bool) -> None: ...

# Request is a reserved word in Pytest.
# Therefore, it is disallowed as an argname.
# As an argvalue, it always has the type `_pytest.fixtures.FixtureRequest` (but this is not checked).
@pytest.mark.parametrize(
    "request",  # error: [pytest-request-keyword]
    [None],
)
def _(request: tuple[()]) -> None: ...

# Overloaded functions are not ignored (it's easier to include them).
# The additional decorator mean that only the final version is checked.
@overload
def overloaded_test(x: None) -> None: ...
@overload
def overloaded_test(x: int) -> None:  # error: [invalid-overload]
    ...
@pytest.mark.parametrize("x", ["a", None, 3])  # error: [pytest-param-mismatched-type]
def overloaded_test(x: str | None) -> None: ...

# Optional arguments are ignored (but generate a separate warning).
# Other kinds of arguments are also ignored.
@pytest.mark.parametrize("x", ["1"])
@pytest.mark.parametrize("y", ["2"])  # error: [pytest-param-mismatched-type]
@pytest.mark.parametrize("z", ["3"])  # error: [pytest-param-mismatched-type]
@pytest.mark.parametrize("optional", ["4"])
# error: [pytest-test-parameter-wrong-kind]
# error: [pytest-test-optional-parameter]
# error: [pytest-test-parameter-wrong-kind]
def _(x: int, /, y: int, *, z: int, optional=None, **kwargs) -> None: ...

# Type variables are ignored, and treated as `object`.
@pytest.mark.parametrize("x", [[], None])
def _[T: Iterable[Any]](x: T) -> None: ...
```
