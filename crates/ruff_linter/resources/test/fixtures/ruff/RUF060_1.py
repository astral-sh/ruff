import contextlib

# (1) Invalid: Variable number of yields based on conditions
@contextlib.contextmanager
def conditional_yields(condition):
    yield "first"
    if condition:
        yield "conditional"  # RUF060

# (2) Invalid(joint): Multiple nested context managers
def outer_function():
    @contextlib.contextmanager
    def inner_context_manager():  # This is fine, one yield
        yield "inner value"

    @contextlib.contextmanager
    def another_inner_cm():
        yield "first"
        yield "second"  # RUF060

# (3) Invalid: else yields after try yields
@contextlib.contextmanager
def invalid_try_else_finally():
    try:
        yield "first"
    except Exception:
        pass
    else:
        yield "second" #RUF060
    print("done")

# (4) Invalid: Multiple yields in the same match arm
@contextlib.contextmanager
def invalid_match_same_case(value):
    match value:
        case "a":
            yield "one"
            yield "two"  # RUF060
        case _:
            yield "default"

# (5) Invalid: Yield in a loop (always ambiguous)
@contextlib.contextmanager
def invalid_yield_in_loop():
    for i in range(3):
        yield i  # RUF060

# (6) Invalid: Yields in try and finally
@contextlib.contextmanager
def invalid_try_finally():
    try:
        yield "in try"
    finally:
        yield "in finally"  # RUF060

# (7) Invalid: Multiple yields in an async context manager
@contextlib.asynccontextmanager
async def invalid_async_context_manager():
    print("Setting up async")
    yield "first async value"
    yield "second async value"  # RUF060
    print("Cleaning up async")

# (8) Invalid: Contextlib imported with an alias
import contextlib as ctx

@ctx.contextmanager
def invalid_with_import_alias():
    yield "first"
    yield "second"  # RUF060

# (9) Invalid: Direct import of contextmanager
from contextlib import contextmanager

@contextmanager
def invalid_with_direct_import():
    yield "first"
    yield "second"  # RUF060

# (10) Invalid: Multiple yields in a context manager
@contextlib.contextmanager
def invalid_multiple_yields():
    print("Setting up")
    yield "first value"
    print("In between yields")
    yield "second value"  # RUF060
    print("Cleaning up")

# (11) Invalid: Multiple yields in the same branch
@contextlib.contextmanager
def invalid_multiple_in_same_branch(condition):
    print("Setting up")
    if condition:
        yield "first in condition"
        yield "second in condition"  # RUF060
    else:
        yield "in else"
    print("Cleaning up")

# (12) Invalid: Multiple yields in complex try/except/else/finally
@contextlib.contextmanager
def complex_try_except_else_finally(condition):
    try:
        if condition:
            yield "in try if"
        else:
            yield "in try else"
    except ValueError:
        yield "in ValueError"  # RUF060
    except TypeError:
        yield "in TypeError"  # RUF060
    else:
        yield "in else"  # RUF060
    finally:
        yield "in finally"  # RUF060

# (13) Invalid: Yield in for loop
@contextlib.contextmanager
def yield_in_for():
    for i in range(3):
        yield f"in loop {i}"  # RUF060 - multiple yields in loop

# (14) Invalid: Yield in while loop
@contextlib.contextmanager
def yield_in_while():
    x = 1
    while x < 3:
        yield f"in loop {x}"  # RUF060 - multiple yields in loop
        x += 1
