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
