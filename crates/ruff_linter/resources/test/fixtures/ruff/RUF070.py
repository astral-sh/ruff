from contextlib import contextmanager, asynccontextmanager
from contextlib import contextmanager as cm
import contextlib


# Errors

@contextmanager
def bad1():
    print("start")
    yield  # RUF070
    print("cleanup")


@contextmanager
def bad_with_code_after_yield():
    with other_cm():
        yield  # RUF070
        print("cleanup")


@contextmanager
def bad_nested_conditional_not_last():
    if condition:
        yield  # RUF070
    print("after if")


@contextmanager
def bad_nested_in_loop_not_last():
    for i in range(10):
        yield  # RUF070
    print("after loop")


@contextmanager
def bad_nested_in_while_not_last():
    while condition:
        yield  # RUF070
    print("after while")


@contextmanager
def bad_nested_in_elif():
    if condition1:
        pass
    elif condition2:
        yield  # RUF070
    print("after if")


@contextmanager
def bad_nested_in_match_not_last():
    match x:
        case 1:
            yield  # RUF070
    print("after match")


# OK

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


def not_a_context_manager():
    yield


@some_other_decorator
def other_decorator():
    yield


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
def good_multiple_yields():
    try:
        yield 1
        yield 2
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


@cm
def good_aliased_import():
    yield


@contextlib.contextmanager
def good_attribute_import():
    yield


@contextmanager
def good_with_code_after_with_block():
    with other_cm():
        yield
    print("after with")


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
def good_yield_terminal_in_conditional():
    if condition:
        yield


@contextmanager
def good_yield_terminal_in_loop():
    for i in range(10):
        yield


@contextmanager
def good_yield_before_return():
    print("setup")
    yield
    return


@contextmanager
def good_yield_terminal_in_branches():
    if condition:
        yield
    else:
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


@contextmanager
def good_return_yield():
    return (yield)


@contextmanager
def bad_yield_in_with_items():
    with some_call((yield)):  # RUF070
        pass
    print("cleanup")


@contextmanager
def good_yield_in_with_items_terminal():
    with some_call((yield)):
        pass


@contextmanager
def good_yield_in_finally():
    try:
        do_something()
    finally:
        yield
