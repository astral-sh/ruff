# Exception Handling

## Single Exception

```py
import re

try:
    help()
except NameError as e:
    reveal_type(e)  # revealed: NameError
except re.error as f:
    reveal_type(f)  # revealed: error
```

## Unknown type in except handler does not cause spurious diagnostic

```py
from nonexistent_module import foo  # error: [unresolved-import]

try:
    help()
except foo as e:
    reveal_type(foo)  # revealed: Unknown
    reveal_type(e)  # revealed: Unknown
```

## Multiple Exceptions in a Tuple

```py
EXCEPTIONS = (AttributeError, TypeError)

try:
    help()
except (RuntimeError, OSError) as e:
    reveal_type(e)  # revealed: RuntimeError | OSError
except EXCEPTIONS as f:
    reveal_type(f)  # revealed: AttributeError | TypeError
```

## Dynamic exception types

```py
def foo(
    x: type[AttributeError],
    y: tuple[type[OSError], type[RuntimeError]],
    z: tuple[type[BaseException], ...],
):
    try:
        help()
    except x as e:
        reveal_type(e)  # revealed: AttributeError
    except y as f:
        reveal_type(f)  # revealed: OSError | RuntimeError
    except z as g:
        # TODO: should be `BaseException`
        reveal_type(g)  # revealed: @Todo(full tuple[...] support)
```

## Invalid exception handlers

```py
try:
    pass
# error: [invalid-exception-caught] "Cannot catch object of type `Literal[3]` in an exception handler (must be a `BaseException` subclass or a tuple of `BaseException` subclasses)"
except 3 as e:
    reveal_type(e)  # revealed: Unknown

try:
    pass
# error: [invalid-exception-caught] "Cannot catch object of type `Literal["foo"]` in an exception handler (must be a `BaseException` subclass or a tuple of `BaseException` subclasses)"
# error: [invalid-exception-caught] "Cannot catch object of type `Literal[b"bar"]` in an exception handler (must be a `BaseException` subclass or a tuple of `BaseException` subclasses)"
except (ValueError, OSError, "foo", b"bar") as e:
    reveal_type(e)  # revealed: ValueError | OSError | Unknown

def foo(
    x: type[str],
    y: tuple[type[OSError], type[RuntimeError], int],
    z: tuple[type[str], ...],
):
    try:
        help()
    # error: [invalid-exception-caught]
    except x as e:
        reveal_type(e)  # revealed: Unknown
    # error: [invalid-exception-caught]
    except y as f:
        reveal_type(f)  # revealed: OSError | RuntimeError | Unknown
    except z as g:
        # TODO: should emit a diagnostic here:
        reveal_type(g)  # revealed: @Todo(full tuple[...] support)
```

## Object raised is not an exception

```py
try:
    raise AttributeError()  # fine
except:
    ...

try:
    raise FloatingPointError  # fine
except:
    ...

try:
    raise 1  # error: [invalid-raise]
except:
    ...

try:
    raise int  # error: [invalid-raise]
except:
    ...

def _(e: Exception | type[Exception]):
    raise e  # fine

def _(e: Exception | type[Exception] | None):
    raise e  # error: [invalid-raise]
```

## Exception cause is not an exception

```py
def _():
    try:
        raise EOFError() from GeneratorExit  # fine
    except:
        ...

def _():
    try:
        raise StopIteration from MemoryError()  # fine
    except:
        ...

def _():
    try:
        raise BufferError() from None  # fine
    except:
        ...

def _():
    try:
        raise ZeroDivisionError from False  # error: [invalid-raise]
    except:
        ...

def _():
    try:
        raise SystemExit from bool()  # error: [invalid-raise]
    except:
        ...

def _():
    try:
        raise
    except KeyboardInterrupt as e:  # fine
        reveal_type(e)  # revealed: KeyboardInterrupt
        raise LookupError from e  # fine

def _():
    try:
        raise
    except int as e:  # error: [invalid-exception-caught]
        reveal_type(e)  # revealed: Unknown
        raise KeyError from e

def _(e: Exception | type[Exception]):
    raise ModuleNotFoundError from e  # fine

def _(e: Exception | type[Exception] | None):
    raise IndexError from e  # fine

def _(e: int | None):
    raise IndexError from e  # error: [invalid-raise]
```
