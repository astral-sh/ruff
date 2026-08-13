from itertools import count, cycle, repeat

# Errors
zip("a", "b")
zip("a", "b", *zip("c"))
zip(zip("a", "b"), strict=False)
zip(zip("a", strict=True),"b")

# OK
zip(range(3), strict=True)
zip("a", "b", strict=False)
zip("a", "b", "c", strict=True)

# OK (infinite iterators).
zip([1, 2, 3], cycle("ABCDEF"))
zip([1, 2, 3], count())
zip([1, 2, 3], repeat(1))
zip([1, 2, 3], repeat(1, None))
zip([1, 2, 3], repeat(1, times=None))

# Errors (limited iterators).
zip([1, 2, 3], repeat(1, 1))
zip([1, 2, 3], repeat(1, times=4))

import builtins
# Still an error even though it uses the qualified name
builtins.zip([1, 2, 3])

# Regression https://github.com/astral-sh/ruff/issues/20997
# Ok
zip()
zip(range(3))
# Error
zip(*lot_of_iterators)
