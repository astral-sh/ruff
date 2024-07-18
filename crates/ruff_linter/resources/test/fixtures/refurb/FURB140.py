def zipped():
    return zip([1, 2, 3], "ABC")

# Errors.

# FURB140
[print(x, y) for x, y in zipped()]

# FURB140
(print(x, y) for x, y in zipped())

# FURB140
{print(x, y) for x, y in zipped()}


from itertools import starmap as sm

# FURB140
[print(x, y) for x, y in zipped()]

# FURB140
(print(x, y) for x, y in zipped())

# FURB140
{print(x, y) for x, y in zipped()}

# FURB140 (check it still flags starred arguments).
# See https://github.com/astral-sh/ruff/issues/7636
[foo(*t) for t in [(85, 60), (100, 80)]]
(foo(*t) for t in [(85, 60), (100, 80)])
{foo(*t) for t in [(85, 60), (100, 80)]}

# Non-errors.

[print(x, int) for x, _ in zipped()]

[print(x, *y) for x, y in zipped()]

[print(x, y, 1) for x, y in zipped()]

[print(y, x) for x, y in zipped()]

[print(x + 1, y) for x, y in zipped()]

[print(x) for x in range(100)]

[print() for x, y in zipped()]

[print(x, end=y) for x, y in zipped()]

[print(*x, y) for x, y in zipped()]

[print(x, *y) for x, y in zipped()]

[print(*x, *y) for x, y in zipped()]

[" ".join(x)(x, y) for x, y in zipped()]

[" ".join(x)(*x) for x in zipped()]
