import os
import subprocess
import time
from pathlib import Path

# Violation cases:


async def func():
    open("foo")


async def func():
    time.sleep(1)


async def func():
    subprocess.run("foo")


async def func():
    subprocess.call("foo")


async def func():
    subprocess.foo(0)


async def func():
    os.wait4(10)


async def func():
    os.wait(12)


# Violation cases for pathlib:


async def func():
    Path("foo").open()  # ASYNC101


async def func():
    p = Path("foo")
    p.open()  # ASYNC101


async def func():
    with Path("foo").open() as f:  # ASYNC101
        pass


async def func() -> None:
    p = Path("foo")

    async def bar():
        p.open()  # ASYNC101


async def func() -> None:
    (p1, p2) = (Path("foo"), Path("bar"))

    p1.open()  # ASYNC101


# Non-violation cases for pathlib:


class Foo:
    def open(self):
        pass


async def func():
    Foo().open()  # OK


async def func():
    def open():
        pass

    open()  # OK


def func():
    Path("foo").open()  # OK
