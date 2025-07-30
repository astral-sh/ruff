class C: a = None
{C.a: None for C.a in "abc"}
print(C.a)

x = [None]
{x[0]: None for x[0] in "abc"}
print(x)

class C(list):
    def __getitem__(self, index, /):
        item = super().__getitem__(index)
        if isinstance(index, slice): item = tuple(item)
        return item
x = C()
{x[:0]: None for x[:0] in "abc"}
print(x)

# C420: side-effecting assignment targets
# These should NOT trigger C420 because they have side-effecting assignment targets
# See https://github.com/astral-sh/ruff/issues/19511
