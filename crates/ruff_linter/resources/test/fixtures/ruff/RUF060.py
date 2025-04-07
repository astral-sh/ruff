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

# Invalid: Multiple yields in a context manager
@contextlib.contextmanager
def invalid_multiple_yields():
    print("Setting up")
    yield "first value"
    print("In between yields")
    yield "second value"  # RUF060
    print("Cleaning up")

# Invalid: Using contextmanager as a variable name followed by direct call
contextmanager = lambda f: f
@contextmanager
def not_really_context_manager():
    yield "first"
    yield "second"  # This is not a violation since it's not the real contextmanager

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

# Edge case: Multiple nested context managers
def outer_function():
    @contextlib.contextmanager
    def inner_context_manager():  # This is fine, one yield
        yield "inner value"

    @contextlib.contextmanager
    def another_inner_cm():
        yield "first"
        yield "second"  # RUF060

# Edge case: Variable number of yields based on conditions
@contextlib.contextmanager
def conditional_yields(condition):
    yield "first"
    if condition:
        yield "conditional"  # RUF060
