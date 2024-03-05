import _thread
import builtins
import os
import posix
import sys as std_sys
import typing
from posix import abort
from typing import NoReturn

import _winapi
import pytest
import typing_extensions
from pytest import xfail as py_xfail

###
# Errors
###

# if/elif/else
def x(y):
    if not y:
        return 1
    # error


def x(y):
    if not y:
        print()  # error
    else:
        return 2


def x(y):
    if not y:
        return 1

    print()  # error


# for
def x(y):
    for i in range(10):
        if i > 10:
            return i
    # error


def x(y):
    for i in range(10):
        if i > 10:
            return i
    else:
        print()  # error


# A nonexistent function
def func_unknown(x):
    if x > 0:
        return False
    no_such_function()  # error


# A function that does return the control
def func_no_noreturn(x):
    if x > 0:
        return False
    print("", end="")  # error


###
# Non-errors
###

# raise as last return
def x(y):
    if not y:
        return 1
    raise Exception


# last line in while loop
def x(y):
    while i > 0:
        if y > 0:
            return 1
        y += 1


# exclude empty functions
def x(y):
    return None


# return inner with statement
def x(y):
    with y:
        return 1


async def function():
    async with thing as foo:
        return foo.bar


# assert as last return
def x(y):
    if not y:
        return 1
    assert False, "Sanity check"


# return value within loop
def bar1(x, y, z):
    for i in x:
        if i > y:
            break
        return z


def bar3(x, y, z):
    for i in x:
        if i > y:
            if z:
                break
        else:
            return z
        return None


def bar1(x, y, z):
    for i in x:
        if i < y:
            continue
        return z


def bar3(x, y, z):
    for i in x:
        if i < y:
            if z:
                continue
        else:
            return z
        return None


def prompts(self, foo):
    if not foo:
        return []

    for x in foo:
        yield x
        yield x + 1


# Functions that never return
def noreturn_exit(x):
    if x > 0:
        return 1
    exit()


def noreturn_quit(x):
    if x > 0:
        return 1
    quit()


def noreturn_builtins_exit(x):
    if x > 0:
        return 1
    builtins.exit()


def noreturn_builtins_quit(x):
    if x > 0:
        return 1
    builtins.quit()


def noreturn_os__exit(x):
    if x > 0:
        return 1
    os._exit(0)


def noreturn_os_abort(x):
    if x > 0:
        return 1
    os.abort()


def noreturn_posix__exit():
    if x > 0:
        return 1
    posix._exit()


def noreturn_posix_abort():
    if x > 0:
        return 1
    posix.abort()


def noreturn_posix_abort_2():
    if x > 0:
        return 1
    abort()


def noreturn_sys_exit():
    if x > 0:
        return 1
    std_sys.exit(0)


def noreturn_typing_assert_never():
    if x > 0:
        return 1
    typing.assert_never(0)


def noreturn_typing_extensions_assert_never():
    if x > 0:
        return 1
    typing_extensions.assert_never(0)


def noreturn__thread_exit():
    if x > 0:
        return 1
    _thread.exit(0)


def noreturn__winapi_exitprocess():
    if x > 0:
        return 1
    _winapi.ExitProcess(0)


def noreturn_pytest_exit():
    if x > 0:
        return 1
    pytest.exit("oof")


def noreturn_pytest_fail():
    if x > 0:
        return 1
    pytest.fail("oof")


def noreturn_pytest_skip():
    if x > 0:
        return 1
    pytest.skip("oof")


def noreturn_pytest_xfail():
    if x > 0:
        return 1
    pytest.xfail("oof")


def noreturn_pytest_xfail_2():
    if x > 0:
        return 1
    py_xfail("oof")


def nested(values):
    if not values:
        return False

    for value in values:
        print(value)


def while_true():
    while True:
        if y > 0:
            return 1
        y += 1


# match
def x(y):
    match y:
        case 0:
            return 1
        case 1:
            print()  # error


def foo(baz: str) -> str:
    return baz


def end_of_statement():
    def example():
        if True:
            return ""


    def example():
        if True:
            return ""


    def example():
        if True:
            return ""  # type: ignore


    def example():
        if True:
            return ""  ;


    def example():
        if True:
            return "" \
                ;  # type: ignore


def end_of_file():
    if False:
        return 1
    x = 2 \



# function return type annotation NoReturn
def foo(x: int) -> int:
    def bar() -> NoReturn:
        abort()
    if x == 5:
        return 5
    bar()


def foo(string: str) -> str:
    def raises(value: str) -> NoReturn:
        raise RuntimeError("something went wrong")

    match string:
        case "a":
            return "first"
        case "b":
            return "second"
        case "c":
            return "third"
        case _:
            raises(string)


def foo() -> int:
    def baz() -> int:
        return 1


    def bar() -> NoReturn:
        a = 1 + 2
        raise AssertionError("Very bad")



    if baz() > 3:
        return 1
    bar()
