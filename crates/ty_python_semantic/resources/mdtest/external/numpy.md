# numpy

```toml
[environment]
python-version = "3.13"
python-platform = "linux"

[project]
dependencies = ["numpy==2.3.0"]
```

## Basic usage

```py
import numpy as np

xs = np.array([1, 2, 3])
# TODO: should be `ndarray[tuple[Any, ...], dtype[Any]]`
reveal_type(xs)  # revealed: ndarray[tuple[Any, ...], dtype[Unknown]]

xs = np.array([1.0, 2.0, 3.0], dtype=np.float64)
reveal_type(xs)  # revealed: ndarray[tuple[Any, ...], dtype[float64]]
```

Explicit dtypes remain distinct when checking an array against a parameter annotation. This is a
regression test for <https://github.com/astral-sh/ty/issues/3199>:

```py
def takes_float16(values: np.ndarray[tuple[int, ...], np.dtype[np.float16]]) -> None: ...

float32_values = np.array([1, 2, 3], dtype=np.float32)
reveal_type(float32_values)  # revealed: ndarray[tuple[Any, ...], dtype[floating[_32Bit]]]

float16_values = np.array([1, 2, 3], dtype=np.float16)
reveal_type(float16_values)  # revealed: ndarray[tuple[Any, ...], dtype[floating[_16Bit]]]

takes_float16(float32_values)  # error: [invalid-argument-type]
takes_float16(float16_values)
```

An explicit integer dtype is also preserved through `array`, allowing `interp` to select its array
overload. This is a regression test for <https://github.com/astral-sh/ty/issues/1429>:

```py
values = np.array([0, 1, 2], dtype=np.int64)
reveal_type(values)  # revealed: ndarray[tuple[Any, ...], dtype[signedinteger[_64Bit]]]

interpolated = np.interp(values, values, values)
reveal_type(interpolated)  # revealed: ndarray[tuple[Any, ...], dtype[float64]]
```

## Iterating over an array union

A callback that only accepts pairs is not valid for a possibly nonempty multidimensional array:
iteration yields subarrays, not tuples. Inferring the iterator element type requires substituting
the array's dtype into the nested `ndarray` result.

```py
from typing import Any
import numpy as np

def prepare(pair: tuple[int | Any, int | Any]) -> None:
    pass

def process(padding):
    if len(padding) == 0:
        padding = np.zeros((0, 2), dtype=np.int64)

    # TODO: error: [invalid-argument-type]
    return list(map(prepare, padding))
```

## Phantom property inference

An overloaded helper method can encode a type mapping for a property whose own receiver is a
protocol. Matching the nonempty-shape overload should contribute the concrete array type to the
property result. The generic fallback currently survives without that concrete arm.

```pyi
from typing import Generic, Protocol, TypeVar, overload, type_check_only
import numpy as np

type Array[Shape: tuple[int, ...], Scalar: np.generic] = np.ndarray[Shape, np.dtype[Scalar]]

FloatT_co = TypeVar("FloatT_co", bound=np.generic, covariant=True)
ShapeT_co = TypeVar("ShapeT_co", bound=tuple[int, ...], covariant=True)

@type_check_only
class HasPhantomParameter[T](Protocol):
    def phantom_parameter(self) -> T: ...

@type_check_only
class PhantomParameterMixin(Generic[ShapeT_co, FloatT_co]):
    @type_check_only
    @overload
    def phantom_parameter[ScalarT: np.generic](
        self: "PhantomParameterMixin[tuple[()], ScalarT]",
    ) -> ScalarT: ...
    @type_check_only
    @overload
    def phantom_parameter[ShapeT: tuple[int, *tuple[int, ...]], ScalarT: np.generic](
        self: "PhantomParameterMixin[ShapeT, ScalarT]",
    ) -> Array[ShapeT, ScalarT]: ...

class Distribution(Generic[FloatT_co, ShapeT_co]): ...

class Normal(
    Distribution[FloatT_co, ShapeT_co],
    PhantomParameterMixin[ShapeT_co, FloatT_co],
    Generic[ShapeT_co, FloatT_co],
):
    @property
    def value[T](self: HasPhantomParameter[T]) -> T: ...

def make_normal() -> Normal[tuple[int], np.float32]: ...

# TODO: revealed: ndarray[ShapeT@phantom_parameter, dtype[ScalarT@phantom_parameter]] | ndarray[tuple[int], dtype[floating[_32Bit]]]
# revealed: ndarray[ShapeT@phantom_parameter, dtype[ScalarT@phantom_parameter]]
reveal_type(make_normal().value)
```
