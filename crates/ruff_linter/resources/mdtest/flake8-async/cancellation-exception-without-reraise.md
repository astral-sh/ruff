# `cancellation-exception-without-reraise` (`ASYNC103`)

```toml
lint.preview = true
lint.select = ["ASYNC103"]
```

## Cancellation handlers

We report a handler for a cancellation exception, or a broad handler that
would catch one, unless every path through the handler raises.

```py
import asyncio
import anyio
import trio

try:
    work()
except asyncio.CancelledError:  # snapshot: cancellation-exception-without-reraise
    cleanup()

try:
    work()
except asyncio.exceptions.CancelledError:  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except trio.Cancelled:  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except anyio.get_cancelled_exc_class():  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except:  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except Exception:
    cleanup()
```

```snapshot
error[ASYNC103]: `asyncio.CancelledError` handler has a code path that does not raise an exception
 --> src/mdtest_snippet.py:7:8
  |
7 | except asyncio.CancelledError:  # snapshot: cancellation-exception-without-reraise
  |        ^^^^^^^^^^^^^^^^^^^^^^
```

Imported aliases and cancellation exceptions within tuples are also recognized.

```py
from asyncio import CancelledError as Cancelled
from anyio import get_cancelled_exc_class as cancelled_class

try:
    work()
except Cancelled:  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except cancelled_class():  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except (ValueError, asyncio.CancelledError, TypeError):  # error: [cancellation-exception-without-reraise]
    cleanup()
```

## Re-raising on every branch

Handlers are accepted when every possible branch raises. An irrefutable `match`
pattern makes the statement exhaustive, and raising a different exception also
counts.

```py
import asyncio

try:
    work()
except asyncio.CancelledError:
    cleanup()
    raise

try:
    work()
except BaseException:
    if condition:
        raise
    else:
        raise

try:
    work()
except BaseException:
    match value:
        case 1 | _:
            raise

try:
    work()
except BaseException:
    raise RuntimeError
```

Conditional branches that can fall through are reported.

```py
try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    if condition:
        raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    match value:
        case _ if condition:
            raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    match value:
        case 1:
            raise
```

## Other exits

Returning, breaking, or continuing before a later raise leaves a path that does
not raise from the handler.

```py
def returns_before_raise():
    try:
        work()
    except BaseException:  # error: [cancellation-exception-without-reraise]
        return
        raise


try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    for _ in [1]:
        if condition:
            break
        raise
    else:
        raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    for _ in [1]:
        if condition:
            continue
        raise
```

## Loops

A raise inside a `for` loop is sufficient only when the iterable is statically
guaranteed to be non-empty. Literal containers and literal `range` calls can
establish that guarantee. A `range` bound too large to evaluate exactly is
still known to be positive.

```py
try:
    work()
except BaseException:
    for _ in [1]:
        raise

try:
    work()
except BaseException:
    for _ in (*(), *(1, 2)):
        raise

try:
    work()
except BaseException:
    for _ in {**{}, **{1: 2}}:
        raise

try:
    work()
except BaseException:
    for _ in range(3, 0, -1):
        raise

try:
    work()
except BaseException:
    for _ in range(27_670_116_110_564_327_421):
        raise

try:
    work()
except BaseException:
    for _ in range(True):
        raise
```

Empty or unknown iterables can skip the loop body and are therefore reported.

```py
try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    for _ in []:
        raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    for _ in range(0):
        raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    for _ in values:
        raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    for _ in range(value):
        raise


async def consumes_async_iterable():
    try:
        work()
    except BaseException:  # error: [cancellation-exception-without-reraise]
        async for _ in values:
            raise
```

A loop `else` suite can guarantee a raise even when the body never runs.

```py
try:
    work()
except BaseException:
    for _ in values:
        cleanup()
    else:
        raise
```

A `while` loop may not run unless its condition is statically known to be
truthy.

```py
try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    while condition:
        raise

try:
    work()
except BaseException:
    while True:
        raise
```

An infinite loop without a `break` never reaches the statements after it.

```py
try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    while True:
        continue
    raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    while True:
        cleanup()
    raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    if condition:
        while True:
            continue
    raise

try:
    work()
except BaseException:
    while True:
        break
    raise
```

## Nested control flow

