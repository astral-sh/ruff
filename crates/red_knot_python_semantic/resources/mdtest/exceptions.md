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
from nonexistent_module import foo # error: [unresolved-import]

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

## TODO: Dynamic exception types

> TODO: `e` should be `AttributeError`. `f` should be `OSError | RuntimeError`. `g` should be `BaseException`.

```py
def foo(x: type[AttributeError], y: tuple[type[OSError], type[RuntimeError]], z: tuple[type[BaseException], ...]):
    try:
        w
    except x as e:
        reveal_type(e)  # revealed: @Todo
    except y as f:
        reveal_type(f)  # revealed: @Todo
    except z as g:
        reveal_type(g)  # revealed: @Todo
```

## TODO: Except star

TODO(Alex): Once we support `sys.version_info` branches, we can set `--target-version=py311` in this tests and the inferred type will just be `BaseExceptionGroup`

## Except\* with BaseException

```py
try:
    x
except* BaseException as e:
    reveal_type(e)  # revealed: Unknown | BaseExceptionGroup
```

## TODO: Except\* with specific exception

> TODO(Alex): more precise would be `ExceptionGroup[OSError]`.

```py
try:
    x
except* OSError as e:
    reveal_type(e)  # revealed: Unknown | BaseExceptionGroup
```

## TODO: Except\* with multiple exceptions

> TODO(Alex): more precise would be `ExceptionGroup[TypeError | AttributeError]`.

```py
try:
    x
except* (TypeError, AttributeError) as e:
    reveal_type(e)  # revealed: Unknown | BaseExceptionGroup
```
