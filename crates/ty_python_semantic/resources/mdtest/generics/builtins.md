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

Collection-initializer inference should still validate custom constructor signatures.

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

class set[T](list[T]): ...
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
list()  # error: [no-matching-overload]
set()  # error: [no-matching-overload]
```

## Constructor return types with custom `list` and `set`

Collection-initializer inference should not replace constructor return semantics supplied by a
custom typeshed.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class object: ...
class int: ...
class str: ...
class tuple: ...
class type: ...

class SetMeta(type):
    def __call__(self) -> str: ...

class list[T]:
    def __new__(cls) -> int: ...

class set[T](metaclass=SetMeta): ...
```

`/typeshed/stdlib/types.pyi`:

```pyi
class FunctionType: ...
```

```py
list_result: int = list()
set_result: str = set()
```
