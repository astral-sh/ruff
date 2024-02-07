# Lint
import codecs
import io
from pathlib import Path

with open('FURB129.py') as f:
    for _line in f.readlines():
        pass
    a = [line.lower() for line in f.readlines()]
    b = {line.upper() for line in f.readlines()}
    c = {line.lower(): line.upper() for line in f.readlines()}

with Path('FURB129.py').open() as f:
    for _line in f.readlines():
        pass

for _line in open('FURB129.py').readlines():
    pass

for _line in Path('FURB129.py').open().readlines():
    pass


def good1():
    f = Path('FURB129.py').open()
    for _line in f.readlines():
        pass
    f.close()


def good2(f: io.BytesIO):
    for _line in f.readlines():
        pass


# False positives
def bad(f):
    for _line in f.readlines():
        pass


def worse(f: codecs.StreamReader):
    for _line in f.readlines():
        pass


def foo():
    class A:
        def readlines(self) -> list[str]:
            return ["a", "b", "c"]

    return A()


for _line in foo().readlines():
    pass

# OK
for _line in ["a", "b", "c"]:
    pass
with open('FURB129.py') as f:
    for _line in f:
        pass
    for _line in f.readlines(10):
        pass
    for _not_line in f.readline():
        pass
