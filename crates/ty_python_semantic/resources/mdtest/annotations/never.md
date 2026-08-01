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
from typing_extensions import NoReturn, Never, Any

# error: [invalid-type-form] "Type `typing.Never` expected no type parameter"
invalid: Never[int]

def _(never: Never):
    # revealed: Never
    reveal_type(never)

def _(noreturn: NoReturn):
    # revealed: Never
    reveal_type(noreturn)

# Never is assignable to all types:
def _(never: Never):
    v1: int = never
    v2: str = never
    v3: Never = never
    v4: Any = never

# No type is assignable to Never except for Never (and Any):
def _(never: Never, noreturn: NoReturn, any: Any):
    v1: Never = 1  # error: [invalid-assignment]
    v2: Never = "a"  # error: [invalid-assignment]

    v3: Never = any
    v4: Never = noreturn
    v4: NoReturn = never
```

## `typing.Never`

`typing.Never` is only available in Python 3.11 and later.

### Python 3.11

```toml
[environment]
python-version = "3.11"
```

```py
from typing import Never

reveal_type(Never)  # revealed: <special-form 'typing.Never'>
```

### Python 3.10

```toml
[environment]
python-version = "3.10"
```

```py
# error: [unresolved-import]
from typing import Never
```
