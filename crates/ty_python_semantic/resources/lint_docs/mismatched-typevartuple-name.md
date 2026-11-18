## What it does

Checks whether the name passed to `TypeVarTuple` differs from the assigned variable.

## Why is this bad?

A mismatched name can be a typo and can make a `TypeVarTuple`'s runtime representation confusing.

## Examples

```python
from typing_extensions import TypeVarTuple

Ts = TypeVarTuple("Us")  # error: [mismatched-typevartuple-name] "Expected "Ts", got "Us""
```