A nested `try` without exception handlers cannot swallow a raise from its
body. With handlers present, any statement in the body can raise and be
caught, so the `try` is only considered to raise when its `finally` suite
always raises, or when its body, `else` suite, and every handler always raise.

```py
try:
    work()
except BaseException:
    try:
        raise
    finally:
        cleanup()

try:
    work()
except BaseException:
    try:
        cleanup()
    finally:
        raise

try:
    work()
except BaseException:
    try:
        raise
    except ValueError:
        raise

try:
    work()
except BaseException:
    try:
        cleanup()
    except ValueError:
        raise
    else:
        raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    try:
        raise
    except ValueError:
        cleanup()
```

Non-raising exits from a nested `try` still escape when its `finally` suite
falls through.

```py
def returns_from_nested_try():
    try:
        work()
    except BaseException:  # error: [cancellation-exception-without-reraise]
        try:
            return
        finally:
            pass
        raise


def returns_from_nested_try_with_handlers():
    try:
        work()
    except BaseException:  # error: [cancellation-exception-without-reraise]
        try:
            return
        except ValueError:
            raise
        raise


try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    for _ in [1]:
        try:
            break
        finally:
            pass
    else:
        raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    for _ in [1]:
        try:
            if condition:
                continue
        finally:
            pass
        raise
```

An exit from `finally` overrides the earlier outcome.

```py
def returns_from_finally():
    try:
        work()
    except BaseException:  # error: [cancellation-exception-without-reraise]
        try:
            raise
        finally:
            return


try:
    work()
except BaseException:
    for _ in [1]:
        try:
            break
        finally:
            raise

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    while True:
        try:
            raise
        finally:
            continue
```

A body that never exits cannot reach its `finally` suite.

```py
try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    try:
        while True:
            cleanup()
    finally:
        raise
```

A raise inside a nested function does not re-raise from its enclosing handler,
but a class body executes as part of the `class` statement.

```py
import asyncio

try:
    work()
except asyncio.CancelledError:  # error: [cancellation-exception-without-reraise]
    def nested():
        raise


try:
    work()
except asyncio.CancelledError:  # error: [cancellation-exception-without-reraise]
    class WithMethod:
        def method(self):
            raise


try:
    work()
except asyncio.CancelledError:
    class Raises:
        raise RuntimeError
```

## Handler ordering and name resolution

Once any handler recognized by this rule appears, later broad handlers are
ignored; a later handler for a different cancellation exception is still
checked. Tuples are classified by their most specific recognized element.

```py
import asyncio
import trio

try:
    work()
except asyncio.CancelledError:
    raise
except BaseException:
    handle_other_base_exception()

try:
    work()
except asyncio.CancelledError:  # error: [cancellation-exception-without-reraise]
    cleanup()
except BaseException:
    handle_other_base_exception()

try:
    work()
except BaseException:  # error: [cancellation-exception-without-reraise]
    cleanup()
except:
    handle_other_base_exception()

try:
    work()
except trio.Cancelled:
    raise
except (BaseException, asyncio.CancelledError):  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except trio.Cancelled:
    raise
except (asyncio.CancelledError, BaseException):  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except asyncio.CancelledError:
    raise
except trio.Cancelled:  # error: [cancellation-exception-without-reraise]
    cleanup()

BaseException = ValueError
try:
    work()
except BaseException:
    cleanup()
```

## Exception groups

`except*` handlers are analyzed in the same way as ordinary handlers.

```py
try:
    work()
except* BaseException:  # error: [cancellation-exception-without-reraise]
    cleanup()

try:
    work()
except* BaseException:
    raise
```

## Known limitations

Exception-suppressing context managers are not modeled. A non-raising path
through an infinite loop may also be missed when a raise appears elsewhere in
the handler: a `continue`, a `break` that may never be taken, or a loop inside
a nested `try` whose handlers could catch a raise from the body.

```py
import contextlib

try:
    work()
except BaseException:
    with contextlib.suppress(BaseException):
        raise

try:
    work()
except BaseException:
    while True:
        if condition:
            continue
        raise

try:
    work()
except BaseException:
    while True:
        if condition:
            break
    raise

try:
    work()
except BaseException:
    try:
        while True:
            pass
    except ValueError:
        raise
    raise
```
