def foo():
    k = input()  # Ok
    input("hello world")  # Ok


async def foo():
    k = input()  # ASYNC250
    input("hello world")  # ASYNC250


import builtins

import fake


def foo():
    builtins.input("testing")  # Ok


async def foo():
    builtins.input("testing")  # ASYNC250
    fake.input("whatever")  # Ok
