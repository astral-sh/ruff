# `fallible-context-manager` (`RUF075`)

```toml
lint.preview = true
lint.select = ["RUF075"]
```

## Basic errors

A `@contextmanager` function whose `yield` is followed by cleanup code is flagged: an exception
raised in the `with` block will skip the cleanup.

```py
from contextlib import contextmanager


@contextmanager
def bad():
    print("start")
    yield  # snapshot: fallible-context-manager
    print("cleanup")
```

```snapshot
error[RUF075]: Context manager does not catch exceptions
 --> src/mdtest_snippet.py:7:5
  |
7 |     yield  # snapshot: fallible-context-manager
  |     ^^^^^
  |
```

## Yield inside a nested `with`, not last

```py
from contextlib import contextmanager


@contextmanager
def bad_with_code_after_yield():
    with other_cm():
        yield  # error: [fallible-context-manager]
        print("cleanup")
```

## Yield inside an `if` that is not terminal

```py
from contextlib import contextmanager


@contextmanager
def bad_conditional_not_last():
    if condition:
        yield  # error: [fallible-context-manager]
    print("after if")
```

## Yield inside a `for` that is not terminal

```py
from contextlib import contextmanager


@contextmanager
def bad_for_not_last():
    for i in range(10):
        yield  # error: [fallible-context-manager]
    print("after loop")
```

## Yield inside a `while` that is not terminal

```py
from contextlib import contextmanager


@contextmanager
def bad_while_not_last():
    while condition:
        yield  # error: [fallible-context-manager]
    print("after while")
```

## Yield inside an `elif` that is not terminal

```py
from contextlib import contextmanager


@contextmanager
def bad_elif_not_last():
    if condition1:
        pass
    elif condition2:
        yield  # error: [fallible-context-manager]
    print("after if")
```

## Yield inside a `match` case that is not terminal

```py
from contextlib import contextmanager


@contextmanager
def bad_match_not_last():
    match x:
        case 1:
            yield  # error: [fallible-context-manager]
    print("after match")
```

## Yield inside a `with` items expression, not terminal

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_in_with_items():
    with some_call((yield)):  # error: [fallible-context-manager]
        pass
    print("cleanup")
```

## Yield inside a `finally` arm, not last

The `finally` arm runs unprotected. A `yield` followed by cleanup code in `finally` will skip the
cleanup if an exception propagates from the `yield`.

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_in_finally_not_last():
    try:
        setup()
    finally:
        yield  # error: [fallible-context-manager]
        cleanup()
```

## Yield in an `except` arm followed by code outside the `try`

`except` is not protected: an exception raised inside the handler is not caught by the same `try`.
A `yield` in `except` followed by code after the `try` is unprotected.

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_in_except_with_trailing_code():
    try:
        setup()
    except Exception:
        yield  # error: [fallible-context-manager]
    cleanup_after_try()
```

## Non-terminal `yield` in an `except` arm

A `yield` inside an `except` handler that is followed by cleanup _inside_ the same handler is
also unprotected: the handler body itself doesn't catch exceptions raised by the `yield`.

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_in_except_with_cleanup():
    try:
        setup()
    except Exception:
        yield  # error: [fallible-context-manager]
        cleanup()
```

## Non-terminal `yield` in an `else` arm

A `yield` inside an `else` arm is not protected by the surrounding `try`, so trailing cleanup
inside the `else` body is skipped if the `yield` raises.

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_in_else_with_cleanup():
    try:
        setup()
    except Exception:
        recover()
    else:
        yield  # error: [fallible-context-manager]
        cleanup()
```

## Yield in a `match` guard expression

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_in_match_guard(value):
    match value:
        case _ if (yield):  # error: [fallible-context-manager]
            pass
    cleanup()
```

## Terminal yields are OK

A `yield` is terminal (and so is OK) when it is the last statement in the function body, or
immediately followed by `return`, or the last statement of an `if`/`for`/`while`/`match` branch
that is itself terminal.

```py
from contextlib import contextmanager, asynccontextmanager


@contextmanager
def good_yield_last():
    print("setup")
    yield


@asynccontextmanager
async def good_async_yield_last():
    yield


@contextmanager
def good_yield_from_last():
    yield from other_generator()


@contextmanager
def good_yield_before_return():
    print("setup")
    yield
    return


@contextmanager
def good_yield_terminal_in_conditional():
    if condition:
        yield


@contextmanager
def good_yield_terminal_in_branches():
    if condition:
        yield
    else:
        yield


@contextmanager
def good_yield_terminal_in_loop():
    for i in range(10):
        yield


@contextmanager
def good_yield_terminal_in_while():
    while condition:
        yield


@contextmanager
def good_yield_terminal_in_elif():
    if condition1:
        yield
    elif condition2:
        yield


@contextmanager
def good_yield_terminal_in_match():
    match x:
        case 1:
            yield
        case _:
            yield


@contextmanager
def good_yield_in_for_else():
    for i in range(10):
        pass
    else:
        yield


@contextmanager
def good_yield_in_while_else():
    while condition:
        pass
    else:
        yield


@contextmanager
def good_yield_in_with_items_terminal():
    with some_call((yield)):
        pass


@contextmanager
def good_return_yield():
    return (yield)
```

## Try / except / finally protections

A `yield` inside a `try` body is protected by `finally` or `except` and is not flagged. Yields in
`except` or `finally` arms that are the last statement of a terminal `try` are also OK.

