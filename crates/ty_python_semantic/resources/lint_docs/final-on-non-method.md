## What it does

Checks for `@final` decorators applied to non-method functions.

## Why is this bad?

The `@final` decorator is only meaningful on methods and classes.
Applying it to a module-level function or a nested function has no
effect and is likely a mistake.

## Example

```python
from typing import final


# @final is not allowed on non-method functions
@final  # error
def my_function() -> int:
    return 0
```
