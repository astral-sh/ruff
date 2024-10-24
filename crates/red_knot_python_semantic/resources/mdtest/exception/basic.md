# Exception Handling

## Single Exception

```py
import re

try:
    x
except NameError as e:
    reveal_type(e)  # revealed: NameError
except re.error as f:
    reveal_type(f)  # revealed: error
```

## Unknown type in except handler does not cause spurious diagnostic

```py
from nonexistent_module import foo  # error: [unresolved-import]

try:
    x
except foo as e:
    reveal_type(foo)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown
```

## Multiple Exceptions in a Tuple

```py
EXCEPTIONS = (AttributeError, TypeError)

try:
    x
except (RuntimeError, OSError) as e:
    reveal_type(e)  # revealed: RuntimeError | OSError
except EXCEPTIONS as f:
    reveal_type(f)  # revealed: AttributeError | TypeError
```

## Dynamic exception types

```py
def foo(x: type[AttributeError], y: tuple[type[OSError], type[RuntimeError]], z: tuple[type[BaseException], ...]):
    try:
        w
    except x as e:
        # TODO: should be `AttributeError`
        reveal_type(e)  # revealed: @Todo
    except y as f:
        # TODO: should be `OSError | RuntimeError`
        reveal_type(f)  # revealed: @Todo
    except z as g:
        # TODO: should be `BaseException`
        reveal_type(g)  # revealed: @Todo
```
