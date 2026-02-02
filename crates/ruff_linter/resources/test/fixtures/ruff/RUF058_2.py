from itertools import starmap
import itertools

# Errors in Python 3.14+
starmap(func, zip(a, b, c, strict=True))
starmap(func, zip(a, b, c, strict=False))
starmap(func, zip(a, b, c, strict=strict))


# No errors

starmap(func)
starmap(func, zip(a, b, c, **kwargs))
starmap(func, zip(a, b, c), foo)
starmap(func, zip(a, b, c, lorem=ipsum))
starmap(func, zip(a, b, c), lorem=ipsum)
