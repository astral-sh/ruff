import some as sum
from some import other as int
from directory import new as dir

print = 1
copyright: 'annotation' = 2
(complex := 3)
float = object = 4
min, max = 5, 6

id = 4

def bytes():
    pass

class slice:
    pass

try:
    ...
except ImportError as ValueError:
    ...

for memoryview, *bytearray in []:
    pass

with open('file') as str, open('file2') as (all, any):
    pass

[0 for sum in ()]


# These should not report violations as discussed in
# https://github.com/astral-sh/ruff/issues/16373
from importlib.machinery import SourceFileLoader
__doc__ = "..."
__name__ = "a001"
__loader__ = SourceFileLoader(__file__, __name__)
__package__ = None
__spec__ = None