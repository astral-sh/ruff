import os
import subprocess
import time
from pathlib import Path

# Violation cases:


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
