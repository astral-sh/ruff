import codecs
import io
from pathlib import Path

# Errors
with open("FURB129.py") as f:
    for _line in f.readlines():
        pass
    a = [line.lower() for line in f.readlines()]
    b = {line.upper() for line in f.readlines()}
    c = {line.lower(): line.upper() for line in f.readlines()}

with Path("FURB129.py").open() as f:
    for _line in f.readlines():
        pass

for _line in open("FURB129.py").readlines():
    pass

for _line in Path("FURB129.py").open().readlines():
    pass


def func():
    f = Path("FURB129.py").open()
    for _line in f.readlines():
        pass
    f.close()


def func(f: io.BytesIO):
    for _line in f.readlines():
        pass


def func():
    with (open("FURB129.py") as f, foo as bar):
        for _line in f.readlines():
            pass
        for _line in bar.readlines():
            pass


import builtins


with builtins.open("FURB129.py") as f:
    for line in f.readlines():
        pass


from builtins import open as o


with o("FURB129.py") as f:
    for line in f.readlines():
        pass


# False positives
def func(f):
    for _line in f.readlines():
        pass


def func(f: codecs.StreamReader):
    for _line in f.readlines():
        pass


def func():
    class A:
        def readlines(self) -> list[str]:
            return ["a", "b", "c"]

    return A()


for _line in func().readlines():
    pass

# OK
for _line in ["a", "b", "c"]:
    pass
with open("FURB129.py") as f:
    for _line in f:
        pass
    for _line in f.readlines(10):
        pass
    for _not_line in f.readline():
        pass
