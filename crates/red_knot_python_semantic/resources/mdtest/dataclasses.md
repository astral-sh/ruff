# Dataclasses

## Basic

```py
from dataclasses import dataclass

@dataclass
class IntAndStr:
    x: int
    y: str

IntAndStr(1, "a")  # OK

# TODO: should be an error
IntAndStr()

# TODO: should be an error
IntAndStr(1)

# TODO: should be an error
IntAndStr(1, "a", None)

# TODO: should be an error
IntAndStr("a", 1)
```
