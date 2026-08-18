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

## Request Keyword

`request` is a keyword in `pytest`.

```py
import pytest
from typing import Any

@pytest.mark.parametrize("request, x", [(None, 1), (None, 2)])  # snapshot: pytest-request-keyword
def test_request_keyword_csv(x: int, request: Any) -> None: ...
@pytest.mark.parametrize(("request", "x"), [(None, 1), (None, 2)])  # snapshot: pytest-request-keyword
def test_request_keyword_tuple(x: int, request: Any) -> None: ...
```

```snapshot
error[pytest-request-keyword]: `request` is a reserved Pytest keyword and cannot be used during parametrization.
 --> src/mdtest_snippet.py:4:26
  |
4 | @pytest.mark.parametrize("request, x", [(None, 1), (None, 2)])  # snapshot: pytest-request-keyword
  |                          ^^^^^^^^^^^^


error[pytest-request-keyword]: `request` is a reserved Pytest keyword and cannot be used during parametrization.
 --> src/mdtest_snippet.py:6:27
  |
6 | @pytest.mark.parametrize(("request", "x"), [(None, 1), (None, 2)])  # snapshot: pytest-request-keyword
  |                           ^^^^^^^^^
```
