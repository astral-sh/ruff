var: int
a = var  # OK

b = c = int  # PYI017

a.b = int  # PYI017

d, e = int, str  # PYI017

f, g, h = int, str, TypeVar("T")  # PYI017

i: TypeAlias = int | str  # OK

j: TypeAlias = int  # OK
