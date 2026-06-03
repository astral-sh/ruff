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

## Empty collection constructors with custom typeshed

Collection-initializer inference is disabled under custom typeshed, leaving custom constructors on
their existing call-inference paths.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
class object: ...
class int: ...
class tuple: ...

class list[T]:
    def __new__(cls) -> int: ...

class set[T]:
    def __init__(self, required: int) -> None: ...
```

`/typeshed/stdlib/types.pyi`:

```pyi
class FunctionType: ...
```

```py
def custom_constructor_semantics() -> None:
    result: int = list()

set()  # error: [missing-argument]
```
