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
```

## Signature

Pytest ignores optional parameterss and parameters must have the correct type.

```py
import pytest
from typing import Any

@pytest.mark.parametrize("", [])
# snapshot: pytest-test-parameter-wrong-kind
# snapshot: pytest-test-parameter-wrong-kind
def test_invalid_variadic_parameters(*args, **kwargs) -> None: ...
@pytest.mark.parametrize("", [])
# snapshot: pytest-test-parameter-wrong-kind
# snapshot: pytest-test-parameter-wrong-kind
def test_invalid_positional_only_parameters(x: int, y: float, /) -> None: ...
@pytest.mark.parametrize("x", [1, 2, 3])
@pytest.mark.parametrize("y", [None, None])
# snapshot: pytest-test-optional-parameter
def test_invalid_optional_parameter(x: int, *, y: None = None) -> None: ...
```

```snapshot
warning[pytest-test-parameter-wrong-kind]: Pytest tests only accept keyword arguments. `*args` is a variadic positional argument.
 --> src/mdtest_snippet.py:7:38
  |
7 | def test_invalid_variadic_parameters(*args, **kwargs) -> None: ...
  |                                      ^^^^^


warning[pytest-test-parameter-wrong-kind]: Pytest tests only accept keyword arguments. `**kwargs` is a variadic keyword argument.
 --> src/mdtest_snippet.py:7:45
  |
7 | def test_invalid_variadic_parameters(*args, **kwargs) -> None: ...
  |                                             ^^^^^^^^


warning[pytest-test-parameter-wrong-kind]: Pytest tests only accept keyword arguments. `x` is a positional only argument.
  --> src/mdtest_snippet.py:11:45
   |
11 | def test_invalid_positional_only_parameters(x: int, y: float, /) -> None: ...
   |                                             ^^^^^^


warning[pytest-test-parameter-wrong-kind]: Pytest tests only accept keyword arguments. `y` is a positional only argument.
  --> src/mdtest_snippet.py:11:53
   |
11 | def test_invalid_positional_only_parameters(x: int, y: float, /) -> None: ...
   |                                                     ^^^^^^^^


warning[pytest-test-optional-parameter]: Pytest tests ignore optional arguments. `y` has a default value.
  --> src/mdtest_snippet.py:15:48
   |
15 | def test_invalid_optional_parameter(x: int, *, y: None = None) -> None: ...
   |                                                ^^^^^^^^^^^^^^
```

## Argvalues with Incorrect Types

There are different cases to consider, each of which is described and listed below.

### Single Argvalue

A single argname is passed in a string and the argvalues are a list or tuple expression.

```py
import pytest

# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize("x", [1, 2, 3.5, (None,)])
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize("y", (1.0, None, "3", pytest.param(4)))
def test_single_argname(x: int, y: float) -> None: ...
```

```snapshot
error[pytest-param-mismatched-type]: Invalid parameter passed to `test_single_argname`.
 --> src/mdtest_snippet.py:5:38
  |
5 | @pytest.mark.parametrize("x", [1, 2, 3.5, (None,)])
  |                                      ^^^ Expected `int | ParameterSet`, found `float*`
info: Argument is incorrect
info: This happens when testing `test_single_argname`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_single_argname`.
 --> src/mdtest_snippet.py:5:43
  |
5 | @pytest.mark.parametrize("x", [1, 2, 3.5, (None,)])
  |                                           ^^^^^^^ Expected `int | ParameterSet`, found `tuple[None]`
info: Argument is incorrect
info: This happens when testing `test_single_argname`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_single_argname`.
 --> src/mdtest_snippet.py:8:37
  |
8 | @pytest.mark.parametrize("y", (1.0, None, "3", pytest.param(4)))
  |                                     ^^^^ Expected `float | ParameterSet`, found `None`
info: Argument is incorrect
info: This happens when testing `test_single_argname`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_single_argname`.
 --> src/mdtest_snippet.py:8:43
  |
8 | @pytest.mark.parametrize("y", (1.0, None, "3", pytest.param(4)))
  |                                           ^^^ Expected `float | ParameterSet`, found `Literal["3"]`
info: Argument is incorrect
info: This happens when testing `test_single_argname`.
```

### Multiple Argvalues as Tuple

Multiple argnames are passed in a string or sequence and the argvalues are a list or tuple of tuple
expressions.

```py
import pytest

# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize(("x",), ((1,), (2, 3), (4.0)))
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize("y, z", [(), (2.0, 3), (None, "a"), (b"", b"")])
def test_multiple_argname_tuple(x: int, y: float, z: str) -> None: ...

# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize(["x", "y", "z"], [(), (1, 2, str(3)), (None, None, None, None)])
def test_multiple_argname_tuple(x: int, y: float, z: str) -> None: ...
```

