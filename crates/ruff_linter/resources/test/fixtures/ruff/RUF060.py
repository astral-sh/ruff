import contextlib

# Valid: Single yield in a context manager
@contextlib.contextmanager
def valid_context_manager():
    print("Setting up")
    yield "value"
    print("Cleaning up")

# Valid: Single yield_from in a context manager
@contextlib.contextmanager
def valid_yield_from_context_manager():
    print("Setting up")
    yield from ["value"]
    print("Cleaning up")

# Valid: Single yield in a context manager with nested function
@contextlib.contextmanager
def valid_with_nested():
    print("Setting up")

    def nested_func():
        yield "nested yield"  # yield OK, nested function
        yield "multiple yields in nested OK too"

    yield "value"
    print("Cleaning up")

# Valid: Using contextmanager with async
@contextlib.asynccontextmanager
async def valid_async_context_manager():
    print("Setting up async")
    yield "async value"
    print("Cleaning up async")

# Valid: No decorator
def not_a_context_manager():
    yield "first"
    yield "second"

# Valid: Different decorator
@staticmethod
def different_decorator():
    yield "first"
    yield "second"

# Valid: Context manager with a class
class ValidClass:
    @contextlib.contextmanager
    def valid_method(self):
        print("Setting up")
        yield "value"
        print("Cleaning up")

# Valid: Multiple yields in mutually exclusive branches
@contextlib.contextmanager
def valid_conditional_yield(condition):
    print("Setting up")
    if condition:
        yield "for condition"
    else:
        yield "for else condition"
    print("Cleaning up")

# Valid: Only one yield executes per run
@contextlib.contextmanager
def valid_try_else_finally():
    try:
        pass
    except Exception:
        yield "in except"
    else:
        yield "in else"
    print("done")

# Valid: Yields in mutually exclusive match arms
@contextlib.contextmanager
def valid_match_yields(value):
    match value:
        case "a":
            yield "from a"
        case "b":
            yield "from b"
        case _:
            yield "from default"

# Valid: Multiple yields in a nested function
@contextlib.contextmanager
def valid_nested_function():
    def inner():
        yield "inner one"
        yield "inner two"
    yield "outer"

# Valid: Using contextmanager as a variable name followed by direct call
contextmanager = lambda f: f
@contextmanager
def not_really_context_manager():
    yield "first"
    yield "second"  # This is not a violation since it's not the real contextmanager

# Valid: Multiple excepts that each yield
@contextlib.contextmanager
def multiple_except_yields():
    try:
        pass
    except ValueError:
        yield "val error"
    except TypeError:
        yield "type error"

# Valid: each arm yields once
@contextlib.contextmanager
def valid_multiple_elifs():
    if 1 == 3:
        yield "never this"
    elif 1 == 2:
        yield "this doesn't"
    elif 2 == 2:
        yield "this does"
    else:
        yield "This would if reached"

# Invalid: Yields in consecutive branches of try/except
@contextlib.contextmanager
def valid_try_except_yields():
    print("Setting up")
    try:
        yield "try yield"
    except Exception:
        yield "except yield"
    print("Cleaning up")

# Invalid: Variable number of yields based on conditions
@contextlib.contextmanager
def conditional_yields(condition):
    yield "first"
    if condition:
        yield "conditional"  # RUF060

# Invalid(joint): Multiple nested context managers
def outer_function():
    @contextlib.contextmanager
    def inner_context_manager():  # This is fine, one yield
        yield "inner value"

    @contextlib.contextmanager
    def another_inner_cm():
        yield "first"
        yield "second"  # RUF060

# Invalid: else yields after try yields
@contextlib.contextmanager
def invalid_try_else_finally():
    try:
        yield "first"
    except Exception:
        pass
    else:
        yield "second" #RUF060
    print("done")

# Invalid: Multiple yields in the same match arm
@contextlib.contextmanager
def invalid_match_same_case(value):
    match value:
        case "a":
            yield "one"
            yield "two"  # RUF060
        case _:
            yield "default"

# Invalid: Yield in a loop (always ambiguous)
@contextlib.contextmanager
def invalid_yield_in_loop():
    for i in range(3):
        yield i  # RUF060

# Invalid: Yields in try and finally
@contextlib.contextmanager
def invalid_try_finally():
    try:
        yield "in try"
    finally:
        yield "in finally"  # RUF060

# Invalid: Multiple yields in an async context manager
@contextlib.asynccontextmanager
async def invalid_async_context_manager():
    print("Setting up async")
    yield "first async value"
    yield "second async value"  # RUF060
    print("Cleaning up async")

# Invalid: Contextlib imported with an alias
import contextlib as ctx

@ctx.contextmanager
def invalid_with_import_alias():
    yield "first"
    yield "second"  # RUF060

# Invalid: Direct import of contextmanager
from contextlib import contextmanager

@contextmanager
def invalid_with_direct_import():
    yield "first"
    yield "second"  # RUF060

# Invalid: Multiple yields in a context manager
@contextlib.contextmanager
def invalid_multiple_yields():
    print("Setting up")
    yield "first value"
    print("In between yields")
    yield "second value"  # RUF060
    print("Cleaning up")

# Invalid: Multiple yields in the same branch
@contextlib.contextmanager
def invalid_multiple_in_same_branch(condition):
    print("Setting up")
    if condition:
        yield "first in condition"
        yield "second in condition"  # RUF060
    else:
        yield "in else"
    print("Cleaning up")
