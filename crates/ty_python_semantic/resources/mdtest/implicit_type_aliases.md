# Implicit type aliases

Implicit type aliases are the earliest form of type alias, introduced in PEP 484. They have no
special marker, just an ordinary assignment statement.

## Basic

We support simple type aliases with no extra effort, when the "value type" of the RHS is still a
valid type for use in a type expression:

```py
MyInt = int

def f(x: MyInt):
    reveal_type(x)  # revealed: int

f(1)
```

## Recursive

### Old union syntax

```py
from typing import Union

T = list[Union["T", None]]
```

### New union syntax

```toml
[environment]
python-version = "3.12"
```

```py
T = list["T" | None]
```