```snapshot
error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
 --> src/mdtest_snippet.py:5:45
  |
5 | @pytest.mark.parametrize(("x",), ((1,), (2, 3), (4.0)))
  |                                             ^ Too many positional arguments: expected 1, got 2
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
 --> src/mdtest_snippet.py:5:50
  |
5 | @pytest.mark.parametrize(("x",), ((1,), (2, 3), (4.0)))
  |                                                  ^^^ Expected `tuple[int] | ParameterSet`, found `float*`
info: Argument is incorrect
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:11:35
   |
11 | @pytest.mark.parametrize("y, z", [(), (2.0, 3), (None, "a"), (b"", b"")])
   |                                   ^^ No arguments provided for required parameters `y`, `z`
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:11:45
   |
11 | @pytest.mark.parametrize("y, z", [(), (2.0, 3), (None, "a"), (b"", b"")])
   |                                             ^ Expected `str`, found `Literal[3]`
info: Argument is incorrect
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:11:50
   |
11 | @pytest.mark.parametrize("y, z", [(), (2.0, 3), (None, "a"), (b"", b"")])
   |                                                  ^^^^ Expected `float`, found `None`
info: Argument is incorrect
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:11:63
   |
11 | @pytest.mark.parametrize("y, z", [(), (2.0, 3), (None, "a"), (b"", b"")])
   |                                                               ^^^ Expected `float`, found `Literal[b""]`
info: Argument is incorrect
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:11:68
   |
11 | @pytest.mark.parametrize("y, z", [(), (2.0, 3), (None, "a"), (b"", b"")])
   |                                                                    ^^^ Expected `str`, found `Literal[b""]`
info: Argument is incorrect
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:19:44
   |
19 | @pytest.mark.parametrize(["x", "y", "z"], [(), (1, 2, str(3)), (None, None, None, None)])
   |                                            ^^ No arguments provided for required parameters `x`, `y`, `z`
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:19:65
   |
19 | @pytest.mark.parametrize(["x", "y", "z"], [(), (1, 2, str(3)), (None, None, None, None)])
   |                                                                 ^^^^ Expected `int`, found `None`
info: Argument is incorrect
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:19:71
   |
19 | @pytest.mark.parametrize(["x", "y", "z"], [(), (1, 2, str(3)), (None, None, None, None)])
   |                                                                       ^^^^ Expected `float`, found `None`
info: Argument is incorrect
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:19:77
   |
19 | @pytest.mark.parametrize(["x", "y", "z"], [(), (1, 2, str(3)), (None, None, None, None)])
   |                                                                             ^^^^ Expected `str`, found `None`
info: Argument is incorrect
info: This happens when testing `test_multiple_argname_tuple`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_multiple_argname_tuple`.
  --> src/mdtest_snippet.py:19:83
   |
19 | @pytest.mark.parametrize(["x", "y", "z"], [(), (1, 2, str(3)), (None, None, None, None)])
   |                                                                                   ^^^^ Too many positional arguments: expected 3, got 4
info: This happens when testing `test_multiple_argname_tuple`.
```

### Multiple Argvalues as Non-Tuple

Multiple argnames are passed in a string or sequence and the argvalues are a list or tuple of
non-tuple expressions. The first case with a single argname is a special case of this.

```py
import pytest
from typing import Literal

test_case_1 = (1, "y")
test_case_2 = (2.0, "y")
test_case_3 = pytest.param(None, marks=pytest.mark.skip)
test_case_4 = ()

# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize("x, y", [test_case_1, test_case_2, test_case_3, test_case_4])
def test_variables(x: int, y: str) -> None: ...

# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
# snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize(("x", "y"), [None, [1, "y"], pytest.param(None), {1, "y"}])
def test_non_tuples(x: int, y: Literal["y"]) -> None: ...
```

```snapshot
error[pytest-param-mismatched-type]: Invalid parameter passed to `test_variables`.
  --> src/mdtest_snippet.py:11:48
   |
11 | @pytest.mark.parametrize("x, y", [test_case_1, test_case_2, test_case_3, test_case_4])
   |                                                ^^^^^^^^^^^ Expected `tuple[int, str] | ParameterSet`, found `tuple[float*, Literal["y"]]`
info: Argument is incorrect
info: This happens when testing `test_variables`.
info: type `tuple[float*, Literal["y"]]` is not assignable to any element of the union `tuple[int, str] | ParameterSet`
info: ├── the first tuple element is not compatible: `float*` is not assignable to `int`
info: └── ... omitted 1 union element without additional context


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_variables`.
  --> src/mdtest_snippet.py:11:74
   |
11 | @pytest.mark.parametrize("x, y", [test_case_1, test_case_2, test_case_3, test_case_4])
   |                                                                          ^^^^^^^^^^^ Expected `tuple[int, str] | ParameterSet`, found `tuple[()]`
info: Argument is incorrect
info: This happens when testing `test_variables`.
info: type `tuple[()]` is not assignable to any element of the union `tuple[int, str] | ParameterSet`
info: ├── a tuple of length 0 is not assignable to a tuple of length 2
info: └── ... omitted 1 union element without additional context


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_non_tuples`.
  --> src/mdtest_snippet.py:17:39
   |
17 | @pytest.mark.parametrize(("x", "y"), [None, [1, "y"], pytest.param(None), {1, "y"}])
   |                                       ^^^^ Expected `tuple[int, Literal["y"]] | ParameterSet`, found `None`
info: Argument is incorrect
info: This happens when testing `test_non_tuples`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_non_tuples`.
  --> src/mdtest_snippet.py:17:45
   |
17 | @pytest.mark.parametrize(("x", "y"), [None, [1, "y"], pytest.param(None), {1, "y"}])
   |                                             ^^^^^^^^ Expected `tuple[int, Literal["y"]] | ParameterSet`, found `list[int | str]`
info: Argument is incorrect
info: This happens when testing `test_non_tuples`.


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_non_tuples`.
  --> src/mdtest_snippet.py:17:75
   |
17 | @pytest.mark.parametrize(("x", "y"), [None, [1, "y"], pytest.param(None), {1, "y"}])
   |                                                                           ^^^^^^^^ Expected `tuple[int, Literal["y"]] | ParameterSet`, found `set[int | str]`
info: Argument is incorrect
info: This happens when testing `test_non_tuples`.
```
