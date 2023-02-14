# deprecated-type-alias (NPY001)

Autofix is always available.

## What it does
Checks for deprecated NumPy type aliases.

## Why is this bad?
NumPy's `np.int` has long been an alias of the builtin `int`. The same
goes for `np.float`, `np.bool`, and others. These aliases exist
primarily primarily for historic reasons, and have been a cause of
frequent confusion for newcomers.

These aliases were been deprecated in 1.20, and removed in 1.24.

## Examples
```python
import numpy as np

np.bool
```

Use instead:
```python
bool
```