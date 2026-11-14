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


# https://github.com/astral-sh/ruff/issues/29640
# Unsafe: these operations share the file position with direct iteration.
def file_operations(fp: io.TextIOBase):
    for line in fp.readlines():
        if line.startswith("legacy_"):
            fp.seek(0, 2)
            fp.write(line.removeprefix("legacy_"))

    for line in fp.readlines():
        fp.read(1)

    for line in fp.readlines():
        print(fp.tell())

    for line in fp.readlines():
        fp.truncate()

    for line in fp.readlines():
        fp.close()

    # Operations in comprehension elements and filters are also unsafe.
    a = [fp.readline() for line in fp.readlines()]
    b = {fp.tell() for line in fp.readlines()}
    c = {line: fp.write(line) for line in fp.readlines()}
    d = [line for line in fp.readlines() if fp.seek(0)]
    e = (fp.read(1) for line in fp.readlines())


# Safe: unrelated file operations and queries that do not depend on the position.
def unrelated_operations(fp: io.TextIOBase, other: io.TextIOBase):
    for line in fp.readlines():
        other.seek(0)
        print(fp.readable())

    a = [other.readline() for line in fp.readlines()]
    b = [line for line in fp.readlines() if fp.readable()]