```py
from contextlib import contextmanager, asynccontextmanager


@contextmanager
def good_try_finally():
    print("start")
    try:
        yield
    finally:
        print("cleanup")


@asynccontextmanager
async def good_async_try_finally():
    try:
        yield
    finally:
        print("cleanup")


@contextmanager
def good_try_finally_with_except():
    try:
        yield
    except Exception:
        pass
    finally:
        print("cleanup")


@contextmanager
def good_nested_try_finally():
    try:
        try:
            yield
        finally:
            print("inner cleanup")
    finally:
        print("outer cleanup")


@contextmanager
def good_conditional_inside_try_finally():
    try:
        if condition:
            yield
        else:
            yield
    finally:
        print("cleanup")


@contextmanager
def good_try_except_without_finally():
    try:
        yield
    except Exception:
        pass


@contextmanager
def good_try_except_with_else():
    try:
        yield
    except Exception:
        pass
    else:
        print("no exception")


@contextmanager
def good_yield_in_except_no_finally():
    try:
        do_something()
    except Exception:
        yield


@contextmanager
def good_yield_in_except_with_finally():
    try:
        do_something()
    except Exception:
        yield
    finally:
        cleanup()


@contextmanager
def good_yield_in_else_with_finally():
    try:
        do_something()
    except Exception:
        pass
    else:
        yield
    finally:
        cleanup()


@contextmanager
def good_yield_in_finally():
    try:
        do_something()
    finally:
        yield


@contextmanager
def good_try_finally_non_terminal():
    try:
        yield
    finally:
        print("cleanup")
    print("code after try")


@contextmanager
def good_try_except_non_terminal():
    try:
        yield
    except Exception:
        print("handle error")
    print("code after try")
```

## `with` block delegation

A `yield` that is the last statement of a `with` body delegates exception handling to the wrapping
context manager and is not flagged.

```py
from contextlib import contextmanager, asynccontextmanager


@contextmanager
def good_with_delegation():
    with other_cm():
        yield


@contextmanager
def good_with_delegation_nested():
    with cm1():
        with cm2():
            yield


@contextmanager
def good_with_multiple_cms():
    with cm1(), cm2():
        yield


@asynccontextmanager
async def good_async_with_delegation():
    async with other_async_cm():
        yield


@contextmanager
def good_with_code_after_with_block():
    with other_cm():
        yield
    print("after with")


@contextmanager
def good_yield_in_with_before_return():
    with other_cm() as value:
        yield value
        return


@contextmanager
def good_yield_in_with_branches():
    with other_cm():
        if condition:
            yield
        else:
            yield
```

## Imports and decorator forms

Both aliased and attribute import forms are recognised.

```py
from contextlib import contextmanager as cm
import contextlib


@cm
def good_aliased_import():
    yield


@contextlib.contextmanager
def good_attribute_import():
    yield
```

## Non-`@contextmanager` functions are ignored

The bodies below would be flagged if the function were decorated with `@contextmanager`
(a non-terminal `yield` followed by cleanup), so this section actually exercises the
decorator gate rather than the `yield`-position check.

```py
def not_a_context_manager():
    yield
    cleanup()


@some_other_decorator
def other_decorator():
    yield
    cleanup()
```

## Nested definitions don't trigger

`yield` inside nested functions, classes, generator expressions, or lambdas does not count toward
the outer `@contextmanager`.

```py
from contextlib import contextmanager


@contextmanager
def good_with_nested_function():
    def inner():
        yield

    try:
        yield
    finally:
        print("cleanup")


@contextmanager
def good_with_generator_expr():
    gen = (x for x in range(10))
    try:
        yield
    finally:
        print("cleanup")


@contextmanager
def good_with_class():
    class Inner:
        def method(self):
            yield

    try:
        yield
    finally:
        print("cleanup")


# A `yield` inside a lambda belongs to the lambda's own (inner) generator,
# not to the surrounding `@contextmanager` function — so the visitor
# short-circuits on lambdas instead of recursing into them.
@contextmanager
def good_with_lambda():
    make_gen = lambda: (yield 1)
    try:
        yield make_gen
    finally:
        cleanup()
```

## Multiple yields under one `try`/`finally`

```py
from contextlib import contextmanager


@contextmanager
def good_multiple_yields():
    try:
        yield 1
        yield 2
    finally:
        print("cleanup")
```

## Yield before `break` in a terminal loop is OK

A `yield` immediately followed by `break` is terminal when the enclosing loop itself sits
in a terminal position, since `break` exits the loop and no cleanup code can run after.

```py
from contextlib import contextmanager


@contextmanager
def good_yield_before_break_in_terminal_for():
    for value in items:
        yield
        break


@contextmanager
def good_yield_before_break_in_terminal_while():
    while condition:
        yield
        break


@contextmanager
def good_yield_before_break_in_terminal_for_in_if():
    if condition:
        for value in items:
            yield
            break
```

## Yield before `break` in a non-terminal loop

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_before_break_with_trailing_code():
    for value in items:
        yield  # error: [fallible-context-manager]
        break
    print("cleanup")
```

## Yield before `break` from an inner loop

`break` only exits the innermost loop, so a `yield` before `break` inside a nested loop
is not terminal: control falls through to the rest of the outer loop body.

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_before_break_in_inner_loop():
    for outer in items:
        for inner in stuff:
            yield  # error: [fallible-context-manager]
            break
        cleanup()
```

## Yield before `continue` is not terminal

```py
from contextlib import contextmanager


@contextmanager
def bad_yield_before_continue():
    for value in items:
        yield  # error: [fallible-context-manager]
        continue
```
