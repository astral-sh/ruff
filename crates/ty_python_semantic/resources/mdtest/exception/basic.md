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
    zz: tuple[type[TypeError | RuntimeError], ...],
    zzz: type[BaseException] | tuple[type[BaseException], ...],
):
    try:
        help()
    except x as e:
        reveal_type(e)  # revealed: AttributeError
    except y as f:
        reveal_type(f)  # revealed: OSError | RuntimeError
    except z as g:
        reveal_type(g)  # revealed: BaseException
    except zz as h:
        reveal_type(h)  # revealed: TypeError | RuntimeError
    except zzz as i:
        reveal_type(i)  # revealed: BaseException
```

We do not emit an `invalid-exception-caught` if a class is caught that has `Any` or `Unknown` in its
MRO, as the dynamic element in the MRO could materialize to some subclass of `BaseException`:

```py
from compat import BASE_EXCEPTION_CLASS  # error: [unresolved-import] "Cannot resolve imported module `compat`"

class Error(BASE_EXCEPTION_CLASS): ...

try:
    ...
except Error as err:
    ...
```

## Exception with no captured type

```py
try:
    {}.get("foo")
except TypeError:
    pass
```

## Exception which catches typevar

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Callable

def silence[T: type[BaseException]](
    func: Callable[[], None],
    exception_type: T,
):
    try:
        func()
    except exception_type as e:
        reveal_type(e)  # revealed: T'instance

def silence2[T: (
    type[ValueError],
    type[TypeError],
)](func: Callable[[], None], exception_type: T,):
    try:
        func()
    except exception_type as e:
        reveal_type(e)  # revealed: T'instance
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
    # error: [invalid-exception-caught]
    except z as g:
        reveal_type(g)  # revealed: Unknown

try:
    {}.get("foo")
# error: [invalid-exception-caught]
except int:
    pass
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
