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

# (19) Valid: Multiple yield in control flow path guarded by return
@contextlib.contextmanager
def valid_yield_with_unreachable_return(value):
    print("Setting up")
    if value:
        yield "value"
        return
    yield "never reached"

# (20) Valid: Return in finally guards second yield
@contextlib.contextmanager
def valid_try_yield_finally_return(value):
    print("Setting up")
    if value:
        try:
            yield "try value"
        finally:
            print("Cleaning up")
            return
    yield "later yield"

# (21) Valid: Return in finally guards second yield
@contextlib.contextmanager
def valid_try_except_yield_finally_return():
    def is_true():
        # Need this to avoid unreachable last yield
        return True
    print("Setting up")
    if is_true():
        try:
            raise
        except:
            yield
        finally:
            print("Cleaning up")
            return
    yield "later yield"


# (22) Valid: Return in try guards second yield
@contextlib.contextmanager
def valid_try_except_with_return():
    try:
        yield "in try"
        return
    except Exception:
        pass
    yield

# (23) Valid: Return in finally guards second yield
@contextlib.contextmanager
def valid_try_multi_except_yield(value):
    if value:
        try:
            pass
        except ValueError:
            yield "value error"
            return
        except TypeError:
            yield "type error"
            return
        except Exception:
            pass
        else:
            yield
            return
    yield

# (24) Valid: Try-except-else with returns
@contextlib.contextmanager
def valid_try_except_else_with_returns(value):
    if value:
        try:
            pass
        except Exception:
            yield "in except"
            return
        else:
            yield "in else"
            return
    yield

# (25) Valid: Nested ifs with returns
@contextlib.contextmanager
def valid_nested_ifs_with_returns(value, another_value):
    if value:
        if not another_value:
            yield
            return
        else:
            yield
            return
    elif value and 2 == 1 :
        if 1 == 1:
            yield
        else:
            yield
        return
    else:
        if 1 == 1:
            yield
            return
        else:
            pass

    yield

# (26) Valid: Returns guard from last yield in match
@contextlib.contextmanager
def valid_match_with_returns(value):
    match value:
        case 2:
            yield
            return
        case 3:
            yield
            return
        case _:
            pass
    yield
