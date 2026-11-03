## What it does

Checks whether the name passed to `TypeVarTuple` differs from the assigned variable.

## Why is this bad?

The name argument identifies the variadic type variable in generic signatures. A mismatch makes it
harder to trace a signature back to the variable's definition.

## Examples

```python
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Us")  # error: [mismatched-typevartuple-name]
```
