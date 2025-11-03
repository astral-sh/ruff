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


# Ok
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

# https://github.com/astral-sh/ruff/issues/18231
with open("furb129.py") as f:
    for line in (f).readlines():
        pass

with open("furb129.py") as f:
    [line for line in (f).readlines()]


with open("furb129.py") as f:
    for line in (((f))).readlines():
        pass
    for line in(f).readlines():
        pass

    # Test case for issue #17683 (missing space before keyword)
    print([line for line in f.readlines()if True])

# https://github.com/astral-sh/ruff/issues/18843
with open("file.txt") as fp:
    for line in (  # 1
        fp.  # 3  # 2
        readlines(  # 4
        )  # 5
    ):
        ...
