var: int
a = var  # OK

b = c = int  # OK

a.b = int  # OK

d, e = int, str  # OK

f, g, h = int, str, TypeVar("T")  # OK

i: TypeAlias = int | str  # OK

j: TypeAlias = int  # OK
