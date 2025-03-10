# Call `type[...]`

## Dynamic base

```py
from typing import Any
from knot_extensions import Unknown

def _(subclass_of_any: type[Any], subclass_of_unknown: type[Unknown]):
    reveal_type(subclass_of_any())  # revealed: Any
    reveal_type(subclass_of_unknown())  # revealed: Unknown
```
