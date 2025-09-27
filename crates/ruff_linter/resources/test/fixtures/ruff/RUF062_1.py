import contextlib


# (1) Invalid: Variable number of yields based on conditions
@contextlib.contextmanager
def conditional_yields(condition):
    yield "first"
    if condition:
        yield "conditional"  # RUF062


# (2) Invalid(joint): Multiple nested context managers
def outer_function():
    @contextlib.contextmanager
    def inner_context_manager():  # This is fine, one yield
        yield "inner value"

    @contextlib.contextmanager
    def another_inner_cm():
        yield "first"
        yield "second"  # RUF062


# (3) Invalid: else yields after try yields
@contextlib.contextmanager
def invalid_try_else_finally():
    try:
        yield "first"
    except Exception:
        pass
    else:
        yield "second"  # RUF062
    print("done")


# (4) Invalid: Multiple yields in the same match arm
@contextlib.contextmanager
def invalid_match_same_case(value):
    match value:
        case "a":
            yield "one"
            yield "two"  # RUF062
        case _:
            yield "default"


# (5) Invalid: Yield in a loop (always ambiguous)
@contextlib.contextmanager
def invalid_yield_in_loop():
    for i in range(3):
        yield i  # RUF062


# (6) Invalid: Yields in try and finally
@contextlib.contextmanager
def invalid_try_finally():
    try:
        yield "in try"
    finally:
        yield "in finally"  # RUF062


# (7) Invalid: Multiple yields in an async context manager
@contextlib.asynccontextmanager
async def invalid_async_context_manager():
    print("Setting up async")
    yield "first async value"
    yield "second async value"  # RUF062
    print("Cleaning up async")


# (8) Invalid: Contextlib imported with an alias
import contextlib as ctx


@ctx.contextmanager
def invalid_with_import_alias():
    yield "first"
    yield "second"  # RUF062


# (9) Invalid: Direct import of contextmanager
from contextlib import contextmanager


@contextmanager
def invalid_with_direct_import():
    yield "first"
    yield "second"  # RUF062


# (10) Invalid: Multiple yields in a context manager
@contextlib.contextmanager
def invalid_multiple_yields():
    print("Setting up")
    yield "first value"
    print("In between yields")
    yield "second value"  # RUF062
    print("Cleaning up")


# (11) Invalid: Multiple yields in the same branch
@contextlib.contextmanager
def invalid_multiple_in_same_branch(condition):
    print("Setting up")
    if condition:
        yield "first in condition"
        yield "second in condition"  # RUF062
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
        yield "in ValueError"  # Unreported RUF062; only report on max yield path
    except TypeError:
        yield "in TypeError"  # Unreported RUF062; only report on max yield path
    else:
        yield "in else"  # RUF062
    finally:
        yield "in finally"  # RUF062


# (13) Invalid: Yield in for loop
@contextlib.contextmanager
def yield_in_for():
    for i in range(3):
        yield f"in loop {i}"  # RUF062 - multiple yields in loop


# (14) Invalid: Yield in while loop
@contextlib.contextmanager
def yield_in_while():
    x = 1
    while x < 3:
        yield f"in loop {x}"  # RUF062 - multiple yields in loop
        x += 1


# (15) Invalid: Unguarded yield in finally
@contextlib.contextmanager
def yield_in_finally():
    try:
        yield
    except:
        pass
    else:
        return
    finally:
        yield # RUF062


# (16) Invalid: No returning except accumulates yields
@contextlib.contextmanager
def yield_in_no_return_except():
    try:
        pass
    except:
        yield
    else:
        yield
        return
    yield # RUF062


# (17) Invalid: Return only in exception
@contextlib.contextmanager
def no_return_guarding_yield():
    try:
        yield
    except:
        return
    yield # RUF062

# (18) Invalid: No returning except accumulates yields
@contextlib.contextmanager
def multiple_yields_in_return_guarded_except():
    try:
        pass
    except RuntimeError:
        yield # valid
        return
    except ZeroDivisionError:
        yield # valid
        yield # RUF062
        return
    except IndexError:
        yield # valid
        pass
    yield # RUF062 -- Invalid yield because of IndexError branch

# (19) Invalid: yield in finally runs after except despite return
@contextlib.contextmanager
def multiple_yields_in_returning_except():
    try:
        pass
    except RuntimeError:
        pass
    except ZeroDivisionError:
        yield
        return
    finally:
        yield # RUF062, runs after ZeroDivisionError except

# (20) Invalid: multiple yields in match arm
@contextlib.contextmanager
def returning_match_case(variable):
    match variable:
        case "a":
            yield # valid
            yield # RUF062
            return
        case "b":
            pass
        case _:
            yield # valid
            return
    yield # valid

# (21) Invalid: preceeding yields makes all match yields illegal
@contextlib.contextmanager
def fully_illegal_match(variable):
    yield # valid
    match variable:
        case "a":
            yield # RUF062
            return
        case "b":
            yield # RUF062

# (22) Invalid: multiple yields in terminating branch
@contextlib.contextmanager
def multiple_yields_in_terminating_branch():
    try:
        pass
    except RuntimeError:
        yield # valid
        yield # RUF062
        return
    except ValueError:
        yield # valid
        return
    yield # valid

# (23) Invalid: preceeding yield makes all try/except yields illegal; report max path (preceeding yield, try, succeeding)
@contextlib.contextmanager
def fully_illegal_try():
    yield # valid
    try:
        yield # RUF062
    except ValueError:
        yield # RUF062
        return
    yield # RUF062


# (24) Invalid: preceeding yield makes all try/except yields illegal; except returns
@contextlib.contextmanager
def fully_illegal_try_except_returns():
    yield # valid
    try:
        pass
    except ValueError:
        yield # RUF062
        return

# (25) Invalid: preceeding yield makes all try/except yields illegal
@contextlib.contextmanager
def fully_illegal_try_except():
    yield # valid
    try:
        pass
    except ValueError:
        yield # RUF062

# (26) Invalid: preceeding yield makes all try/except yields illegal; finally returns
@contextlib.contextmanager
def fully_illegal_try_except_finally_returns():
    yield # valid
    try:
        pass
    except ValueError:
        return
    finally:
        yield # RUF062
        return

# (27) Invalid: yield from in loop
@contextlib.contextmanager
def invalid_yield_from_in_loop():
    for i in range(3):
        yield from [i]  # RUF062

# (28) Invalid: yield after invalid yield in loop
@contextlib.contextmanager
def invalid_loop_and_after():
    for i in range(2):
        yield i  # RUF062
    yield "after loop"  # RUF062


# (29) Invalid: invalid with stacked decorators
@contextlib.contextmanager
@staticmethod
def invalid_stacked_decorators():
    yield "value"
    yield "value"  # RUF062

# (30) Invalid: invalid with stacked decorators
@staticmethod
@contextlib.contextmanager
def invalid_stacked_decorators_reverse():
    yield "value"
    yield "value"  # RUF062

# (31) Invalid: yield in loop and else and after
@contextlib.contextmanager
def invalid_loop_else_and_after():
    for i in range(5):
        yield i  # RUF062
    else:
        yield "else"  # RUF062
    yield "after"  # RUF062
