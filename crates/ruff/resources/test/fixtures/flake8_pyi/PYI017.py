var: int
a = var # Ok


b = c = int  # OK


a.b = int  # OK


d, e = int, str # OK


f, g, h = int, str, TypeVar("T") # OK


i: TypeAlias = int | str # Ok


j: TypeAlias = int # Ok
