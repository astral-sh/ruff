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
reveal_type(xs)  # revealed: ndarray[tuple[Any, ...], dtype[Any]]

xs = np.array([1.0, 2.0, 3.0], dtype=np.float64)
# TODO: should be `ndarray[tuple[Any, ...], dtype[float64]]`
reveal_type(xs)  # revealed: ndarray[tuple[Any, ...], dtype[Unknown]]
```
