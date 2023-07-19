a = int # Ok


b = c = int  # PYI017


a.b = int  # PYI017


d, e = int, str # PYI017


f, g, h = int, str, TypeVar("T") # PYI017


i = int | str # Ok