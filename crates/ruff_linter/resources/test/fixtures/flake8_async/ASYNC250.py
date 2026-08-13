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

# Regression test for https://github.com/astral-sh/ruff/issues/23425
import asyncio

async def main() -> None:
    input("sync") # should emit here
    await asyncio.to_thread(lambda: input("async")) # but not here
