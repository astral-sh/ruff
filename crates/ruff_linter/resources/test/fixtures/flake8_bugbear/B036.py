from contextlib import contextmanager, asynccontextmanager
from contextlib import contextmanager as cm
import contextlib


# Errors

@contextmanager
def bad1():
    print("start")
    yield  # B036
    print("cleanup")


@contextmanager
def bad_try_without_finally():
    try:
        yield  # B036
    except Exception:
        pass


@contextmanager
def bad_with_code_after_yield():
    with other_cm():
        yield  # B036
        print("cleanup")


@contextmanager
def bad_nested_conditional():
    if condition:
        yield  # B036


@contextmanager
def bad_nested_in_loop():
    for i in range(10):
        yield  # B036


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
def bad_try_with_else():
    try:
        yield  # B036
    except Exception:
        pass
    else:
        print("no exception")


@contextmanager
def bad_yield_in_except_no_finally():
    try:
        do_something()
    except Exception:
        yield  # B036


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
