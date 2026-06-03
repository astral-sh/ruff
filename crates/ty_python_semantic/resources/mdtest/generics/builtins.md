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
    def append(self, value: T) -> None: ...

class set[T](list[T]):
    def add(self, value: T) -> None: ...
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

def assigned_failed_constructor_calls() -> None:
    xs = list()  # error: [no-matching-overload]
    xs.append(1)

    ys = set()  # error: [no-matching-overload]
    ys.add(1)
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
from typing_extensions import Never

class object: ...

class int:
    def stop(self) -> Never: ...

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

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
Never: object

def reveal_type(obj, /): ...
```

```py
from typing_extensions import reveal_type

list_result: int = list()
set_result: str = set()

def constructor_return_reachability() -> None:
    result = list()
    reveal_type(result)  # revealed: int
    reveal_type(result.stop)  # revealed: bound method int.stop() -> Never
    result.stop()

    after: str = 1
```

## Explicit unspecialized collection constructor returns

An explicit unspecialized collection return is still part of the constructor's semantics.

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

class list[T]:
    def __new__(cls) -> list: ...
    def append(self, value: T) -> None: ...
    def get(self) -> T: ...
```

`/typeshed/stdlib/types.pyi`:

```pyi
class FunctionType: ...
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
def reveal_type(obj, /): ...
```

```py
from typing_extensions import reveal_type

def explicit_unspecialized_return() -> None:
    xs = list()
    reveal_type(xs)  # revealed: list[Unknown]
    xs.append(1)
    value: str = xs.get()
```

## Reachability for specialized custom collection constructor returns

Collection method calls still need normal reachability analysis when a custom typeshed preserves a
specialized collection return type.

```toml
[environment]
typeshed = "/typeshed"
```

`/typeshed/stdlib/builtins.pyi`:

```pyi
from typing_extensions import Never

class object: ...
class int: ...
class str: ...
class tuple: ...
class type: ...

class ListMeta(type):
    def __call__(self) -> list[Never]: ...

class list[T](metaclass=ListMeta):
    def pop(self) -> T: ...
```

`/typeshed/stdlib/types.pyi`:

```pyi
class FunctionType: ...
```

`/typeshed/stdlib/typing_extensions.pyi`:

```pyi
Never: object
```

```py
def specialized_constructor_return_reachability() -> None:
    result = list()
    result.pop()

    after: str = 1
```
