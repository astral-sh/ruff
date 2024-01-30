import os
import subprocess
import time
from pathlib import Path

# Violation cases:

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

# Violation cases for pathlib:
    
async def foo():
    Path("foo").open() # ASYNC101

async def foo():
    p = Path("foo")
    p.open() # ASYNC101

async def foo():
    with Path("foo").open() as f: # ASYNC101
        pass


async def foo() -> None:
    p = Path("foo")

    async def bar():
        p.open() # ASYNC101


# Non-violation cases for pathlib:
    

class Foo:
    def open(self):
        pass

async def foo():
    Foo().open() # OK


async def foo():
    def open():
        pass
    open() # OK

def foo():
    Path("foo").open() # OK