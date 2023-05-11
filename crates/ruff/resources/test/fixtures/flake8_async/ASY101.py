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
