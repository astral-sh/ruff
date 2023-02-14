import builtins
import os
import posix
from posix import abort
import sys as std_sys
import _thread
import _winapi

import pytest
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
    while True:
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
