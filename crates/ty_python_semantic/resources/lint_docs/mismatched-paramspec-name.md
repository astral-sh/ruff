## What it does

Checks whether the name passed to `ParamSpec` differs from the assigned variable.

## Why is this bad?

The name argument identifies the parameter specification in a callable's signature. A mismatch can
make it harder to connect that signature to the definition.

## Examples

```python
from typing import ParamSpec

P = ParamSpec("Q")  # error: [mismatched-paramspec-name]
```
