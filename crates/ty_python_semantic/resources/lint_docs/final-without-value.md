## What it does
Checks for `Final` symbols that are declared without a value and are never
assigned a value in their scope.

## Why is this bad?
A `Final` symbol must be initialized with a value at the time of declaration
or in a subsequent assignment. At module or function scope, the assignment must
occur in the same scope. In a class body, the assignment may occur in `__init__`.

## Examples
```python
from typing import Final

# Error: `Final` symbol without a value
MY_CONSTANT: Final[int]

# OK: `Final` symbol with a value
MY_CONSTANT: Final[int] = 1
```
