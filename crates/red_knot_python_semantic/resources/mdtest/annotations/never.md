# NoReturn & Never

## Annotation

`NoReturn` to annotate functions that never return normally. `Never` represents the bottom type, a
type that represents the empty set of Python objects. These two annotations can be used
interchangeably.

```py
from typing import NoReturn, Never, Any

def stop() -> NoReturn:
    raise RuntimeError("no way")

# revealed: Never
reveal_type(stop())

a1: NoReturn
# TODO: Test `Never` is only available in python >= 3.11
a2: Never
b1: Any
b2: int

def f():
    # revealed: Never
    reveal_type(a1)
    # revealed: Never
    reveal_type(a2)

    # Never is compatible with all types.
    v1: int = a1
    v2: str = a1
    # Other types are not compatible with Never except for Never (and Any).
    v3: Never = b1
    v4: Never = stop()
    v5: Any = b2
    # error: Object of type `Literal[1]` is not assignable to `Never`
    v6: Never = 1
```

## Typing Extensions

```py
from typing_extensions import NoReturn, Never

x: NoReturn
y: Never

def f():
    # revealed: Never
    reveal_type(x)
    # revealed: Never
    reveal_type(y)
```
