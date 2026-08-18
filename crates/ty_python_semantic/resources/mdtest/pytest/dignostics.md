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

### Sequence of Single Values

A single argname is passed in a string and the argvalues are not a list nor tuple expression.

```py
import pytest
import itertools
from typing import Any, Iterable, Literal

@pytest.mark.parametrize("x", itertools.count())
@pytest.mark.parametrize("y", "abracadabra")
@pytest.mark.parametrize("z", dict.fromkeys([set, list, tuple]))
def test_unusual_iterables(x: float, y: str, z: type) -> None: ...
@pytest.mark.parametrize("x", range(5))  # snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize("y", b"oops")  # snapshot: pytest-param-mismatched-type
@pytest.mark.parametrize("z", dict.fromkeys([{}, (), []]))  # snapshot: pytest-param-mismatched-type
def test_wrong_type_iterables(x: Literal[2], y: str, z: type) -> None: ...
```

```snapshot
error[pytest-param-mismatched-type]: Invalid parameter passed to `test_wrong_type_iterables`.
 --> src/mdtest_snippet.py:9:31
  |
9 | @pytest.mark.parametrize("x", range(5))  # snapshot: pytest-param-mismatched-type
  |                               ^^^^^^^^ Expected `Iterable[Literal[2] | ParameterSet]`, found `range`
info: Argument is incorrect
info: This happens when testing `test_wrong_type_iterables`.
info: type `range` is not assignable to protocol `Iterable[Literal[2] | ParameterSet]`
info: └── protocol member `__iter__` is incompatible
info:     └── incompatible return types: `Iterator[int]` is not assignable to `Iterator[Literal[2] | ParameterSet]`
info:         └── protocol `Iterator[int]` is not assignable to protocol `Iterator[Literal[2] | ParameterSet]`
info:             └── protocol member `__next__` is incompatible
info:                 └── incompatible return types: `int` is not assignable to `Literal[2] | ParameterSet`


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_wrong_type_iterables`.
  --> src/mdtest_snippet.py:10:31
   |
10 | @pytest.mark.parametrize("y", b"oops")  # snapshot: pytest-param-mismatched-type
   |                               ^^^^^^^ Expected `Iterable[str | ParameterSet]`, found `Literal[b"oops"]`
info: Argument is incorrect
info: This happens when testing `test_wrong_type_iterables`.
info: type `Literal[b"oops"]` is not assignable to protocol `Iterable[str | ParameterSet]`
info: └── protocol member `__iter__` is incompatible
info:     └── incompatible return types: `Iterator[int]` is not assignable to `Iterator[str | ParameterSet]`
info:         └── protocol `Iterator[int]` is not assignable to protocol `Iterator[str | ParameterSet]`
info:             └── protocol member `__next__` is incompatible
info:                 └── incompatible return types: `int` is not assignable to `str | ParameterSet`
info:                     └── type `int` is not assignable to any element of the union `str | ParameterSet`
info:                         ├── type `int` is not assignable to any element of the union `str | ParameterSet`
info:                         │   ├── element `Literal[112]` of union `Literal[112, 115, 111]` is not assignable to `str | ParameterSet`
info:                         │   └── ... omitted 1 union element without additional context
info:                         └── ... omitted 1 union element without additional context


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_wrong_type_iterables`.
  --> src/mdtest_snippet.py:11:31
   |
11 | @pytest.mark.parametrize("z", dict.fromkeys([{}, (), []]))  # snapshot: pytest-param-mismatched-type
   |                               ^^^^^^^^^^^^^^^^^^^^^^^^^^^ Expected `Iterable[type | ParameterSet]`, found `dict[dict[Unknown, Unknown] | tuple[()] | list[Unknown], Any | None]`
info: Argument is incorrect
info: This happens when testing `test_wrong_type_iterables`.
info: type `dict[dict[Unknown, Unknown] | tuple[()] | list[Unknown], Any | None]` is not assignable to protocol `Iterable[type | ParameterSet]`
info: └── protocol member `__iter__` is incompatible
info:     └── incompatible return types: `Iterator[dict[Unknown, Unknown] | tuple[()] | list[Unknown]]` is not assignable to `Iterator[type | ParameterSet]`
info:         └── protocol `Iterator[dict[Unknown, Unknown] | tuple[()] | list[Unknown]]` is not assignable to protocol `Iterator[type | ParameterSet]`
info:             └── protocol member `__next__` is incompatible
info:                 └── incompatible return types: `dict[Unknown, Unknown] | tuple[()] | list[Unknown]` is not assignable to `type | ParameterSet`
info:                     └── element `dict[Unknown, Unknown]` of union `dict[Unknown, Unknown] | tuple[()] | list[Unknown]` is not assignable to `type | ParameterSet`
info:                         └── type `dict[Unknown, Unknown]` is not assignable to any element of the union `type | ParameterSet`
info:                             ├── element `dict[Unknown, Unknown]` of union `dict[Unknown, Unknown] | tuple[()] | list[Unknown]` is not assignable to `type | ParameterSet`
info:                             │   └── type `dict[Unknown, Unknown]` is not assignable to any element of the union `type | ParameterSet`
info:                             │       ├── element `dict[Unknown, Unknown]` of union `dict[Unknown, Unknown] | tuple[()] | list[Unknown]` is not assignable to `type | ParameterSet`
info:                             │       └── ... omitted 1 union element without additional context
info:                             └── ... omitted 1 union element without additional context
```

