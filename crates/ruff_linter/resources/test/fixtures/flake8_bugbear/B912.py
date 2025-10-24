from itertools import count, cycle, repeat

# Errors
map(lambda x: x, [1, 2, 3])
map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6])
map(lambda x, y, z: x + y + z, [1, 2, 3], [4, 5, 6], [7, 8, 9])
map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6], *map(lambda x: x, [7, 8, 9]))
map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6], *map(lambda x: x, [7, 8, 9]), strict=False)
map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6], *map(lambda x: x, [7, 8, 9], strict=True))

# Errors (limited iterators).
map(lambda x, y: x + y, [1, 2, 3], repeat(1, 1))
map(lambda x, y: x + y, [1, 2, 3], repeat(1, times=4))

import builtins
# Still an error even though it uses the qualified name
builtins.map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6])

# OK
map(lambda x: x, [1, 2, 3], strict=True)
map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6], strict=True)
map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6], strict=False)
map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6], *map(lambda x: x, [7, 8, 9]), strict=True)
map(lambda x, y: x + y, [1, 2, 3], [4, 5, 6], *map(lambda x: x, [7, 8, 9], strict=True), strict=True)

# OK (single iterable - no strict required)
map(lambda x: x, [1, 2, 3])

# OK (infinite iterators)
map(lambda x, y: x + y, [1, 2, 3], cycle([1, 2, 3]))
map(lambda x, y: x + y, [1, 2, 3], repeat(1))
map(lambda x, y: x + y, [1, 2, 3], repeat(1, times=None))
map(lambda x, y: x + y, [1, 2, 3], count())

# Regression https://github.com/astral-sh/ruff/issues/20997
map(f, *lots_of_iterators)
