var: int
a = var # Ok


b = c = int  # PYI017


a.b = int  # PYI017


d, e = int, str # PYI017


f, g, h = int, str, TypeVar("T") # PYI017


i: TypeAlias = int | str # Ok


j: TypeAlias = int # Ok
