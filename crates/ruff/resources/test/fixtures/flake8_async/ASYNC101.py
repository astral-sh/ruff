import os
import subprocess
import time


async def foo():
    open("foo")


async def foo():
    time.sleep(1)


async def foo():
    subprocess.run("foo")


async def foo():
    subprocess.call("foo")


async def foo():
    subprocess.foo(0)


async def foo():
    os.wait4(10)


async def foo():
    os.wait(12)
