# Generic builtins

## Variadic keyword arguments with a custom `dict`

When we define `dict` in a custom typeshed, we must take care to define it as a generic class in the
same way as in the real typeshed.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class object: ...
class int: ...
class tuple: ...
class dict[K, V, Extra]: ...
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

If we don't, then we may get "surprising" results when inferring the types of variadic keyword
arguments.

```py
def f(**kwargs):
    reveal_type(kwargs)  # revealed: dict[Unknown, Unknown, Unknown]

def g(**kwargs: int):
    reveal_type(kwargs)  # revealed: dict[Unknown, Unknown, Unknown]
```

## Constructor diagnostics with custom `list` and `set`

Collection-initializer inference should only replace the result type of a supported `list()` or
`set()` call. We should still validate the call against custom typeshed constructor signatures.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
from typing_extensions import overload

class object: ...
class int: ...
class str: ...
class tuple: ...

class list[T]:
    @overload
    def __init__(self, required: int) -> None: ...
    @overload
    def __init__(self, required: str) -> None: ...

class set[T]:
    @overload
    def __init__(self, required: int) -> None: ...
    @overload
    def __init__(self, required: str) -> None: ...
```

`/typeshed/stdlib/types.pyi`:

```pyi
class FunctionType: ...
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def overload(func, /): ...
```

```py
import builtins

list()  # error: [no-matching-overload]
set()  # error: [no-matching-overload]
builtins.list()  # error: [no-matching-overload]
builtins.set()  # error: [no-matching-overload]
```
