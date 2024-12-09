# Invalid syntax

## Missing module name

```py
from import bar  # error: [invalid-syntax]

reveal_type(bar)  # revealed: Unknown
```
