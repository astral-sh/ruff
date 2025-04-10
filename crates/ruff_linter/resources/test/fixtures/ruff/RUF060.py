import contextlib

# (1) Valid: Single yield in a context manager
@contextlib.contextmanager
def valid_context_manager():
    print("Setting up")
    yield "value"
    print("Cleaning up")

# (2) Valid: Single yield_from in a context manager
@contextlib.contextmanager
def valid_yield_from_context_manager():
    print("Setting up")
    yield from ["value"]
    print("Cleaning up")

# (3) Valid: Single yield in a context manager with nested function
@contextlib.contextmanager
def valid_with_nested():
    print("Setting up")

    def nested_func():
        yield "nested yield"  # yield OK, nested function
        yield "multiple yields in nested OK too"

    yield "value"
    print("Cleaning up")

# (4) Valid: Using contextmanager with async
@contextlib.asynccontextmanager
async def valid_async_context_manager():
    print("Setting up async")
    yield "async value"
    print("Cleaning up async")

# (5) Valid: No decorator
def not_a_context_manager():
    yield "first"
    yield "second"

# (6) Valid: Different decorator
@staticmethod
def different_decorator():
    yield "first"
    yield "second"

# (7) Valid: Context manager with a class
class ValidClass:
    @contextlib.contextmanager
    def valid_method(self):
        print("Setting up")
        yield "value"
        print("Cleaning up")

# (8) Valid: Multiple yields in mutually exclusive branches
@contextlib.contextmanager
def valid_conditional_yield(condition):
    print("Setting up")
    if condition:
        yield "for condition"
    else:
        yield "for else condition"
    print("Cleaning up")

# (9) Valid: Only one yield executes per run
@contextlib.contextmanager
def valid_try_else_finally():
    try:
        pass
    except Exception:
        yield "in except"
    else:
        yield "in else"
    print("done")

# (10) Valid: Yields in mutually exclusive match arms
@contextlib.contextmanager
def valid_match_yields(value):
    match value:
        case "a":
            yield "from a"
        case "b":
            yield "from b"
        case _:
            yield "from default"

# (11) Valid: Multiple yields in a nested function
@contextlib.contextmanager
def valid_nested_function():
    def inner():
        yield "inner one"
        yield "inner two"
    yield "outer"

# (12) Valid: Using contextmanager as a variable name followed by direct call
contextmanager = lambda f: f
@contextmanager
def not_really_context_manager():
    yield "first"
    yield "second"  # This is not a violation since it's not the real contextmanager

# (13) Valid: Multiple excepts that each yield
@contextlib.contextmanager
def multiple_except_yields():
    try:
        pass
    except ValueError:
        yield "val error"
    except TypeError:
        yield "type error"

# (14) Valid: each arm yields once
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

# (15) Valid: Yield in else of for loop
@contextlib.contextmanager
def for_loop_with_else():
    for i in range(3):
        pass
    else:
        yield

# (16) Valid: Yield in else of while loop
@contextlib.contextmanager
def while_loop_with_else():
    x = 1
    while x < 3:
        x += 1
    yield

# (17) Valid: Yield after loop
@contextlib.contextmanager
def for_loop():
    for i in range(3):
        pass
    yield

# (18) Valid: Yield after loop
@contextlib.contextmanager
def while_loop():
    x = 1
    while x < 3:
        x += 1
    else:
        yield
