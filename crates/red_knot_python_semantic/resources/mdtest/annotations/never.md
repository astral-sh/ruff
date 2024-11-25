# NoReturn & Never

`NoReturn` is used to annotate the return type for functions that never return. `Never` is the
bottom type, representing the empty set of Python objects. These two annotations can be used
interchangeably.

## Function Return Type Annotation

```py
from typing import NoReturn

def stop() -> NoReturn:
    raise RuntimeError("no way")

# revealed: Never
reveal_type(stop())
```

## Assignment

```py
from typing import NoReturn, Never, Any

# error: [invalid-type-parameter] "Type `typing.Never` expected no type parameter"
x: Never[int]
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

    # Never is assignable to all types.
    v1: int = a1
    v2: str = a1
    # Other types are not assignable to Never except for Never (and Any).
    v3: Never = b1
    v4: Never = a2
    v5: Any = b2
    # error: [invalid-assignment] "Object of type `Literal[1]` is not assignable to `Never`"
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
