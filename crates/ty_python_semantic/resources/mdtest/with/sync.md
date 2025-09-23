# With statements

## Basic `with` statement

The type of the target variable in a `with` statement is the return type from the context manager's
`__enter__` method.

```py
class Target: ...

class Manager:
    def __enter__(self) -> Target:
        return Target()

    def __exit__(self, exc_type, exc_value, traceback): ...

with Manager() as f:
    reveal_type(f)  # revealed: Target
```

## Union context manager

```py
def _(flag: bool):
    class Manager1:
        def __enter__(self) -> str:
            return "foo"

        def __exit__(self, exc_type, exc_value, traceback): ...

    class Manager2:
        def __enter__(self) -> int:
            return 42

        def __exit__(self, exc_type, exc_value, traceback): ...

    context_expr = Manager1() if flag else Manager2()

    with context_expr as f:
        reveal_type(f)  # revealed: str | int
```

## Context manager without an `__enter__` or `__exit__` method

```py
class Manager: ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    ...
```

## Context manager without an `__enter__` method

```py
class Manager:
    def __exit__(self, exc_tpe, exc_value, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__`"
with Manager():
    ...
```

## Context manager without an `__exit__` method

```py
class Manager:
    def __enter__(self): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__exit__`"
with Manager():
    ...
```

## Context manager with non-callable `__enter__` attribute

```py
class Manager:
    __enter__: int = 42

    def __exit__(self, exc_tpe, exc_value, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__enter__`"
with Manager():
    ...
```

## Union context managers with specific member issues

### Union where one member lacks `__enter__`

```py
def _(flag: bool):
    class Bound:
        def __enter__(self) -> str:
            return "foo"

        def __exit__(self, exc_type, exc_value, traceback): ...

    class EnterUnbound:
        def __exit__(self): ...

    context_expr = Bound() if flag else EnterUnbound()

    # error: [invalid-context-manager] "Object of type `Bound | EnterUnbound` cannot be used with `with` because the method `__enter__` of `EnterUnbound` is possibly unbound"
    with context_expr as f:
        reveal_type(f)  # revealed: str
```

### Union where one member lacks `__exit__`

```py
def _(flag: bool):
    class Bound:
        def __enter__(self) -> str:
            return "foo"

        def __exit__(self, exc_type, exc_value, traceback): ...

    class ExitUnbound:
        def __enter__(self): ...

    context_expr = Bound() if flag else ExitUnbound()

    # error: [invalid-context-manager] "Object of type `Bound | ExitUnbound` cannot be used with `with` because the method `__exit__` of `ExitUnbound` is possibly unbound"
    with context_expr as f:
        reveal_type(f)  # revealed: str | Unknown
```

### Union where one member lacks both methods

```py
def _(flag: bool):
    class Bound:
        def __enter__(self) -> str:
            return "foo"

        def __exit__(self, exc_type, exc_value, traceback): ...

    class Unbound: ...
    context_expr = Bound() if flag else Unbound()

    # error: [invalid-context-manager] "Object of type `Bound | Unbound` cannot be used with `with` because the methods `__enter__` and `__exit__` of `Unbound` are possibly unbound"
    with context_expr as f:
        reveal_type(f)  # revealed: str
```

### Complex union with multiple issues

```py
def _(flag: int):
    class Bound:
        def __enter__(self) -> str:
            return "foo"

        def __exit__(self, exc_type, exc_value, traceback): ...

    class EnterUnbound:
        def __exit__(self): ...

    class ExitUnbound:
        def __enter__(self): ...

    if flag == 0:
        context_expr = Bound()
    elif flag == 1:
        context_expr = EnterUnbound()
    else:
        context_expr = ExitUnbound()

    # error: [invalid-context-manager] "Object of type `Bound | EnterUnbound | ExitUnbound` cannot be used with `with` because the method `__enter__` of `EnterUnbound` is possibly unbound, and the method `__exit__` of `ExitUnbound` is possibly unbound"
    with context_expr as f:
        reveal_type(f)  # revealed: str | Unknown
```

### Union with multiple members missing the same methods

```py
def _(flag: int):
    class EnterUnbound:
        def __exit__(self): ...

    class ExitUnbound:
        def __enter__(self): ...

    class Unbound: ...
    if flag == 0:
        context_expr = EnterUnbound()
    elif flag == 1:
        context_expr = ExitUnbound()
    else:
        context_expr = Unbound()

    # error: [invalid-context-manager] "Object of type `EnterUnbound | ExitUnbound | Unbound` cannot be used with `with` because the method `__enter__` of `EnterUnbound` and `Unbound` are possibly unbound, and the method `__exit__` of `ExitUnbound` and `Unbound` are possibly unbound"
    with context_expr:
        ...
```

## Context manager with non-callable `__exit__` attribute

```py
from typing_extensions import Self

class Manager:
    def __enter__(self) -> Self:
        return self
    __exit__: int = 32

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__exit__`"
with Manager():
    ...
```

## Context expression with possibly-unbound union variants

```py
def _(flag: bool):
    class Manager1:
        def __enter__(self) -> str:
            return "foo"

        def __exit__(self, exc_type, exc_value, traceback): ...

    class NotAContextManager: ...
    context_expr = Manager1() if flag else NotAContextManager()

    # error: [invalid-context-manager] "Object of type `Manager1 | NotAContextManager` cannot be used with `with` because the methods `__enter__` and `__exit__` of `NotAContextManager` are possibly unbound"
    with context_expr as f:
        reveal_type(f)  # revealed: str
```

## Context expression with "sometimes" callable `__enter__` method

```py
def _(flag: bool):
    class Manager:
        if flag:
            def __enter__(self) -> str:
                return "abcd"

        def __exit__(self, *args): ...

    # error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because the method `__enter__` is possibly unbound"
    with Manager() as f:
        reveal_type(f)  # revealed: str
```

## Invalid `__enter__` signature

```py
class Manager:
    def __enter__() -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

context_expr = Manager()

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not correctly implement `__enter__`"
with context_expr as f:
    reveal_type(f)  # revealed: str
```

## Accidental use of non-async `with`

<!-- snapshot-diagnostics -->

If a synchronous `with` statement is used on a type with `__aenter__` and `__aexit__`, we show a
diagnostic hint that the user might have intended to use `async with` instead.

```py
class Manager:
    async def __aenter__(self): ...
    async def __aexit__(self, *args): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    ...
```

## Incorrect signatures

The sub-diagnostic is also provided if the signatures of `__aenter__` and `__aexit__` do not match
the expected signatures for a context manager:

```py
class Manager:
    async def __aenter__(self): ...
    async def __aexit__(self, typ: str, exc, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    ...
```

## Incorrect number of arguments

Similarly, we also show the hint if the functions have the wrong number of arguments:

```py
class Manager:
    async def __aenter__(self, wrong_extra_arg): ...
    async def __aexit__(self, typ, exc, traceback, wrong_extra_arg): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it does not implement `__enter__` and `__exit__`"
with Manager():
    ...
```