### Sequence of Multiple Values

Multiple argnames are passed in a string or sequence and the argvalues are not a list nor tuple
expression.

```py
import pytest
import itertools
from typing import Literal, Never

@pytest.mark.parametrize(["x", "y"], dict(a=1, b=2, c=3).items())
def test_dict_items(x: str, y: int) -> None: ...
@pytest.mark.parametrize(("x", "y"), zip([], itertools.count()))
def test_zip(x: Never, y: int) -> None: ...
@pytest.mark.parametrize(("x",), range(5))  # snapshot: pytest-param-mismatched-type
def test_range_over_tuple(x: int) -> None: ...
@pytest.mark.parametrize("x,y,z", zip(["a"], ("b",), {b"c"}))  # snapshot: pytest-param-mismatched-type
def test_invalid_zip(x: Literal["a"], y: Literal["b"], z: Literal["c"]) -> None: ...
```

```snapshot
error[pytest-param-mismatched-type]: Invalid parameter passed to `test_range_over_tuple`.
 --> src/mdtest_snippet.py:9:34
  |
9 | @pytest.mark.parametrize(("x",), range(5))  # snapshot: pytest-param-mismatched-type
  |                                  ^^^^^^^^ Expected `Iterable[tuple[int] | ParameterSet]`, found `range`
info: Argument is incorrect
info: This happens when testing `test_range_over_tuple`.
info: type `range` is not assignable to protocol `Iterable[tuple[int] | ParameterSet]`
info: └── protocol member `__iter__` is incompatible
info:     └── incompatible return types: `Iterator[int]` is not assignable to `Iterator[tuple[int] | ParameterSet]`
info:         └── protocol `Iterator[int]` is not assignable to protocol `Iterator[tuple[int] | ParameterSet]`
info:             └── protocol member `__next__` is incompatible
info:                 └── incompatible return types: `int` is not assignable to `tuple[int] | ParameterSet`


error[pytest-param-mismatched-type]: Invalid parameter passed to `test_invalid_zip`.
  --> src/mdtest_snippet.py:11:35
   |
11 | @pytest.mark.parametrize("x,y,z", zip(["a"], ("b",), {b"c"}))  # snapshot: pytest-param-mismatched-type
   |                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^ Expected `Iterable[tuple[Literal["a"], Literal["b"], Literal["c"]] | ParameterSet]`, found `zip[tuple[str, Literal["b"], bytes]]`
info: Argument is incorrect
info: This happens when testing `test_invalid_zip`.
info: type `zip[tuple[str, Literal["b"], bytes]]` is not assignable to protocol `Iterable[tuple[Literal["a"], Literal["b"], Literal["c"]] | ParameterSet]`
info: └── protocol member `__iter__` is incompatible
info:     └── incompatible return types: `zip[tuple[str, Literal["b"], bytes]]` is not assignable to `Iterator[tuple[Literal["a"], Literal["b"], Literal["c"]] | ParameterSet]`
info:         └── type `zip[tuple[str, Literal["b"], bytes]]` is not assignable to protocol `Iterator[tuple[Literal["a"], Literal["b"], Literal["c"]] | ParameterSet]`
info:             └── protocol member `__next__` is incompatible
info:                 └── incompatible return types: `tuple[str, Literal["b"], bytes]` is not assignable to `tuple[Literal["a"], Literal["b"], Literal["c"]] | ParameterSet`
info:                     └── type `tuple[str, Literal["b"], bytes]` is not assignable to any element of the union `tuple[Literal["a"], Literal["b"], Literal["c"]] | ParameterSet`
info:                         ├── the first tuple element is not compatible: `str` is not assignable to `Literal["a"]`
info:                         └── ... omitted 1 union element without additional context
```
