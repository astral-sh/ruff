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
def coinflip() -> bool:
    return True

class Manager1:
    def __enter__(self) -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

class Manager2:
    def __enter__(self) -> int:
        return 42

    def __exit__(self, exc_type, exc_value, traceback): ...

context_expr = Manager1() if coinflip() else Manager2()

with context_expr as f:
    reveal_type(f)  # revealed: str | int
```

## Context manager without an `__enter__` or `__exit__` method

```py
class Manager: ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it doesn't implement `__enter__` and `__exit__`"
with Manager():
    ...
```

## Context manager without an `__enter__` method

```py
class Manager:
    def __exit__(self, exc_tpe, exc_value, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it doesn't implement `__enter__`"
with Manager():
    ...
```

## Context manager without an `__exit__` method

```py
class Manager:
    def __enter__(self): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because it doesn't implement `__exit__`"
with Manager():
    ...
```

## Context manager with non-callable `__enter__` attribute

```py
class Manager:
    __enter__ = 42

    def __exit__(self, exc_tpe, exc_value, traceback): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because the method `__enter__` of type `Literal[42]` is not callable"
with Manager():
    ...
```

## Context manager with non-callable `__exit__` attribute

```py
class Manager:
    def __enter__(self) -> Self: ...

    __exit__ = 32

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because the method `__exit__` of type `Literal[32]` is not callable"
with Manager():
    ...
```

## Context expression with possibly-unbound union variants

```py
def coinflip() -> bool:
    return True

class Manager1:
    def __enter__(self) -> str:
        return "foo"

    def __exit__(self, exc_type, exc_value, traceback): ...

class NotAContextManager: ...

context_expr = Manager1() if coinflip() else NotAContextManager()

# error: [invalid-context-manager] "Object of type `Manager1 | NotAContextManager` cannot be used with `with` because the method `__enter__` is possibly unbound"
# error: [invalid-context-manager] "Object of type `Manager1 | NotAContextManager` cannot be used with `with` because the method `__exit__` is possibly unbound"
with context_expr as f:
    reveal_type(f)  # revealed: str
```

## Context expression with "sometimes" callable `__enter__` method

```py
def coinflip() -> bool:
    return True

class Manager:
    if coinflip():
        def __enter__(self) -> str:
            return "abcd"

    def __exit__(self, *args): ...

# error: [invalid-context-manager] "Object of type `Manager` cannot be used with `with` because the method `__enter__` is possibly unbound"
with Manager() as f:
    reveal_type(f)  # revealed: str
```
