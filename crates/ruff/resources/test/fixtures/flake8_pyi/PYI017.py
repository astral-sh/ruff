a = int # Ok

b = c = int  # Ok

a.b = int  # Ok

d, e = int, str # Ok

f, g, h = int, str, TypeVar("T") # Ok

i = int | str # Ok
