# Pytest Diagnostics

Tests in this file are for checking the message diagnostics. They encorporate all the errors
described in the other files.

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["pytest==9.0.2"]
```

## Invalid Argnames

The argnames are incorrect to these test functions.

```py
import pytest

@pytest.mark.parametrize("x y", [])  # snapshot: pytest-invalid-argnames-literal
def test_space_instead_of_comma(x: int, y: bool) -> None: ...
@pytest.mark.parametrize(["x", " y "], [])  # snapshot: pytest-invalid-argnames-literal
def test_invalid_name_in_sequence(x: int, y: bool) -> None: ...
@pytest.mark.parametrize(("x,y",), [])  # snapshot: pytest-invalid-argnames-literal
def test_invalid_name_in_sequence(x: int, y: bool) -> None: ...
```

```snapshot
error[pytest-invalid-argnames-literal]: `x y` is not a valid Python identifier.
 --> src/mdtest_snippet.py:3:26
  |
3 | @pytest.mark.parametrize("x y", [])  # snapshot: pytest-invalid-argnames-literal
  |                          ^^^^^


error[pytest-invalid-argnames-literal]: ` y ` is not a valid Python identifier.
 --> src/mdtest_snippet.py:5:32
  |
5 | @pytest.mark.parametrize(["x", " y "], [])  # snapshot: pytest-invalid-argnames-literal
  |                                ^^^^^


error[pytest-invalid-argnames-literal]: `x,y` is not a valid Python identifier.
 --> src/mdtest_snippet.py:7:27
  |
7 | @pytest.mark.parametrize(("x,y",), [])  # snapshot: pytest-invalid-argnames-literal
  |                           ^^^^^
```
