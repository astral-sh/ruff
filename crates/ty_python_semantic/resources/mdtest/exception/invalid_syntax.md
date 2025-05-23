# Exception Handling

## Invalid syntax

```py
from typing_extensions import reveal_type

try:
    print
except as e:  # error: [invalid-syntax]
    reveal_type(e)  # revealed: Unknown
```
