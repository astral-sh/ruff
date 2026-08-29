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

```py
import pytest

@pytest.mark.parametrize("x y", [])  # snapshot: pytest-invalid-argnames-literal
def test_space_instead_of_comma(x: int, y: bool) -> None: ...
@pytest.mark.parametrize(["x", " y "], [])  # snapshot: pytest-invalid-argnames-literal
def test_invalid_name_in_sequence(x: int, y: bool) -> None: ...
@pytest.mark.parametrize(("x,y",), [])  # snapshot: pytest-invalid-argnames-literal
def test_invalid_name_in_sequence(x: int, y: bool) -> None: ...
@pytest.mark.parametrize(argvalues=[], argnames="oops!")  # snapshot: pytest-invalid-argnames-literal
def test_flipped_argnames_argvalues(x: int, y: bool) -> None: ...
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


error[pytest-invalid-argnames-literal]: `oops!` is not a valid Python identifier.
 --> src/mdtest_snippet.py:9:49
  |
9 | @pytest.mark.parametrize(argvalues=[], argnames="oops!")  # snapshot: pytest-invalid-argnames-literal
  |                                                 ^^^^^^^
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

## Duplicate Argnames

```py
import pytest
from typing import Any

@pytest.mark.parametrize("x, y", [(1.0, 1)])
# snapshot: pytest-duplicate-argname
# snapshot: pytest-duplicate-argname
@pytest.mark.parametrize("x, y", [(2.0, 2)])
def test_duplicate_argnames_csv(x: float, y: int) -> None: ...
@pytest.mark.parametrize(["x", "y"], [(1.0, 1)])
# snapshot: pytest-duplicate-argname
@pytest.mark.parametrize(("x", "z"), [(2.0, "2")])
# snapshot: pytest-duplicate-argname
# snapshot: pytest-duplicate-argname
@pytest.mark.parametrize(("y", "z"), [(3, "3")])
def test_duplicate_argnames_sequence(x: float, y: int, z: str) -> None: ...
@pytest.mark.parametrize("x", [])
# snapshot: pytest-duplicate-argname
@pytest.mark.parametrize("x", [])
# snapshot: pytest-duplicate-argname
@pytest.mark.parametrize("x", [])
def test_duplicate_argnames_many_repeats(x: Any) -> None: ...
```

```snapshot
error[pytest-duplicate-argname]: Duplicate argname `x`
 --> src/mdtest_snippet.py:7:26
  |
7 | @pytest.mark.parametrize("x, y", [(2.0, 2)])
  |                          ^^^^^^
info: `x` already used here
 --> src/mdtest_snippet.py:4:26
  |
4 | @pytest.mark.parametrize("x, y", [(1.0, 1)])
  |                          ^^^^^^


error[pytest-duplicate-argname]: Duplicate argname `y`
 --> src/mdtest_snippet.py:7:26
  |
7 | @pytest.mark.parametrize("x, y", [(2.0, 2)])
  |                          ^^^^^^
info: `y` already used here
 --> src/mdtest_snippet.py:4:26
  |
4 | @pytest.mark.parametrize("x, y", [(1.0, 1)])
  |                          ^^^^^^


error[pytest-duplicate-argname]: Duplicate argname `x`
  --> src/mdtest_snippet.py:11:27
   |
11 | @pytest.mark.parametrize(("x", "z"), [(2.0, "2")])
   |                           ^^^
info: `x` already used here
 --> src/mdtest_snippet.py:9:27
  |
9 | @pytest.mark.parametrize(["x", "y"], [(1.0, 1)])
  |                           ^^^


error[pytest-duplicate-argname]: Duplicate argname `y`
  --> src/mdtest_snippet.py:14:27
   |
14 | @pytest.mark.parametrize(("y", "z"), [(3, "3")])
   |                           ^^^
info: `y` already used here
 --> src/mdtest_snippet.py:9:32
  |
9 | @pytest.mark.parametrize(["x", "y"], [(1.0, 1)])
  |                                ^^^


error[pytest-duplicate-argname]: Duplicate argname `z`
  --> src/mdtest_snippet.py:14:32
   |
14 | @pytest.mark.parametrize(("y", "z"), [(3, "3")])
   |                                ^^^
info: `z` already used here
  --> src/mdtest_snippet.py:11:32
   |
11 | @pytest.mark.parametrize(("x", "z"), [(2.0, "2")])
   |                                ^^^


error[pytest-duplicate-argname]: Duplicate argname `x`
  --> src/mdtest_snippet.py:18:26
   |
18 | @pytest.mark.parametrize("x", [])
   |                          ^^^
info: `x` already used here
  --> src/mdtest_snippet.py:16:26
   |
16 | @pytest.mark.parametrize("x", [])
   |                          ^^^


error[pytest-duplicate-argname]: Duplicate argname `x`
  --> src/mdtest_snippet.py:20:26
   |
20 | @pytest.mark.parametrize("x", [])
   |                          ^^^
info: `x` already used here
  --> src/mdtest_snippet.py:16:26
   |
16 | @pytest.mark.parametrize("x", [])
   |                          ^^^
```
