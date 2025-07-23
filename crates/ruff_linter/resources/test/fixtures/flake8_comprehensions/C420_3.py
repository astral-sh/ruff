class C: a = None
dict.fromkeys("abc")  # OK
print(C.a)

x = [None]
dict.fromkeys("abc")  # OK
print(x)

class C(list):
    def __getitem__(self, index, /):
        item = super().__getitem__(index)
        if isinstance(index, slice): item = tuple(item)
        return item
x = C()
dict.fromkeys("abc")  # OK
print(x) 

# C420: side-effecting assignment targets
# See https://github.com/astral-sh/ruff/issues/19511
